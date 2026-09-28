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

/// MED3: auto-download bitflags per media type, mirroring TGX
/// `TdlibFilesManager` (`settings_autodownload` key, per-chat-type shifts):
/// PHOTO=0x01, VOICE=0x02, VIDEO=0x04, FILE=0x08, MUSIC=0x10, GIF=0x20,
/// VIDEO_NOTE=0x40. TGX defaults: PHOTO|VOICE|GIF|VIDEO_NOTE in every
/// chat kind.
pub const AUTO_DOWNLOAD_PHOTO: u8 = 0x01;
pub const AUTO_DOWNLOAD_VOICE: u8 = 0x02;
pub const AUTO_DOWNLOAD_VIDEO: u8 = 0x04;
pub const AUTO_DOWNLOAD_FILE: u8 = 0x08;
pub const AUTO_DOWNLOAD_MUSIC: u8 = 0x10;
pub const AUTO_DOWNLOAD_GIF: u8 = 0x20;
pub const AUTO_DOWNLOAD_VIDEO_NOTE: u8 = 0x40;
pub const AUTO_DOWNLOAD_DEFAULT: u8 =
    AUTO_DOWNLOAD_PHOTO | AUTO_DOWNLOAD_VOICE | AUTO_DOWNLOAD_GIF | AUTO_DOWNLOAD_VIDEO_NOTE;

/// MED3: TGX `canAutomaticallyDownload` rejects files above the download
/// limit (default 50 MiB on WiFi, steps 1/5/15/50/100/500 MiB/None). Quill
/// honors the WiFi default as a fixed cap; per-type configurable limits are
/// future work.
pub const AUTO_DOWNLOAD_MAX_BYTES: i64 = 50 * 1024 * 1024;

/// MED1: local-only media preferences, persisted as JSON next to the
/// account root (`media_prefs.json`). Client-side only (no TDLib setting):
/// - `remember_media_grouping`: when true, the composer's "group media"
///   choice is remembered between sends (TGX `RememberAlbumSetting`);
/// - `group_media`: the last-used grouping choice (only honored when
///   `remember_media_grouping` is on).
/// - `hq_round_videos`: MED2 — "Record HQ Round Videos" (TGX
///   `UseHqRoundVideos`): capture round video notes at 480px instead of 280px;
/// - `prefer_video_mode`: MED2 — the record button's mode (TGX
///   `preferVideoMode`); right-click on the record button flips it.
/// - `data_saver`: MED3 — pause-all auto-downloads (TGX `settings_datasaver`
///   master bit). User-initiated downloads are unaffected.
/// - `auto_download_private` / `auto_download_groups` /
///   `auto_download_channels`: MED3 — per-chat-kind media-type bitfields
///   (TGX `settings_autodownload` per-chat-type shifts 8/16/24). Desktop
///   has no mobile/wifi/roaming distinction, so TGX's per-connection
///   limits collapse to this single per-kind grid (Telegram Desktop's
///   own auto-download dialog is the same grid).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaPrefs {
    pub remember_media_grouping: bool,
    pub group_media: bool,
    pub hq_round_videos: bool,
    pub prefer_video_mode: bool,
    #[serde(default)]
    pub data_saver: bool,
    #[serde(default = "auto_download_default")]
    pub auto_download_private: u8,
    #[serde(default = "auto_download_default")]
    pub auto_download_groups: u8,
    #[serde(default = "auto_download_default")]
    pub auto_download_channels: u8,
    /// MED4: Instant View mode (TGX `Settings.INSTANT_VIEW_MODE`:
    /// None / Telegram-internal / All, `Settings.java:796-798`).
    /// Default `Telegram` — the reader opens for links TDLib flagged
    /// with `instant_view_version > 0`.
    #[serde(default)]
    pub instant_view_mode: InstantViewMode,
}

/// MED4: Instant View preference (TGX values 0/1/2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstantViewMode {
    /// Never attempt Instant View — links open in the browser.
    Off,
    /// Only links carrying `instant_view_version > 0` (preview cards).
    #[default]
    Telegram,
    /// Every http(s) link is tried via `getWebPageInstantView` first
    /// (TDLib 404 → browser fallback).
    All,
}

fn auto_download_default() -> u8 {
    AUTO_DOWNLOAD_DEFAULT
}

impl Default for MediaPrefs {
    fn default() -> Self {
        Self {
            remember_media_grouping: false,
            group_media: false,
            hq_round_videos: false,
            prefer_video_mode: false,
            data_saver: false,
            auto_download_private: AUTO_DOWNLOAD_DEFAULT,
            auto_download_groups: AUTO_DOWNLOAD_DEFAULT,
            auto_download_channels: AUTO_DOWNLOAD_DEFAULT,
            instant_view_mode: InstantViewMode::default(),
        }
    }
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

fn default_true() -> bool {
    true
}

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
    let path = contact_prefs_path(paths);
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
            hq_round_videos: true,
            prefer_video_mode: true,
            ..Default::default()
        };
        save_media_prefs(&paths, &prefs).expect("save works");
        assert_eq!(load_media_prefs(&paths), prefs);
        assert!(!prefs.default_grouping());
        // Remember off → grouped regardless of the stored choice.
        let prefs = MediaPrefs {
            remember_media_grouping: false,
            group_media: false,
            ..Default::default()
        };
        assert!(prefs.default_grouping());
        std::fs::write(dir.join("accounts/primary/media_prefs.json"), b"not json").unwrap();
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_download_defaults_match_tgx_and_old_files_still_load() {
        // MED3: TGX defaults (PHOTO|VOICE|GIF|VIDEO_NOTE) in every chat
        // kind; data saver off.
        let defaults = MediaPrefs::default();
        assert!(!defaults.data_saver);
        assert_eq!(defaults.auto_download_private, AUTO_DOWNLOAD_DEFAULT);
        assert_eq!(defaults.auto_download_groups, AUTO_DOWNLOAD_DEFAULT);
        assert_eq!(defaults.auto_download_channels, AUTO_DOWNLOAD_DEFAULT);
        // A media_prefs.json written before MED3 (no new fields) loads
        // with the new fields defaulted — old prefs are preserved.
        let dir =
            std::env::temp_dir().join(format!("quill-media-prefs-old-{}", std::process::id()));
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        std::fs::create_dir_all(dir.join("accounts/primary")).unwrap();
        std::fs::write(
            dir.join("accounts/primary/media_prefs.json"),
            br#"{"remember_media_grouping":true,"group_media":true,"hq_round_videos":true,"prefer_video_mode":false}"#,
        )
        .unwrap();
        let loaded = load_media_prefs(&paths);
        assert!(loaded.remember_media_grouping);
        assert!(loaded.hq_round_videos);
        assert!(!loaded.data_saver);
        assert_eq!(loaded.auto_download_private, AUTO_DOWNLOAD_DEFAULT);
        let _ = std::fs::remove_dir_all(&dir);
    }

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
