//! Contact, chat, hashtag, spellcheck, language and translation preferences.
use super::*;

/// Slice A6: local-only contacts preferences, persisted as JSON next to
/// the account root (`contacts_prefs.json`). Client-side only —
/// TDLib 1.8.67 has no contact-sync switch (concept-level check of
/// `schema/td_api.tl`; TGX implements sync client-side in
/// `TdlibContactManager`):
/// - `sync_enabled`: when true (default), opening the Contacts tab
///   refreshes the list via `getContacts`; when false, the tab shows
///   the last loaded snapshot and never syncs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactPrefs {
    #[serde(default = "default_true")]
    pub sync_enabled: bool,
}

impl Default for ContactPrefs {
    fn default() -> Self {
        Self { sync_enabled: true }
    }
}

fn contact_prefs_path(paths: &AccountPaths) -> PathBuf {
    paths.root.join("contacts_prefs.json")
}

/// Load contacts prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_contact_prefs(paths: &AccountPaths) -> ContactPrefs {
    std::fs::read(contact_prefs_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist contacts prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_contact_prefs(paths: &AccountPaths, prefs: &ContactPrefs) -> std::io::Result<()> {
    write_json_atomic(&contact_prefs_path(paths), prefs)
}

/// Chat-composer behavior prefs, persisted as JSON next to the account
/// root (`chat_prefs.json`). Client-side only (no TDLib setting):
/// - `send_key_mode`: which keystroke sends a message
///   (`composer::SendKeyMode`; parity:settings-enter-send,
///   parity:settings-ctrlenter-send).
/// - `spellcheck_enabled`: flag misspelled words in the composer
///   (macOS: system NSSpellChecker; elsewhere an English wordlist).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatPrefs {
    #[serde(default)]
    pub send_key_mode: crate::composer::SendKeyMode,
    #[serde(default = "default_true")]
    pub spellcheck_enabled: bool,
    /// Telegram Desktop's "Suggest emoji replacements" (`suggestEmoji`,
    /// default on): `:name` in the composer offers matching emoji.
    #[serde(default = "default_true")]
    pub suggest_emoji: bool,
    /// Telegram Desktop's "Replace emoji automatically" (`replaceEmoji`,
    /// default on): `:-)`, `<3` and `:name:` become emoji as you type.
    #[serde(default = "default_true")]
    pub replace_emoji: bool,
}

impl Default for ChatPrefs {
    fn default() -> Self {
        Self {
            send_key_mode: crate::composer::SendKeyMode::default(),
            // Telegram Desktop ships spellcheck on; match that.
            spellcheck_enabled: true,
            suggest_emoji: true,
            replace_emoji: true,
        }
    }
}

/// Load chat prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_chat_prefs(paths: &AccountPaths) -> ChatPrefs {
    load_json_prefs(paths, "chat_prefs.json")
}

/// Persist chat prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_chat_prefs(paths: &AccountPaths, prefs: &ChatPrefs) -> std::io::Result<()> {
    save_json_prefs(paths, "chat_prefs.json", prefs)
}

/// Load the hashtags the user has sent (`recent_hashtags.json`); missing
/// or corrupt files read as empty.
pub fn load_recent_hashtags(paths: &AccountPaths) -> crate::suggest::RecentHashtags {
    load_json_prefs(paths, "recent_hashtags.json")
}

/// Persist the recent hashtags; failures are returned to the caller.
pub fn save_recent_hashtags(
    paths: &AccountPaths,
    recent: &crate::suggest::RecentHashtags,
) -> std::io::Result<()> {
    save_json_prefs(paths, "recent_hashtags.json", recent)
}

/// Slice parity:platform-spellcheck: the user's own words ("Add to
/// dictionary"), persisted as JSON next to the account root
/// (`spellcheck_words.json`). A plain sorted Vec on disk; the engine
/// holds them in a HashSet at runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpellcheckWords {
    #[serde(default)]
    pub words: Vec<String>,
}

/// Load custom words; missing or corrupt files fall back to empty
/// (never a hard error — prefs must not block startup).
pub fn load_spellcheck_words(paths: &AccountPaths) -> SpellcheckWords {
    load_json_prefs(paths, "spellcheck_words.json")
}

/// Persist custom words; failures are returned to the caller.
pub fn save_spellcheck_words(paths: &AccountPaths, prefs: &SpellcheckWords) -> std::io::Result<()> {
    save_json_prefs(paths, "spellcheck_words.json", prefs)
}

/// The spelling dictionaries the user picked (Linux: Hunspell codes such
/// as `en_US`; tdesktop's "Settings > Advanced > Spell checker" language
/// list), persisted as `spellcheck_languages.json`. Empty follows the
/// system locale.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpellcheckLanguages {
    #[serde(default)]
    pub languages: Vec<String>,
}

/// Load the picked spelling languages; missing or corrupt files read as
/// empty (automatic).
pub fn load_spellcheck_languages(paths: &AccountPaths) -> SpellcheckLanguages {
    load_json_prefs(paths, "spellcheck_languages.json")
}

/// Persist the picked spelling languages; failures are returned.
pub fn save_spellcheck_languages(
    paths: &AccountPaths,
    prefs: &SpellcheckLanguages,
) -> std::io::Result<()> {
    save_json_prefs(paths, "spellcheck_languages.json", prefs)
}

/// Slice parity:settings-language: local-only app language preference,
/// persisted as JSON next to the account root (`language_prefs.json`).
/// `system_language_code` is the IETF tag sent in TDLib's
/// `setTdlibParameters` (hardcoded `"en"` before this slice). TDLib
/// parameters are sent once at startup, so a change takes effect on
/// restart — no runtime application is attempted:
/// `setOption("language_pack_id")` is real (schema 1.8.67 :15662) but it
/// only names a downloaded language-pack database whose strings the
/// client then fetches via `getLanguagePackString`/`updateLanguagePackStrings`
/// (schema 1.8.67 :10970); Quill's own strings are hardcoded English, so
/// the option would change nothing visible. Full UI-string translation is
/// a separate project and out of scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguagePrefs {
    /// IETF language tag, e.g. "en", "es", "zh".
    #[serde(default = "default_language_code")]
    pub system_language_code: String,
}

/// Default app language tag.
pub const DEFAULT_LANGUAGE_CODE: &str = "en";

fn default_language_code() -> String {
    DEFAULT_LANGUAGE_CODE.to_string()
}

/// TDLib `setTdlibParameters` tags offered in Settings (code, native
/// name). Language tags only — Quill's UI strings stay English.
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Español"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("ar", "العربية"),
    ("he", "עברית"),
    ("fa", "فارسی"),
    ("zh", "中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("tr", "Türkçe"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("id", "Bahasa Indonesia"),
];

impl Default for LanguagePrefs {
    fn default() -> Self {
        Self {
            system_language_code: default_language_code(),
        }
    }
}

/// Load language prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup). A stored tag
/// that isn't in `SUPPORTED_LANGUAGES` (hand-edited JSON, removed
/// entries) falls back to `DEFAULT_LANGUAGE_CODE`.
pub fn load_language_prefs(paths: &AccountPaths) -> LanguagePrefs {
    let prefs: LanguagePrefs = load_json_prefs(paths, "language_prefs.json");
    if SUPPORTED_LANGUAGES
        .iter()
        .any(|(code, _)| *code == prefs.system_language_code)
    {
        prefs
    } else {
        LanguagePrefs::default()
    }
}

/// Persist language prefs; failures are returned to the caller to
/// surface in the status note.
pub fn save_language_prefs(paths: &AccountPaths, prefs: &LanguagePrefs) -> std::io::Result<()> {
    save_json_prefs(paths, "language_prefs.json", prefs)
}

/// Load the translation prefs (`translate_prefs.json`); missing or corrupt
/// files fall back to defaults.
pub fn load_translate_prefs(paths: &AccountPaths) -> crate::translate::TranslatePrefs {
    load_json_prefs(paths, "translate_prefs.json")
}

/// Persist the translation prefs; failures are returned to the caller.
pub fn save_translate_prefs(
    paths: &AccountPaths,
    prefs: &crate::translate::TranslatePrefs,
) -> std::io::Result<()> {
    save_json_prefs(paths, "translate_prefs.json", prefs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn contact_prefs_roundtrip_and_missing_file() {
        // Slice A6: what the sync toggle is ultimately validating — the
        // stored value survives a load, and it defaults to ON.
        let dir =
            std::env::temp_dir().join(format!("quill-contact-prefs-test-{}", std::process::id()));
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert_eq!(load_contact_prefs(&paths), ContactPrefs::default());
        assert!(ContactPrefs::default().sync_enabled);
        let prefs = ContactPrefs {
            sync_enabled: false,
        };
        save_contact_prefs(&paths, &prefs).expect("save works");
        assert_eq!(load_contact_prefs(&paths), prefs);
        std::fs::write(
            dir.join("accounts/primary/contacts_prefs.json"),
            b"not json",
        )
        .unwrap();
        assert_eq!(load_contact_prefs(&paths), ContactPrefs::default());
        // A contacts_prefs.json written before the field existed (empty
        // object) still loads with sync on.
        std::fs::write(dir.join("accounts/primary/contacts_prefs.json"), b"{}").unwrap();
        assert!(load_contact_prefs(&paths).sync_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Slice parity:settings-language: the stored language tag survives
    /// a save/load roundtrip; a missing file defaults to "en"; an
    /// unsupported tag (hand-edited JSON) falls back to "en" rather
    /// than being reported to TDLib.
    #[test]
    fn language_prefs_roundtrip_default_and_sanitize() {
        let dir = std::env::temp_dir().join(format!("quill-language-prefs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        // Missing file → "en".
        assert_eq!(load_language_prefs(&paths), LanguagePrefs::default());
        assert_eq!(LanguagePrefs::default().system_language_code, "en");
        // Roundtrip.
        let prefs = LanguagePrefs {
            system_language_code: "de".to_string(),
        };
        save_language_prefs(&paths, &prefs).unwrap();
        assert_eq!(load_language_prefs(&paths), prefs);
        // Unsupported tag → default, never an error.
        fs::write(
            paths.root.join("language_prefs.json"),
            br#"{"system_language_code": "xx"}"#,
        )
        .unwrap();
        assert_eq!(load_language_prefs(&paths), LanguagePrefs::default());
        // A file from before the field existed (empty object) still loads.
        fs::write(paths.root.join("language_prefs.json"), b"{}").unwrap();
        assert_eq!(load_language_prefs(&paths), LanguagePrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
