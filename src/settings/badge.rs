//! App badge counter preferences.
use super::*;

/// Slice parity:chatlist-badge-settings: local-only app badge counter
/// preferences, persisted as JSON next to the account root
/// (`badge_prefs.json`). Client-side only — the tray badge count is a
/// desktop-client concern (TDLib 1.8.67 has no badge settings):
/// - `include_muted`: count muted chats (Telegram Desktop default: ON —
///   its `_includeMutedCounter` is true)
/// - `include_archived`: count archived chats (default ON: tdesktop folds
///   the archive into the main list as all-muted, so it counts whenever
///   muted chats do; this toggle is a Quill-only opt-out)
/// - `count_messages`: sum unread messages vs count unread chats
///   (default: messages)
/// - `include_muted_folders`: tdesktop `includeMutedCounterFolders`, the
///   same choice for the folder tab counters (default ON)
/// - `flash_bounce`: tdesktop `flashBounceNotify`, bounce the Dock icon /
///   flash the taskbar / mark the window urgent for a new message
///   (default ON). It rides along in this file because it is the other
///   per-account "how loudly do I count" preference.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadgePrefs {
    #[serde(default = "default_true")]
    pub include_muted: bool,
    #[serde(default = "default_true")]
    pub include_archived: bool,
    #[serde(default = "default_true")]
    pub count_messages: bool,
    #[serde(default = "default_true")]
    pub include_muted_folders: bool,
    #[serde(default = "default_true")]
    pub flash_bounce: bool,
}

impl Default for BadgePrefs {
    fn default() -> Self {
        Self {
            include_muted: true,
            include_archived: true,
            count_messages: true,
            include_muted_folders: true,
            flash_bounce: true,
        }
    }
}

fn badge_prefs_path(paths: &AccountPaths) -> PathBuf {
    paths.root.join("badge_prefs.json")
}

/// Load badge prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_badge_prefs(paths: &AccountPaths) -> BadgePrefs {
    std::fs::read(badge_prefs_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist badge prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_badge_prefs(paths: &AccountPaths, prefs: &BadgePrefs) -> std::io::Result<()> {
    write_json_atomic(&badge_prefs_path(paths), prefs)
}

#[cfg(test)]
mod badge_prefs_tests {
    use super::*;

    #[test]
    fn badge_prefs_default_matches_telegram_desktop() {
        let prefs = BadgePrefs::default();
        assert!(prefs.include_muted);
        assert!(prefs.include_archived);
        assert!(prefs.count_messages);
        assert!(prefs.include_muted_folders);
        assert!(prefs.flash_bounce);
    }

    #[test]
    fn old_badge_prefs_file_gets_new_defaults() {
        let prefs: BadgePrefs =
            serde_json::from_str(r#"{"include_muted":false,"include_archived":true}"#).unwrap();
        assert!(!prefs.include_muted);
        assert!(prefs.include_muted_folders && prefs.flash_bounce);
    }

    #[test]
    fn badge_prefs_serde_roundtrip() {
        let prefs = BadgePrefs {
            include_muted: false,
            include_archived: true,
            count_messages: false,
            include_muted_folders: false,
            flash_bounce: false,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<BadgePrefs>(&json).unwrap(), prefs);
    }

    #[test]
    fn badge_prefs_missing_file_falls_back_to_default() {
        let paths = AccountPaths {
            root: PathBuf::from("/nonexistent-dir-for-badge-test"),
            tdlib_database: PathBuf::new(),
            tdlib_files: PathBuf::new(),
            app_thumbnails: PathBuf::new(),
            exports: PathBuf::new(),
        };
        assert_eq!(load_badge_prefs(&paths), BadgePrefs::default());
    }
}
