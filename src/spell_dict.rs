//! Hunspell-dictionary spellcheck engine for Linux (and the dictionary
//! machinery any non-OS engine needs), after Telegram Desktop's
//! `lib_spellcheck` (`spellcheck/platform/linux/spellcheck_linux.cpp`,
//! which drives enchant over the system hunspell dictionaries, and the
//! hunspell fallback next to `spellcheck/platform/win/spellcheck_win.cpp`).
//!
//! Quill uses the pure-Rust `spellbook` crate (Hunspell-compatible,
//! MPL-2.0) instead of dlopen-ing libhunspell/libenchant: nothing to link,
//! nothing to add to the Linux bundle, and the package starts without any
//! spelling library installed. Only the dictionary *files* are external
//! (`/usr/share/hunspell/en_US.{aff,dic}` and friends).
//!
//! Everything here is pure (paths and environment values are passed in) so
//! it is unit-tested with temp dirs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::spellcheck::{
    Script, SpellBackend, SpellChecker, locale_script, spelling_languages, word_script,
};

/// A Hunspell dictionary found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryFile {
    /// Locale code with `_`: `en_US`, `de`, `pt_BR`.
    pub code: String,
    pub aff: PathBuf,
    pub dic: PathBuf,
}

/// Standard Hunspell/MySpell dictionary directories, most specific first:
/// the user's `$XDG_DATA_HOME/hunspell` (tdesktop also loads dictionaries
/// from its own working dir; this is the user-writable equivalent), each
/// `$XDG_DATA_DIRS/hunspell`, then the classic system locations.
pub fn standard_dictionary_dirs(
    home: Option<&Path>,
    xdg_data_home: Option<&str>,
    xdg_data_dirs: Option<&str>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    match xdg_data_home.filter(|s| !s.is_empty()) {
        Some(dir) => push(Path::new(dir).join("hunspell")),
        None => {
            if let Some(home) = home {
                push(home.join(".local/share/hunspell"));
            }
        }
    }
    let data_dirs = xdg_data_dirs
        .filter(|s| !s.is_empty())
        .unwrap_or("/usr/local/share:/usr/share");
    for dir in data_dirs.split(':').filter(|s| !s.is_empty()) {
        push(Path::new(dir).join("hunspell"));
    }
    push(PathBuf::from("/usr/share/hunspell"));
    push(PathBuf::from("/usr/share/myspell/dicts"));
    push(PathBuf::from("/usr/share/myspell"));
    // Flatpak runtimes.
    push(PathBuf::from("/app/share/hunspell"));
    out
}

/// [`standard_dictionary_dirs`] from the process environment.
pub fn standard_dictionary_dirs_from_env() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    standard_dictionary_dirs(
        home.as_deref(),
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
        std::env::var("XDG_DATA_DIRS").ok().as_deref(),
    )
}

/// Every `<code>.dic` with a sibling `<code>.aff` in `dirs`. The first
/// directory that has a code wins (so a user dictionary overrides the
/// system one). Hyphenation (`hyph_*`) and thesaurus (`th_*`) files are
/// not dictionaries. Sorted by code.
pub fn discover_dictionaries(dirs: &[PathBuf]) -> Vec<DictionaryFile> {
    let mut found: BTreeMap<String, DictionaryFile> = BTreeMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut local: Vec<DictionaryFile> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("dic") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with("hyph_") || stem.starts_with("th_") {
                continue;
            }
            let aff = path.with_extension("aff");
            // `metadata` follows the symlinks distros use for aliases
            // (`en_GB.dic -> en_GB-large.dic`).
            if !std::fs::metadata(&path).is_ok_and(|m| m.is_file())
                || !std::fs::metadata(&aff).is_ok_and(|m| m.is_file())
            {
                continue;
            }
            let code = stem.replace('-', "_");
            if locale_script(&code).is_none() {
                continue;
            }
            local.push(DictionaryFile {
                code,
                aff,
                dic: path,
            });
        }
        for file in local {
            found.entry(file.code.clone()).or_insert(file);
        }
    }
    found.into_values().collect()
}

/// The user's locales in preference order from environment values (the
/// glibc order: `LANGUAGE` list, then `LC_ALL`, `LC_MESSAGES`, `LANG`),
/// with encoding and modifier stripped and `C`/`POSIX` dropped:
/// `de_DE.UTF-8@euro` becomes `de_DE`.
pub fn locales_from_env(get: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let mut raw: Vec<String> = Vec::new();
    let primary = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| get(k))
        .find(|v| !v.is_empty());
    // `LANGUAGE` only applies when the locale is not C/POSIX.
    let c_locale = primary
        .as_deref()
        .is_none_or(|p| matches!(p, "C" | "POSIX") || p.starts_with("C."));
    if !c_locale && let Some(list) = get("LANGUAGE") {
        raw.extend(list.split(':').map(str::to_string));
    }
    raw.extend(primary);
    let mut out: Vec<String> = Vec::new();
    for value in raw {
        let base = value
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if base.is_empty() || base == "C" || base == "POSIX" {
            continue;
        }
        if !out.contains(&base) {
            out.push(base);
        }
    }
    out
}

/// [`locales_from_env`] from the process environment.
pub fn system_locales() -> Vec<String> {
    locales_from_env(|k| std::env::var(k).ok())
}

/// Dictionaries to use: the user's explicit picks that exist, else the
/// ones matching the system locales (tdesktop: the system language's
/// dictionary first, then the others).
pub fn pick_dictionaries(
    system: &[String],
    available: &[String],
    chosen: &[String],
) -> Vec<String> {
    let picked: Vec<String> = chosen
        .iter()
        .filter(|c| available.contains(c))
        .cloned()
        .collect();
    if !picked.is_empty() {
        return picked;
    }
    spelling_languages(system, available)
}

/// Decodes a Hunspell file: UTF-8 as is, otherwise the charset named by
/// the `.aff` `SET` line (ISO-8859-x, KOI8-R, CP125x...). `spellbook` reads
/// UTF-8 text.
fn decode_text(bytes: &[u8], charset: Option<&str>) -> Option<String> {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Some(s.strip_prefix('\u{feff}').unwrap_or(s).to_string());
    }
    let encoding = encoding_rs::Encoding::for_label(charset?.as_bytes())?;
    let (text, _, _) = encoding.decode(bytes);
    Some(text.into_owned())
}

fn aff_charset(aff: &[u8]) -> Option<String> {
    let head = &aff[..aff.len().min(4096)];
    String::from_utf8_lossy(head)
        .lines()
        .find_map(|l| l.trim().strip_prefix("SET ").map(|s| s.trim().to_string()))
}

/// Parses a dictionary from the bytes of its `.aff` and `.dic` files.
/// `None` when either is not valid Hunspell data (also used to vet a
/// download before it is installed).
pub fn parse_dictionary(aff_bytes: &[u8], dic_bytes: &[u8]) -> Option<spellbook::Dictionary> {
    let charset = aff_charset(aff_bytes);
    let aff = decode_text(aff_bytes, charset.as_deref())?;
    let dic = decode_text(dic_bytes, charset.as_deref())?;
    spellbook::Dictionary::new(&aff, &dic).ok()
}

/// Loads one dictionary from disk. `None` when unreadable or unparsable.
pub fn load_dictionary(file: &DictionaryFile) -> Option<spellbook::Dictionary> {
    let aff_bytes = std::fs::read(&file.aff).ok()?;
    let dic_bytes = std::fs::read(&file.dic).ok()?;
    parse_dictionary(&aff_bytes, &dic_bytes)
}

struct Entry {
    file: DictionaryFile,
    script: Script,
    /// Parsed on first use (a big dictionary takes a noticeable moment),
    /// which happens on the checker's background thread.
    dict: OnceLock<Option<spellbook::Dictionary>>,
}

impl Entry {
    fn dict(&self) -> Option<&spellbook::Dictionary> {
        self.dict
            .get_or_init(|| load_dictionary(&self.file))
            .as_ref()
    }
}

/// Spell checking over Hunspell dictionaries; a word is correct when any
/// dictionary of its script accepts it (tdesktop's enchant backend: any
/// validator).
pub struct HunspellBackend {
    entries: Vec<Entry>,
}

impl HunspellBackend {
    /// `files` in priority order. `None` when none has a checkable script.
    pub fn new(files: Vec<DictionaryFile>) -> Option<Self> {
        let entries: Vec<Entry> = files
            .into_iter()
            .filter_map(|file| {
                let script = locale_script(&file.code).filter(|s| s.is_checkable())?;
                Some(Entry {
                    file,
                    script,
                    dict: OnceLock::new(),
                })
            })
            .collect();
        (!entries.is_empty()).then_some(Self { entries })
    }

    /// Whether every dictionary has been parsed (or failed to).
    pub fn is_warm(&self) -> bool {
        self.entries.iter().all(|e| e.dict.get().is_some())
    }

    fn for_word(&self, word: &str) -> impl Iterator<Item = &Entry> {
        let script = word_script(word);
        self.entries
            .iter()
            .filter(move |e| Some(e.script) == script)
    }
}

impl SpellBackend for HunspellBackend {
    fn warm(&self) {
        for entry in &self.entries {
            let _ = entry.dict();
        }
    }

    fn handles(&self, script: Script, _word: &str) -> bool {
        self.entries.iter().any(|e| e.script == script)
    }

    fn is_correct(&self, word: &str) -> bool {
        let mut any_loaded = false;
        for entry in self.for_word(word) {
            let Some(dict) = entry.dict() else { continue };
            any_loaded = true;
            if dict.check(word) {
                return true;
            }
        }
        // Nothing could judge the word (unloadable dictionaries): don't
        // flag it.
        !any_loaded
    }

    fn suggestions(&self, word: &str, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for entry in self.for_word(word) {
            let Some(dict) = entry.dict() else { continue };
            let mut found = Vec::new();
            dict.suggest(word, &mut found);
            for s in found {
                if !s.is_empty() && !out.contains(&s) {
                    out.push(s);
                }
                if out.len() >= limit {
                    return out;
                }
            }
            // tdesktop stops at the first dictionary that has suggestions.
            if !out.is_empty() {
                break;
            }
        }
        out
    }
}

/// Which engine ended up checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// The OS spell checker (macOS NSSpellChecker, Windows ISpellChecker).
    System,
    /// Hunspell dictionaries from disk.
    Hunspell,
    /// No dictionary available: spell checking is off.
    None,
}

/// Judges nothing: every word is skipped.
struct NoBackend;

impl SpellBackend for NoBackend {
    fn handles(&self, _script: Script, _word: &str) -> bool {
        false
    }
    fn is_correct(&self, _word: &str) -> bool {
        true
    }
    fn suggestions(&self, _word: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

/// The fallback chain: the OS checker, else Hunspell dictionaries, else
/// nothing (`EngineKind::None`; the settings page then says why).
pub fn select_backend(
    system: Option<Arc<dyn SpellBackend>>,
    hunspell: Option<Arc<dyn SpellBackend>>,
) -> (EngineKind, Arc<dyn SpellBackend>) {
    if let Some(backend) = system {
        (EngineKind::System, backend)
    } else if let Some(backend) = hunspell {
        (EngineKind::Hunspell, backend)
    } else {
        (EngineKind::None, Arc::new(NoBackend))
    }
}

/// Parse the dictionaries off the calling thread. Until it finishes the
/// checker's background pass simply waits on the same `OnceLock`, so the UI
/// thread (which only paints cached underlines) never blocks on the parse.
fn warm_in_background(backend: &Arc<dyn SpellBackend>) {
    let backend = Arc::clone(backend);
    let _ = std::thread::Builder::new()
        .name("quill-spell-warm".into())
        .spawn(move || backend.warm());
}

/// A checker plus what the settings page shows about it.
pub struct SpellEngine {
    pub kind: EngineKind,
    pub checker: Arc<SpellChecker>,
    /// Every dictionary the user can pick (empty for the OS checkers,
    /// which own their language list).
    pub available: Vec<String>,
    /// Dictionaries in use.
    pub active: Vec<String>,
}

/// Builds the Hunspell/none engine from `dirs` (no OS checker). `chosen`
/// is the user's explicit language list (empty: follow the locale).
pub fn hunspell_engine(dirs: &[PathBuf], system: &[String], chosen: &[String]) -> SpellEngine {
    let files = discover_dictionaries(dirs);
    let available: Vec<String> = files.iter().map(|f| f.code.clone()).collect();
    let active = pick_dictionaries(system, &available, chosen);
    let ordered: Vec<DictionaryFile> = active
        .iter()
        .filter_map(|c| files.iter().find(|f| &f.code == c).cloned())
        .collect();
    let hunspell = HunspellBackend::new(ordered).map(|b| Arc::new(b) as Arc<dyn SpellBackend>);
    let (kind, backend) = select_backend(None, hunspell);
    warm_in_background(&backend);
    SpellEngine {
        kind,
        checker: Arc::new(SpellChecker::new(backend)),
        available,
        active: if kind == EngineKind::None {
            Vec::new()
        } else {
            active
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EngineKind, discover_dictionaries, hunspell_engine, locales_from_env, pick_dictionaries,
        select_backend, standard_dictionary_dirs,
    };
    use crate::spellcheck::{Script, SpellBackend, SpellChecker, WordlistBackend, checkable_words};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    /// A throwaway directory removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("quill-spell-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hunspell")
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn standard_dirs_order_and_dedup() {
        let dirs = standard_dictionary_dirs(
            Some(Path::new("/home/u")),
            None,
            Some("/opt/share:/usr/share"),
        );
        assert_eq!(dirs[0], Path::new("/home/u/.local/share/hunspell"));
        assert_eq!(dirs[1], Path::new("/opt/share/hunspell"));
        assert_eq!(
            dirs.iter()
                .filter(|d| **d == Path::new("/usr/share/hunspell"))
                .count(),
            1
        );
        assert!(dirs.contains(&PathBuf::from("/usr/share/myspell/dicts")));
        let xdg = standard_dictionary_dirs(Some(Path::new("/home/u")), Some("/x"), None);
        assert_eq!(xdg[0], Path::new("/x/hunspell"));
        assert!(xdg.contains(&PathBuf::from("/usr/local/share/hunspell")));
    }

    #[test]
    fn discovery_finds_pairs_and_user_dir_wins() {
        let root = TempDir::new();
        let user = root.path().join("user");
        let sys = root.path().join("sys");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&sys).unwrap();
        for (dir, name) in [
            (&sys, "en_US"),
            (&sys, "de_DE"),
            (&sys, "pt-BR"),
            (&user, "en_US"),
        ] {
            std::fs::write(dir.join(format!("{name}.aff")), "SET UTF-8\n").unwrap();
            std::fs::write(dir.join(format!("{name}.dic")), "0\n").unwrap();
        }
        // Not dictionaries: no .aff, hyphenation, thesaurus, unknown locale.
        std::fs::write(sys.join("fr.dic"), "0\n").unwrap();
        std::fs::write(sys.join("hyph_en_US.dic"), "x").unwrap();
        std::fs::write(sys.join("hyph_en_US.aff"), "x").unwrap();
        std::fs::write(sys.join("th_en_US.dic"), "x").unwrap();
        std::fs::write(sys.join("zz.dic"), "0\n").unwrap();
        std::fs::write(sys.join("zz.aff"), "").unwrap();
        let found = discover_dictionaries(&[user.clone(), sys.clone(), root.path().join("nope")]);
        let codes: Vec<&str> = found.iter().map(|f| f.code.as_str()).collect();
        assert_eq!(codes, ["de_DE", "en_US", "pt_BR"]);
        assert_eq!(found[1].dic, user.join("en_US.dic"));
        assert_eq!(found[2].dic, sys.join("pt-BR.dic"));
    }

    fn env(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Option<String> + 'static {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn locales_follow_glibc_order() {
        assert_eq!(
            locales_from_env(env(&[
                ("LANG", "de_DE.UTF-8@euro"),
                ("LANGUAGE", "fr_CH:de_DE:en")
            ])),
            ["fr_CH", "de_DE", "en"]
        );
        assert_eq!(
            locales_from_env(env(&[("LC_ALL", "en_GB.UTF-8"), ("LANG", "de_DE")])),
            ["en_GB"]
        );
        // LANGUAGE is ignored under the C locale.
        assert!(locales_from_env(env(&[("LANG", "C.UTF-8"), ("LANGUAGE", "de")])).is_empty());
        assert!(locales_from_env(env(&[])).is_empty());
    }

    #[test]
    fn pick_prefers_user_choice_then_locale() {
        let avail = s(&["de_DE", "en_GB", "en_US"]);
        assert_eq!(pick_dictionaries(&s(&["en_GB"]), &avail, &[]), ["en_GB"]);
        assert_eq!(
            pick_dictionaries(&s(&["en_GB"]), &avail, &s(&["de_DE", "xx"])),
            ["de_DE"]
        );
        // A stale choice (dictionary uninstalled) falls back to the locale.
        assert_eq!(
            pick_dictionaries(&s(&["de_AT"]), &avail, &s(&["xx"])),
            ["de_DE"]
        );
        assert!(pick_dictionaries(&s(&["ja_JP"]), &avail, &[]).is_empty());
    }

    #[test]
    fn backend_selection_falls_back() {
        let wl: Arc<dyn SpellBackend> = Arc::new(WordlistBackend::new());
        assert_eq!(
            select_backend(Some(wl.clone()), Some(wl.clone())).0,
            EngineKind::System
        );
        assert_eq!(select_backend(None, Some(wl)).0, EngineKind::Hunspell);
        let (kind, none) = select_backend(None, None);
        assert_eq!(kind, EngineKind::None);
        assert!(!none.handles(Script::Latin, "teh"));
    }

    #[test]
    fn hunspell_backend_checks_and_suggests() {
        let engine = hunspell_engine(&[fixtures()], &s(&["en_US"]), &[]);
        assert_eq!(engine.kind, EngineKind::Hunspell);
        assert_eq!(engine.active, ["en_US"]);
        assert!(engine.available.contains(&"de_DE".to_string()));
        let checker = &engine.checker;
        assert!(checker.is_correct("hello"));
        assert!(checker.is_correct("Hello"));
        assert!(checker.is_correct("worlds"), "suffix rule");
        assert!(!checker.is_correct("wrold"));
        assert!(
            checker
                .suggestions("wrold", 5)
                .contains(&"world".to_string())
        );
        // Other-script words are never judged by a Latin dictionary.
        assert!(checker.is_correct("привет"));
    }

    #[test]
    fn explicit_language_choice_uses_that_dictionary() {
        let engine = hunspell_engine(&[fixtures()], &s(&["en_US"]), &s(&["de_DE"]));
        assert_eq!(engine.active, ["de_DE"]);
        assert!(engine.checker.is_correct("Hallo"));
        assert!(!engine.checker.is_correct("hello"));
    }

    #[test]
    fn missing_dictionaries_disable_checking() {
        let engine = hunspell_engine(&[PathBuf::from("/nonexistent/dicts")], &s(&["en_US"]), &[]);
        assert_eq!(engine.kind, EngineKind::None);
        assert!(engine.available.is_empty() && engine.active.is_empty());
        assert!(engine.checker.check_text("wrold teh").is_empty());
    }

    /// Manual timing probe: `cargo test --lib -- --ignored --nocapture
    /// large_dictionary_parse_time`. Parses a synthetic 400k-word
    /// dictionary (the size class of the big European ones).
    #[test]
    #[ignore = "timing probe"]
    fn large_dictionary_parse_time() {
        let dir = TempDir::new();
        let mut dic = String::from("400000\n");
        for i in 0..400_000u32 {
            dic.push_str(&format!("word{i}abc/S\n"));
        }
        let aff = "SET UTF-8\nSFX S Y 1\nSFX S 0 s .\n";
        std::fs::write(dir.path().join("xx.aff"), aff).unwrap();
        std::fs::write(dir.path().join("xx.dic"), dic).unwrap();
        let file = super::DictionaryFile {
            code: "en_US".into(),
            aff: dir.path().join("xx.aff"),
            dic: dir.path().join("xx.dic"),
        };
        let start = std::time::Instant::now();
        let loaded = super::load_dictionary(&file).is_some();
        println!("parse 400k words: {:?} (ok={loaded})", start.elapsed());
    }

    #[test]
    fn warm_parses_every_dictionary_once() {
        let files = discover_dictionaries(&[fixtures()]);
        let backend = super::HunspellBackend::new(files).expect("fixture dictionaries");
        assert!(!backend.is_warm());
        backend.warm();
        assert!(backend.is_warm());
    }

    #[test]
    fn legacy_charset_dictionary_loads() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("de.aff"), "SET ISO8859-1\n").unwrap();
        // "Grüße" in Latin-1.
        let mut dic = b"1\nGr".to_vec();
        dic.extend([0xFC, 0xDF, b'e', b'\n']);
        std::fs::write(dir.path().join("de.dic"), dic).unwrap();
        let engine = hunspell_engine(&[dir.path().to_path_buf()], &s(&["de"]), &[]);
        assert_eq!(engine.kind, EngineKind::Hunspell);
        assert!(engine.checker.is_correct("Grüße"));
        assert!(!engine.checker.is_correct("Gruße"));
    }

    /// Word segmentation is backend-independent: the same text yields the
    /// same flagged words whichever engine judges them.
    #[test]
    fn tokenization_is_shared_across_backends() {
        let text =
            "Hello wrold, don't visit https://x.org/wrold or @wrold #wrold `wrold` end.wrold";
        let words: Vec<&str> = checkable_words(text)
            .into_iter()
            .map(|r| &text[r])
            .collect();
        assert!(words.contains(&"don't"));
        let hun = hunspell_engine(&[fixtures()], &s(&["en_US"]), &[]).checker;
        let wl = SpellChecker::wordlist();
        let flagged = |c: &SpellChecker| {
            c.check_text(text)
                .iter()
                .map(|m| m.word.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(flagged(&hun), ["wrold", "wrold"]);
        assert_eq!(flagged(&hun), flagged(&wl));
    }
}
