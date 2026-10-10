//! Spelling dictionary manager core (parity:appearance-dictionaries),
//! after Telegram Desktop's `boxes/dictionaries_manager.cpp`.
//!
//! tdesktop lists the spell-checker languages, shows each one's state
//! ("Download size", progress, "Enabled"), downloads a dictionary when the
//! user switches it on, and offers "Remove dictionary" in a context menu.
//! There, dictionaries are zip files Telegram hosts and MTProto serves;
//! TDLib cannot fetch them, so Quill downloads the Hunspell `.aff`/`.dic`
//! pair of a pinned revision of the open `wooorm/dictionaries` collection
//! over HTTPS instead. Installing writes `<code>.aff`/`<code>.dic` into
//! Quill's own dictionaries folder, which `spell_dict::discover_dictionaries`
//! already searches before the system folders. The engine therefore needs
//! no change: install, enable and remove only move files and the picked
//! language list (`spellcheck_languages.json`).
//!
//! Everything here is pure or works on a directory passed in, so it is
//! tested with temp dirs. The network part is `spell_download`.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};

use crate::spell_dict::parse_dictionary;

/// The `wooorm/dictionaries` commit the catalog sizes were read from. A
/// pinned revision keeps what is downloaded reproducible; bump it together
/// with `CATALOG`.
pub const CATALOG_REV: &str = "8cfea406b505e4d7df52d5a19bce525df98c54ab";

/// Host the dictionaries come from. Only this host is ever contacted.
pub const DOWNLOAD_HOST: &str = "raw.githubusercontent.com";

/// Upper bound for one downloaded file. The largest catalog file is about
/// 12 MB; a bigger answer is not a dictionary.
pub const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;

/// One downloadable dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The code it is installed under (`en_US`, `de`, `pt_BR`): the file
    /// stem in the dictionaries folder and the id in the picked list.
    pub code: &'static str,
    /// Its folder name in the collection (`en`, `de-AT`).
    pub source: &'static str,
    /// English display name.
    pub name: &'static str,
    /// `.aff` plus `.dic` size in bytes (tdesktop's "Download size").
    pub bytes: u64,
}

/// The dictionaries Quill offers, sorted by name. Only languages whose
/// script `spellcheck::locale_script` knows are listed, since the engine
/// would skip the others anyway; dictionaries above 16 MB are left out.
pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        code: "hy",
        source: "hy",
        name: "Armenian",
        bytes: 2236399,
    },
    CatalogEntry {
        code: "eu",
        source: "eu",
        name: "Basque",
        bytes: 5032811,
    },
    CatalogEntry {
        code: "bg",
        source: "bg",
        name: "Bulgarian",
        bytes: 1624431,
    },
    CatalogEntry {
        code: "ca",
        source: "ca",
        name: "Catalan",
        bytes: 2926530,
    },
    CatalogEntry {
        code: "ca_valencia",
        source: "ca-valencia",
        name: "Catalan (Valencian)",
        bytes: 2931226,
    },
    CatalogEntry {
        code: "hr",
        source: "hr",
        name: "Croatian",
        bytes: 828688,
    },
    CatalogEntry {
        code: "cs",
        source: "cs",
        name: "Czech",
        bytes: 3760848,
    },
    CatalogEntry {
        code: "da",
        source: "da",
        name: "Danish",
        bytes: 3800884,
    },
    CatalogEntry {
        code: "nl",
        source: "nl",
        name: "Dutch",
        bytes: 2535898,
    },
    CatalogEntry {
        code: "en_AU",
        source: "en-AU",
        name: "English (Australia)",
        bytes: 557422,
    },
    CatalogEntry {
        code: "en_CA",
        source: "en-CA",
        name: "English (Canada)",
        bytes: 554669,
    },
    CatalogEntry {
        code: "en_ZA",
        source: "en-ZA",
        name: "English (South Africa)",
        bytes: 632249,
    },
    CatalogEntry {
        code: "en_GB",
        source: "en-GB",
        name: "English (United Kingdom)",
        bytes: 555192,
    },
    CatalogEntry {
        code: "en_US",
        source: "en",
        name: "English (United States)",
        bytes: 554848,
    },
    CatalogEntry {
        code: "et",
        source: "et",
        name: "Estonian",
        bytes: 4697457,
    },
    CatalogEntry {
        code: "fo",
        source: "fo",
        name: "Faroese",
        bytes: 1886312,
    },
    CatalogEntry {
        code: "fr",
        source: "fr",
        name: "French",
        bytes: 1429003,
    },
    CatalogEntry {
        code: "gl",
        source: "gl",
        name: "Galician",
        bytes: 9796316,
    },
    CatalogEntry {
        code: "ka",
        source: "ka",
        name: "Georgian",
        bytes: 3948156,
    },
    CatalogEntry {
        code: "de",
        source: "de",
        name: "German",
        bytes: 1137393,
    },
    CatalogEntry {
        code: "de_AT",
        source: "de-AT",
        name: "German (Austria)",
        bytes: 1141021,
    },
    CatalogEntry {
        code: "de_CH",
        source: "de-CH",
        name: "German (Switzerland)",
        bytes: 1138909,
    },
    CatalogEntry {
        code: "he",
        source: "he",
        name: "Hebrew",
        bytes: 5766994,
    },
    CatalogEntry {
        code: "hu",
        source: "hu",
        name: "Hungarian",
        bytes: 3961688,
    },
    CatalogEntry {
        code: "is",
        source: "is",
        name: "Icelandic",
        bytes: 2763872,
    },
    CatalogEntry {
        code: "ga",
        source: "ga",
        name: "Irish",
        bytes: 1602866,
    },
    CatalogEntry {
        code: "it",
        source: "it",
        name: "Italian",
        bytes: 1365774,
    },
    CatalogEntry {
        code: "lv",
        source: "lv",
        name: "Latvian",
        bytes: 1975306,
    },
    CatalogEntry {
        code: "lt",
        source: "lt",
        name: "Lithuanian",
        bytes: 1280898,
    },
    CatalogEntry {
        code: "lb",
        source: "lb",
        name: "Luxembourgish",
        bytes: 3783157,
    },
    CatalogEntry {
        code: "mk",
        source: "mk",
        name: "Macedonian",
        bytes: 2461397,
    },
    CatalogEntry {
        code: "ne",
        source: "ne",
        name: "Nepali",
        bytes: 946122,
    },
    CatalogEntry {
        code: "nb",
        source: "nb",
        name: "Norwegian Bokmal",
        bytes: 5367105,
    },
    CatalogEntry {
        code: "nn",
        source: "nn",
        name: "Norwegian Nynorsk",
        bytes: 3352250,
    },
    CatalogEntry {
        code: "fa",
        source: "fa",
        name: "Persian",
        bytes: 2581430,
    },
    CatalogEntry {
        code: "pl",
        source: "pl",
        name: "Polish",
        bytes: 4952403,
    },
    CatalogEntry {
        code: "pt_BR",
        source: "pt",
        name: "Portuguese (Brazil)",
        bytes: 5457481,
    },
    CatalogEntry {
        code: "pt_PT",
        source: "pt-PT",
        name: "Portuguese (Portugal)",
        bytes: 1570417,
    },
    CatalogEntry {
        code: "ro",
        source: "ro",
        name: "Romanian",
        bytes: 2257434,
    },
    CatalogEntry {
        code: "ru",
        source: "ru",
        name: "Russian",
        bytes: 3544427,
    },
    CatalogEntry {
        code: "sr",
        source: "sr",
        name: "Serbian",
        bytes: 4449266,
    },
    CatalogEntry {
        code: "sk",
        source: "sk",
        name: "Slovak",
        bytes: 3601445,
    },
    CatalogEntry {
        code: "sl",
        source: "sl",
        name: "Slovenian",
        bytes: 3099502,
    },
    CatalogEntry {
        code: "es",
        source: "es",
        name: "Spanish",
        bytes: 873337,
    },
    CatalogEntry {
        code: "es_AR",
        source: "es-AR",
        name: "Spanish (Argentina)",
        bytes: 867495,
    },
    CatalogEntry {
        code: "es_CL",
        source: "es-CL",
        name: "Spanish (Chile)",
        bytes: 856042,
    },
    CatalogEntry {
        code: "es_CO",
        source: "es-CO",
        name: "Spanish (Colombia)",
        bytes: 900318,
    },
    CatalogEntry {
        code: "es_MX",
        source: "es-MX",
        name: "Spanish (Mexico)",
        bytes: 872058,
    },
    CatalogEntry {
        code: "es_PE",
        source: "es-PE",
        name: "Spanish (Peru)",
        bytes: 863780,
    },
    CatalogEntry {
        code: "es_US",
        source: "es-US",
        name: "Spanish (United States)",
        bytes: 855897,
    },
    CatalogEntry {
        code: "es_VE",
        source: "es-VE",
        name: "Spanish (Venezuela)",
        bytes: 852783,
    },
    CatalogEntry {
        code: "sv",
        source: "sv",
        name: "Swedish",
        bytes: 2362780,
    },
    CatalogEntry {
        code: "sv_FI",
        source: "sv-FI",
        name: "Swedish (Finland)",
        bytes: 2357945,
    },
    CatalogEntry {
        code: "tr",
        source: "tr",
        name: "Turkish",
        bytes: 9358006,
    },
    CatalogEntry {
        code: "uk",
        source: "uk",
        name: "Ukrainian",
        bytes: 8696386,
    },
    CatalogEntry {
        code: "vi",
        source: "vi",
        name: "Vietnamese",
        bytes: 40939,
    },
    CatalogEntry {
        code: "cy",
        source: "cy",
        name: "Welsh",
        bytes: 896140,
    },
];

/// The catalog entry for an install code.
pub fn entry(code: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.code == code)
}

/// `https` URL of one of the two files (`ext` is `aff` or `dic`).
pub fn download_url(entry: &CatalogEntry, ext: &str) -> String {
    format!(
        "https://{DOWNLOAD_HOST}/wooorm/dictionaries/{CATALOG_REV}/dictionaries/{}/index.{ext}",
        entry.source
    )
}

/// Quill's own dictionaries folder under the app data root.
pub fn managed_dir(app_root: &Path) -> PathBuf {
    app_root.join("dictionaries")
}

/// `dirs` with the managed folder first, so a downloaded dictionary
/// overrides a system one of the same code.
pub fn with_managed_dir(app_root: Option<&Path>, dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    let Some(root) = app_root else { return dirs };
    let managed = managed_dir(root);
    let mut out = vec![managed.clone()];
    out.extend(dirs.into_iter().filter(|d| *d != managed));
    out
}

/// Codes of the dictionaries installed in the managed folder.
pub fn installed_codes(dir: &Path) -> BTreeSet<String> {
    crate::spell_dict::discover_dictionaries(&[dir.to_path_buf()])
        .into_iter()
        .map(|f| f.code)
        .collect()
}

/// Why an install did not happen.
#[derive(Debug)]
pub enum InstallError {
    /// Not a code in the catalog.
    UnknownCode,
    /// One of the files is empty, too large or not a Hunspell dictionary.
    Invalid,
    Io(io::Error),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::UnknownCode => f.write_str("Unknown dictionary."),
            InstallError::Invalid => f.write_str("The downloaded dictionary is not valid."),
            InstallError::Io(err) => write!(f, "Couldn't save the dictionary: {err}"),
        }
    }
}

/// Vet a downloaded pair and move it into `dir`. Each file is written to a
/// temporary name and renamed, `.dic` last, so a dictionary the engine can
/// see (it needs both) is always complete.
pub fn install(dir: &Path, code: &str, aff: &[u8], dic: &[u8]) -> Result<(), InstallError> {
    if entry(code).is_none() {
        return Err(InstallError::UnknownCode);
    }
    if aff.is_empty()
        || dic.is_empty()
        || aff.len() as u64 > MAX_FILE_BYTES
        || dic.len() as u64 > MAX_FILE_BYTES
        || parse_dictionary(aff, dic).is_none()
    {
        return Err(InstallError::Invalid);
    }
    std::fs::create_dir_all(dir).map_err(InstallError::Io)?;
    for (ext, bytes) in [("aff", aff), ("dic", dic)] {
        let tmp = dir.join(format!(".{code}.{ext}.part"));
        let target = dir.join(format!("{code}.{ext}"));
        std::fs::write(&tmp, bytes).map_err(InstallError::Io)?;
        if let Err(err) = std::fs::rename(&tmp, &target) {
            let _ = std::fs::remove_file(&tmp);
            return Err(InstallError::Io(err));
        }
    }
    Ok(())
}

/// Delete the managed copy of `code` (tdesktop "Remove dictionary").
/// Only catalog codes are accepted, so no path can leave `dir`. `false`
/// when nothing was installed.
pub fn remove(dir: &Path, code: &str) -> io::Result<bool> {
    if entry(code).is_none() {
        return Ok(false);
    }
    let mut removed = false;
    for ext in ["dic", "aff"] {
        match std::fs::remove_file(dir.join(format!("{code}.{ext}"))) {
            Ok(()) => removed = true,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(removed)
}

/// The picked-language list after switching `code` on or off. An empty
/// `chosen` means "automatic", in which case the switch starts from the
/// languages in use.
pub fn next_chosen(chosen: &[String], active: &[String], code: &str, enable: bool) -> Vec<String> {
    let mut next: Vec<String> = if chosen.is_empty() {
        active.to_vec()
    } else {
        chosen.to_vec()
    };
    let at = next.iter().position(|c| c == code);
    match (enable, at) {
        (true, None) => next.push(code.to_string()),
        (false, Some(i)) => {
            next.remove(i);
        }
        _ => {}
    }
    next
}

/// What a row shows (tdesktop `BlobState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// Installed and in use.
    Enabled,
    /// Installed, switched off.
    Installed,
    /// Not installed; `bytes` is the download size.
    Available { bytes: u64 },
    /// Downloading, `percent` of the catalog size.
    Downloading { percent: u8 },
    /// The last download failed.
    Failed,
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerRow {
    pub code: String,
    pub name: String,
    pub state: RowState,
    /// Installed by Quill, so "Remove" may delete it. System dictionaries
    /// are never deleted.
    pub removable: bool,
}

impl ManagerRow {
    /// The grey line under the name (tdesktop `StateDescription`).
    pub fn status(&self) -> String {
        match self.state {
            RowState::Enabled => "Enabled".to_string(),
            RowState::Installed => "Installed".to_string(),
            RowState::Available { bytes } => format!("Download size {}", format_size(bytes)),
            RowState::Downloading { percent } => format!("Downloading {percent}%"),
            RowState::Failed => "Download failed. Click to retry.".to_string(),
        }
    }
}

/// A download in progress for the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transfer<'a> {
    pub code: &'a str,
    pub percent: u8,
}

/// Everything the list is built from.
pub struct RowInputs<'a> {
    /// Installed in the managed folder.
    pub managed: &'a BTreeSet<String>,
    /// Every dictionary the engine found (managed and system).
    pub available: &'a [String],
    /// In use.
    pub active: &'a [String],
    pub transfer: Option<Transfer<'a>>,
    pub failed: Option<&'a str>,
}

/// The rows for the manager: the catalog, then dictionaries found on the
/// system that the catalog lacks (shown by code), filtered by `query`.
pub fn build_rows(inputs: &RowInputs<'_>, query: &str) -> Vec<ManagerRow> {
    let mut rows = Vec::new();
    let state_of = |code: &str, bytes: u64| -> RowState {
        if let Some(t) = inputs.transfer.filter(|t| t.code == code) {
            RowState::Downloading { percent: t.percent }
        } else if inputs.available.iter().any(|a| a == code) {
            if inputs.active.iter().any(|a| a == code) {
                RowState::Enabled
            } else {
                RowState::Installed
            }
        } else if inputs.failed == Some(code) {
            RowState::Failed
        } else {
            RowState::Available { bytes }
        }
    };
    for e in CATALOG {
        if matches_query(e.name, e.code, query) {
            rows.push(ManagerRow {
                code: e.code.to_string(),
                name: e.name.to_string(),
                state: state_of(e.code, e.bytes),
                removable: inputs.managed.contains(e.code),
            });
        }
    }
    for code in inputs.available {
        if entry(code).is_none() && matches_query(code, code, query) {
            rows.push(ManagerRow {
                code: code.clone(),
                name: code.clone(),
                state: state_of(code, 0),
                removable: false,
            });
        }
    }
    rows
}

/// tdesktop filters by prefix of the language or country name; here a row
/// matches when the query starts any word of its name or its code.
/// Case-insensitive; an empty query matches everything.
pub fn matches_query(name: &str, code: &str, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    code.to_lowercase().starts_with(&q)
        || name
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with(&q))
        || name.to_lowercase().starts_with(&q)
}

/// Whole-percent progress of `received` of `total` bytes, capped at 99 so
/// a row only reads 100% once the files are installed.
pub fn percent(received: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    (received.saturating_mul(100) / total).min(99) as u8
}

/// "541 KB", "4.8 MB".
pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{} KB", (b / KB).round().max(1.0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "quill-spell-catalog-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/hunspell")
                .join(name),
        )
        .unwrap()
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn catalog_is_sorted_unique_and_every_script_is_known() {
        let mut codes = BTreeSet::new();
        for e in CATALOG {
            assert!(codes.insert(e.code), "duplicate {}", e.code);
            assert!(
                crate::spellcheck::locale_script(e.code).is_some(),
                "{} would be skipped by the engine",
                e.code
            );
            assert!(e.bytes > 0 && e.bytes < 16 * 1024 * 1024, "{}", e.code);
            assert!(!e.code.contains(['/', '\\', '.']), "{}", e.code);
        }
        let names: Vec<_> = CATALOG.iter().map(|e| e.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        assert_eq!(entry("en_US").unwrap().source, "en");
        assert!(entry("../x").is_none());
    }

    #[test]
    fn urls_pin_the_revision_and_use_https() {
        let en = entry("en_GB").unwrap();
        assert_eq!(
            download_url(en, "dic"),
            format!(
                "https://raw.githubusercontent.com/wooorm/dictionaries/{CATALOG_REV}/dictionaries/en-GB/index.dic"
            )
        );
        assert_eq!(CATALOG_REV.len(), 40);
    }

    #[test]
    fn managed_dir_goes_first_without_duplicates() {
        let root = Path::new("/data/quill");
        let dirs = vec![PathBuf::from("/usr/share/hunspell"), managed_dir(root)];
        let out = with_managed_dir(Some(root), dirs.clone());
        assert_eq!(out[0], managed_dir(root));
        assert_eq!(out.len(), 2);
        assert_eq!(with_managed_dir(None, dirs.clone()), dirs);
    }

    #[test]
    fn install_validates_writes_atomically_and_remove_cleans_up() {
        let tmp = TempDir::new();
        let dir = tmp.0.join("dictionaries");
        assert_eq!(
            install(&dir, "xx", b"SET UTF-8\n", b"1\nhello\n")
                .unwrap_err()
                .to_string(),
            "Unknown dictionary."
        );
        assert!(matches!(
            install(&dir, "de", b"", b"1\nhallo\n"),
            Err(InstallError::Invalid)
        ));
        assert!(matches!(
            install(&dir, "de", b"SET UTF-8\n", b"not a count\n"),
            Err(InstallError::Invalid)
        ));
        assert!(!dir.exists(), "a rejected download leaves nothing behind");

        install(&dir, "de", &fixture("de_DE.aff"), &fixture("de_DE.dic")).unwrap();
        assert_eq!(installed_codes(&dir), BTreeSet::from(["de".to_string()]));
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty());

        // The engine reads what the manager installed.
        let engine = crate::spell_dict::hunspell_engine(
            std::slice::from_ref(&dir),
            &strings(&["de_DE"]),
            &[],
        );
        assert_eq!(engine.available, strings(&["de"]));
        assert_eq!(engine.active, strings(&["de"]));

        assert!(!remove(&dir, "fr").unwrap());
        assert!(remove(&dir, "de").unwrap());
        assert!(installed_codes(&dir).is_empty());
        assert!(!remove(&dir, "de").unwrap());
        // Path tricks never reach outside the folder.
        std::fs::write(tmp.0.join("keep.dic"), b"x").unwrap();
        assert!(!remove(&dir, "../keep").unwrap());
        assert!(tmp.0.join("keep.dic").exists());
    }

    #[test]
    fn switching_a_language_starts_from_the_languages_in_use() {
        let active = strings(&["en_US"]);
        assert_eq!(
            next_chosen(&[], &active, "de", true),
            strings(&["en_US", "de"])
        );
        assert_eq!(
            next_chosen(&strings(&["de"]), &active, "de", true),
            strings(&["de"])
        );
        assert_eq!(
            next_chosen(&[], &active, "en_US", false),
            Vec::<String>::new()
        );
        assert_eq!(
            next_chosen(&strings(&["en_US", "de"]), &active, "en_US", false),
            strings(&["de"])
        );
        assert_eq!(next_chosen(&[], &active, "fr", false), active);
    }

    #[test]
    fn rows_show_state_size_and_progress() {
        let managed = BTreeSet::from(["de".to_string()]);
        let available = strings(&["de", "en_US", "tlh_XX"]);
        let active = strings(&["en_US"]);
        let rows = build_rows(
            &RowInputs {
                managed: &managed,
                available: &available,
                active: &active,
                transfer: Some(Transfer {
                    code: "fr",
                    percent: 42,
                }),
                failed: Some("it"),
            },
            "",
        );
        let by = |code: &str| rows.iter().find(|r| r.code == code).unwrap();
        assert_eq!(by("en_US").state, RowState::Enabled);
        assert!(!by("en_US").removable, "a system copy is never removed");
        assert_eq!(by("de").state, RowState::Installed);
        assert!(by("de").removable);
        assert_eq!(by("fr").status(), "Downloading 42%");
        assert_eq!(by("it").state, RowState::Failed);
        assert!(by("es").status().starts_with("Download size "));
        // A system dictionary outside the catalog is listed by its code.
        assert_eq!(by("tlh_XX").name, "tlh_XX");
        assert_eq!(rows.len(), CATALOG.len() + 1);
    }

    #[test]
    fn filter_matches_name_words_and_codes() {
        assert!(matches_query("English (United States)", "en_US", "eng"));
        assert!(matches_query("English (United States)", "en_US", "UNITED"));
        assert!(matches_query("German", "de", "de"));
        assert!(matches_query("German", "de", "  "));
        assert!(!matches_query("German", "de", "fr"));
        assert!(!matches_query("German", "de", "rman"));
        let managed = BTreeSet::new();
        let rows = build_rows(
            &RowInputs {
                managed: &managed,
                available: &[],
                active: &[],
                transfer: None,
                failed: None,
            },
            "span",
        );
        assert!(!rows.is_empty() && rows.iter().all(|r| r.name.starts_with("Spanish")));
    }

    #[test]
    fn sizes_and_percent() {
        assert_eq!(format_size(554_848), "542 KB");
        assert_eq!(format_size(5_032_811), "4.8 MB");
        assert_eq!(format_size(10), "1 KB");
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(500, 200), 99);
        assert_eq!(percent(1, 0), 0);
    }
}
