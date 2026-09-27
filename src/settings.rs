//! Small non-message preferences. Message history lives in TDLib.

use crate::ids::AccountKey;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "Quill";
pub const PREFS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preferences {
    pub version: u32,
    pub account: AccountKey,
    /// Application API credentials are never stored here. See env / local untracked file.
    pub hide_notification_previews: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: PREFS_VERSION,
            account: AccountKey::primary(),
            hide_notification_previews: true,
        }
    }
}

/// Phase C2i: local-only call preferences, persisted as JSON next to
/// the account root (`call_prefs.json`). These are client-side (no
/// TDLib setting exists for them):
/// - `confirm_before_calling`: ask before placing an outgoing call;
/// - `less_data_for_calls`: stored and shown; the native call engine
///   (ntgcalls) exposes no data-saving API, so it currently has no
///   media effect — the settings UI says so honestly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPrefs {
    pub confirm_before_calling: bool,
    pub less_data_for_calls: bool,
}

fn call_prefs_path(paths: &AccountPaths) -> PathBuf {
    paths.root.join("call_prefs.json")
}

/// Load call prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_call_prefs(paths: &AccountPaths) -> CallPrefs {
    std::fs::read(call_prefs_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist call prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_call_prefs(paths: &AccountPaths, prefs: &CallPrefs) -> std::io::Result<()> {
    let path = call_prefs_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

/// MED1: local-only media preferences, persisted as JSON next to the
/// account root (`media_prefs.json`). Client-side only (no TDLib setting):
/// - `remember_media_grouping`: when true, the composer's "group media"
///   choice is remembered between sends (TGX `RememberAlbumSetting`);
/// - `group_media`: the last-used grouping choice (only honored when
///   `remember_media_grouping` is on).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaPrefs {
    pub remember_media_grouping: bool,
    pub group_media: bool,
}

impl MediaPrefs {
    /// Effective grouping for a fresh composer: the remembered choice when
    /// remembering is on, grouped (the historical behavior) otherwise.
    pub fn default_grouping(&self) -> bool {
        if self.remember_media_grouping {
            self.group_media
        } else {
            true
        }
    }
}

fn media_prefs_path(paths: &AccountPaths) -> PathBuf {
    paths.root.join("media_prefs.json")
}

/// Load media prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_media_prefs(paths: &AccountPaths) -> MediaPrefs {
    std::fs::read(media_prefs_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist media prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_media_prefs(paths: &AccountPaths, prefs: &MediaPrefs) -> std::io::Result<()> {
    let path = media_prefs_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

#[derive(Debug, Clone)]
pub struct AccountPaths {
    pub root: PathBuf,
    pub tdlib_database: PathBuf,
    pub tdlib_files: PathBuf,
    pub app_thumbnails: PathBuf,
    pub exports: PathBuf,
}

impl AccountPaths {
    pub fn for_root(app_root: &Path, account: &AccountKey) -> Self {
        let root = app_root.join("accounts").join(&account.0);
        Self {
            tdlib_database: root.join("tdlib"),
            tdlib_files: root.join("files"),
            app_thumbnails: root.join("thumbnails"),
            exports: root.join("exports"),
            root,
        }
    }

    pub fn database_exists(&self) -> bool {
        self.tdlib_database.join("db.sqlite").exists()
            || self.tdlib_database.join("td.binlog").exists()
            || directory_nonempty(&self.tdlib_database)
    }
}

fn directory_nonempty(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .map(|mut it| it.next().is_some())
        .unwrap_or(false)
}

pub fn default_app_root() -> PathBuf {
    directories::ProjectDirs::from("org", "shinycake", APP_DIR_NAME)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join("quill-data"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn account_paths_are_scoped() {
        let root = PathBuf::from("/tmp/quill-test");
        let paths = AccountPaths::for_root(&root, &AccountKey::primary());
        assert!(paths.tdlib_database.ends_with("accounts/primary/tdlib"));
        assert!(paths.tdlib_files.ends_with("accounts/primary/files"));
    }

    #[test]
    fn call_prefs_roundtrip_and_missing_file() {
        // Phase C2i: what the toggle round-trip is ultimately
        // validating — the stored value survives a load.
        let dir = std::env::temp_dir().join(format!("quill-prefs-test-{}", std::process::id()));
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        // Missing file → defaults, never an error.
        assert_eq!(load_call_prefs(&paths), CallPrefs::default());
        let prefs = CallPrefs {
            confirm_before_calling: true,
            less_data_for_calls: true,
        };
        save_call_prefs(&paths, &prefs).expect("save works");
        assert_eq!(load_call_prefs(&paths), prefs);
        // Corrupt file → defaults, never a panic.
        std::fs::write(dir.join("accounts/primary/call_prefs.json"), b"not json").unwrap();
        assert_eq!(load_call_prefs(&paths), CallPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn media_prefs_roundtrip_and_default_grouping() {
        // MED1: the stored grouping choice survives a load, and the
        // effective default follows the remember flag.
        let dir =
            std::env::temp_dir().join(format!("quill-media-prefs-test-{}", std::process::id()));
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        assert!(MediaPrefs::default().default_grouping());
        let prefs = MediaPrefs {
            remember_media_grouping: true,
            group_media: false,
        };
        save_media_prefs(&paths, &prefs).expect("save works");
        assert_eq!(load_media_prefs(&paths), prefs);
        assert!(!prefs.default_grouping());
        // Remember off → grouped regardless of the stored choice.
        let prefs = MediaPrefs {
            remember_media_grouping: false,
            group_media: false,
        };
        assert!(prefs.default_grouping());
        std::fs::write(dir.join("accounts/primary/media_prefs.json"), b"not json").unwrap();
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_dir_is_not_an_existing_database() {
        let dir = std::env::temp_dir().join(format!("quill-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("accounts/primary/tdlib")).unwrap();
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert!(!paths.database_exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
