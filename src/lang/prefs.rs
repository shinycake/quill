//! The chosen interface language, app-wide like tdesktop's (one choice for
//! every account), and the strings of the last run so the first frame is
//! already translated.
//!
//! Files in the app data folder: `language.json` (the choice),
//! `lang_cache.json` (the pack's strings Quill uses) and `langpack.sqlite`
//! (TDLib's `language_pack_database_path`, shared by every account).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::parse::parse_strings;
use super::store::{PackValue, Snapshot};

const CHOICE_FILE: &str = "language.json";
const CACHE_FILE: &str = "lang_cache.json";
const RECENT_FILE: &str = "language_recent.json";
/// TDLib's language pack database, shared by every account.
pub const DATABASE_FILE: &str = "langpack.sqlite";

/// The pack the person chose in Settings → Language. An empty `pack_id`
/// is the built-in English.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageChoice {
    #[serde(default)]
    pub pack_id: String,
    #[serde(default)]
    pub base_pack_id: String,
    #[serde(default)]
    pub plural_code: String,
    #[serde(default)]
    pub is_rtl: bool,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub native_name: String,
}

impl LanguageChoice {
    /// Whether this is the built-in English (no pack to load).
    pub fn is_builtin(&self) -> bool {
        self.pack_id.is_empty() || self.pack_id == "en"
    }
}

/// Packs used before, newest first (tdesktop's recent languages): the
/// Language box lists the unofficial ones above the official list.
pub fn load_recent(root: &Path) -> Vec<LanguageChoice> {
    std::fs::read(root.join(RECENT_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_recent(root: &Path, recent: &[LanguageChoice]) -> std::io::Result<()> {
    crate::settings::write_json_atomic(&root.join(RECENT_FILE), &recent)
}

/// `recent` with `choice` moved to the front (at most 32 entries).
pub fn with_recent(recent: &[LanguageChoice], choice: &LanguageChoice) -> Vec<LanguageChoice> {
    let mut out = vec![choice.clone()];
    out.extend(
        recent
            .iter()
            .filter(|c| c.pack_id != choice.pack_id)
            .take(31)
            .cloned(),
    );
    out
}

/// The strings of the chosen pack as they were last installed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Cache {
    pack_id: String,
    strings: BTreeMap<String, PackValue>,
}

/// `<app data>/langpack.sqlite`.
pub fn database_path(root: &Path) -> PathBuf {
    root.join(DATABASE_FILE)
}

/// The saved choice; missing or unreadable files are English.
pub fn load_choice(root: &Path) -> LanguageChoice {
    std::fs::read(root.join(CHOICE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_choice(root: &Path, choice: &LanguageChoice) -> std::io::Result<()> {
    crate::settings::write_json_atomic(&root.join(CHOICE_FILE), choice)
}

/// Builds the snapshot of `choice` from a pack's strings and its base
/// pack's (TDLib `getLanguagePackStrings` answers), with Quill's community
/// overlay below them.
pub fn build_snapshot(
    choice: &LanguageChoice,
    base: &[(String, PackValue)],
    pack: &[(String, PackValue)],
) -> Snapshot {
    let mut snapshot = Snapshot::new(&choice.pack_id, &choice.plural_code, choice.is_rtl);
    for code in [&choice.base_pack_id, &choice.pack_id] {
        if let Some(entries) = overlay_entries(code) {
            snapshot.layer_entries(&entries);
        }
    }
    snapshot.layer(base.iter().map(|(k, v)| (k.as_str(), v)));
    snapshot.layer(pack.iter().map(|(k, v)| (k.as_str(), v)));
    snapshot
}

/// The community overlay of a pack (`lang/<code>/*.strings`), if Quill
/// ships one.
fn overlay_entries(code: &str) -> Option<Vec<super::parse::Entry>> {
    if code.is_empty() {
        return None;
    }
    let code = code.to_ascii_lowercase();
    let mut out = Vec::new();
    for (lang, text) in super::key_table::OVERLAYS {
        if *lang == code
            && let Ok(entries) = parse_strings(text)
        {
            out.extend(entries);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Writes the installed strings so the next launch starts translated.
pub fn save_cache(root: &Path, snapshot: &Snapshot) -> std::io::Result<()> {
    save_cache_entries(root, snapshot.pack_id(), snapshot.entries())
}

/// [`save_cache`] from [`Snapshot::entries`] taken on another thread.
pub fn save_cache_entries(
    root: &Path,
    pack_id: &str,
    entries: Vec<(String, PackValue)>,
) -> std::io::Result<()> {
    let cache = Cache {
        pack_id: pack_id.to_string(),
        strings: entries.into_iter().collect(),
    };
    crate::settings::write_json_atomic(&root.join(CACHE_FILE), &cache)
}

/// The snapshot of the saved choice from the last run's cache; `None` for
/// English or when there is no cache for that pack (TDLib then answers
/// from its own database moments later).
pub fn load_cached_snapshot(root: &Path) -> Option<Snapshot> {
    let choice = load_choice(root);
    if choice.is_builtin() {
        return None;
    }
    let cache: Cache = std::fs::read(root.join(CACHE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())?;
    if cache.pack_id != choice.pack_id {
        return None;
    }
    let strings: Vec<(String, PackValue)> = cache.strings.into_iter().collect();
    Some(build_snapshot(&choice, &[], &strings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_and_cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("quill-lang-prefs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_choice(&dir).is_builtin());
        let choice = LanguageChoice {
            pack_id: "pt-br".into(),
            plural_code: "pt".into(),
            name: "Portuguese (Brazil)".into(),
            native_name: "Português (Brasil)".into(),
            ..Default::default()
        };
        save_choice(&dir, &choice).unwrap();
        assert_eq!(load_choice(&dir), choice);
        let name = crate::lang::keys::LANGUAGES.name().to_string();
        let snapshot = build_snapshot(
            &choice,
            &[],
            &[(name.clone(), PackValue::Ordinary("Olá".into()))],
        );
        save_cache(&dir, &snapshot).unwrap();
        let loaded = load_cached_snapshot(&dir).unwrap();
        assert_eq!(loaded.pack_id(), "pt-br");
        assert_eq!(
            loaded.entries(),
            vec![(name, PackValue::Ordinary("Olá".into()))]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
