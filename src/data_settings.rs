//! Settings → Data & Storage domain (TGX `SettingsDataStorage`):
//! per-network auto-download settings pushed to TDLib via
//! `setAutoDownloadSettings`, plus the per-chat storage rows the usage
//! section renders.
//!
//! TDLib 1.8.67 offers `getAutoDownloadSettingsPresets` but no getter
//! for the *current* per-network settings, so Quill keeps the local
//! per-account copy (`data_storage.json`) as the source of truth: it is
//! seeded once from the presets (Wi-Fi ← high, mobile ← medium,
//! roaming ← low, TGX-style) and every edit is pushed to TDLib via
//! `setAutoDownloadSettings` at the same time.
#![deny(clippy::indexing_slicing)]

use crate::settings::AccountPaths;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

pub const DATA_STORAGE_PREFS_VERSION: u32 = 1;

/// Slice S4: the three network types `setAutoDownloadSettings` accepts
/// (schema 1.8.67 — `networkTypeMobile` :9814,
/// `networkTypeMobileRoaming` :9817, `networkTypeWiFi` :9820).
/// `networkTypeNone` / `networkTypeOther` carry no settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetworkKind {
    Mobile,
    Roaming,
    WiFi,
}

impl NetworkKind {
    pub const ALL: [NetworkKind; 3] =
        [NetworkKind::Mobile, NetworkKind::Roaming, NetworkKind::WiFi];

    /// Verbatim `NetworkType` constructor names (schema 1.8.67).
    pub fn td_type(self) -> &'static str {
        match self {
            NetworkKind::Mobile => "networkTypeMobile",
            NetworkKind::Roaming => "networkTypeMobileRoaming",
            NetworkKind::WiFi => "networkTypeWiFi",
        }
    }

    /// Screen labels (TGX `SettingsDataStorage` connection rows).
    pub fn label(self) -> &'static str {
        match self {
            NetworkKind::Mobile => "Mobile data",
            NetworkKind::Roaming => "Roaming",
            NetworkKind::WiFi => "Wi-Fi",
        }
    }
}

/// Slice S4: one network's `autoDownloadSettings` (schema 1.8.67,
/// :9856). `video_upload_bitrate` and the `preload_*` flags are parsed
/// and passed through untouched — the screen only edits the enable
/// flag, the three size caps, and `use_less_data_for_calls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutoDownloadNetSettings {
    pub is_auto_download_enabled: bool,
    pub max_photo_file_size: i64,
    pub max_video_file_size: i64,
    pub max_other_file_size: i64,
    #[serde(default)]
    pub video_upload_bitrate: i32,
    #[serde(default)]
    pub preload_large_videos: bool,
    #[serde(default)]
    pub preload_next_audio: bool,
    #[serde(default)]
    pub preload_stories: bool,
    #[serde(default)]
    pub use_less_data_for_calls: bool,
}

impl Default for AutoDownloadNetSettings {
    /// All-off: only used before the presets seed (the UI shows the
    /// loading state until then, so these never render).
    fn default() -> Self {
        Self {
            is_auto_download_enabled: false,
            max_photo_file_size: 0,
            max_video_file_size: 0,
            max_other_file_size: 0,
            video_upload_bitrate: 0,
            preload_large_videos: false,
            preload_next_audio: false,
            preload_stories: false,
            use_less_data_for_calls: false,
        }
    }
}

impl AutoDownloadNetSettings {
    /// Slice S4: parse one `autoDownloadSettings` object (presets
    /// answers and local persistence share the shape).
    pub fn parse(value: &Value) -> Self {
        let b = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        let n = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0);
        Self {
            is_auto_download_enabled: b("is_auto_download_enabled"),
            max_photo_file_size: n("max_photo_file_size").max(0),
            max_video_file_size: n("max_video_file_size").max(0),
            max_other_file_size: n("max_other_file_size").max(0),
            video_upload_bitrate: n("video_upload_bitrate").clamp(0, i32::MAX as i64) as i32,
            preload_large_videos: b("preload_large_videos"),
            preload_next_audio: b("preload_next_audio"),
            preload_stories: b("preload_stories"),
            use_less_data_for_calls: b("use_less_data_for_calls"),
        }
    }

    /// Slice S4: serialize for `setAutoDownloadSettings`
    /// (`autoDownloadSettings`, schema 1.8.67, :9856).
    pub fn to_json(&self) -> Value {
        json!({
            "@type": "autoDownloadSettings",
            "is_auto_download_enabled": self.is_auto_download_enabled,
            "max_photo_file_size": self.max_photo_file_size,
            "max_video_file_size": self.max_video_file_size,
            "max_other_file_size": self.max_other_file_size,
            "video_upload_bitrate": self.video_upload_bitrate,
            "preload_large_videos": self.preload_large_videos,
            "preload_next_audio": self.preload_next_audio,
            "preload_stories": self.preload_stories,
            "use_less_data_for_calls": self.use_less_data_for_calls,
        })
    }

    /// Slice S4: parse an `autoDownloadSettingsPresets` answer
    /// (`autoDownloadSettingsPresets low:… medium:… high:…`, schema
    /// 1.8.67, :9862) into (low, medium, high).
    pub fn parse_presets(value: &Value) -> Option<(Self, Self, Self)> {
        Some((
            Self::parse(value.get("low")?),
            Self::parse(value.get("medium")?),
            Self::parse(value.get("high")?),
        ))
    }

    /// The cap of one media type.
    pub fn cap(&self, kind: CapKind) -> i64 {
        match kind {
            CapKind::Photo => self.max_photo_file_size,
            CapKind::Video => self.max_video_file_size,
            CapKind::Other => self.max_other_file_size,
        }
    }

    /// Set the cap of one media type, clamped to what TDLib's wire format
    /// accepts (`max_photo_file_size` is an int32).
    pub fn set_cap(&mut self, kind: CapKind, bytes: i64) {
        let bytes = bytes.clamp(0, NO_LIMIT_SIZE_CAP);
        match kind {
            CapKind::Photo => self.max_photo_file_size = bytes,
            CapKind::Video => self.max_video_file_size = bytes,
            CapKind::Other => self.max_other_file_size = bytes,
        }
    }

    /// Whether `kind` downloads automatically (tdesktop's per-type
    /// switch: a zero cap is off).
    pub fn cap_enabled(&self, kind: CapKind) -> bool {
        self.cap(kind) > 0
    }

    /// Flip tdesktop's per-type switch: turning it on starts at
    /// [`DEFAULT_ENABLED_CAP`] (`kDefaultDownloadLimit`).
    pub fn set_cap_enabled(&mut self, kind: CapKind, enabled: bool) {
        let bytes = if !enabled {
            0
        } else if self.cap(kind) > 0 {
            self.cap(kind)
        } else {
            DEFAULT_ENABLED_CAP
        };
        self.set_cap(kind, bytes);
    }
}

/// The three size caps `setAutoDownloadSettings` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapKind {
    Photo,
    Video,
    Other,
}

/// tdesktop `kDefaultDownloadLimit`: the cap a type starts at when its
/// switch is turned on while it was off.
pub const DEFAULT_ENABLED_CAP: i64 = 10 * 1024 * 1024;

/// Slice S4: one chat's row from a `getStorageStatistics` answer
/// (`storageStatisticsByChat chat_id:int53 size:int53 count:int32
/// by_file_type:vector<storageStatisticsByFileType> =
/// StorageStatisticsByChat;`, schema 1.8.67 :9787). The envelope parser
/// builds these; the UI sorts them with [`top_chats_by_size`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageChatStats {
    pub chat_id: i64,
    pub size: i64,
    pub count: i32,
}

/// Slice S4: "No limit" sentinel for the size-cap pickers. `i32::MAX`:
/// fits both schema bounds (`max_photo_file_size:int32`,
/// `max_video_file_size:int53`), so `setAutoDownloadSettings` can never
/// fail TDLib's wire round-trip check. (`i64::MAX` would deterministically
/// 400 — TDLib's `from_json(int32&)` rejects out-of-range values.)
pub const NO_LIMIT_SIZE_CAP: i64 = i32::MAX as i64;

/// Slice S4: label for one size-cap step.
pub fn size_cap_label(bytes: i64) -> String {
    if bytes <= 0 {
        "Off".to_string()
    } else if bytes == NO_LIMIT_SIZE_CAP {
        "No limit".to_string()
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} MB", bytes / (1024 * 1024))
    }
}

/// Slice S4: per-account per-network auto-download settings,
/// persisted as JSON next to the account root (`data_storage.json`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataStoragePrefs {
    pub version: u32,
    /// True once seeded from `getAutoDownloadSettingsPresets` (or
    /// loaded from disk); the screen shows "Loading…" until then.
    #[serde(default)]
    pub seeded: bool,
    /// True once the cache limits got their first value on this account
    /// (`ConnectDriver::maybe_seed_storage_limits`). Never reset, so a
    /// later "No limit" sticks.
    #[serde(default)]
    pub storage_limits_seeded: bool,
    pub mobile: AutoDownloadNetSettings,
    pub roaming: AutoDownloadNetSettings,
    pub wifi: AutoDownloadNetSettings,
}

impl Default for DataStoragePrefs {
    fn default() -> Self {
        Self {
            version: DATA_STORAGE_PREFS_VERSION,
            seeded: false,
            storage_limits_seeded: false,
            mobile: AutoDownloadNetSettings::default(),
            roaming: AutoDownloadNetSettings::default(),
            wifi: AutoDownloadNetSettings::default(),
        }
    }
}

impl DataStoragePrefs {
    pub fn for_network(&self, network: NetworkKind) -> &AutoDownloadNetSettings {
        match network {
            NetworkKind::Mobile => &self.mobile,
            NetworkKind::Roaming => &self.roaming,
            NetworkKind::WiFi => &self.wifi,
        }
    }

    pub fn for_network_mut(&mut self, network: NetworkKind) -> &mut AutoDownloadNetSettings {
        match network {
            NetworkKind::Mobile => &mut self.mobile,
            NetworkKind::Roaming => &mut self.roaming,
            NetworkKind::WiFi => &mut self.wifi,
        }
    }

    /// Slice S4: seed from presets — Wi-Fi ← high, mobile ← medium,
    /// roaming ← low (TGX-style: the unmetered network gets the
    /// generous preset).
    pub fn seed_from_presets(
        &mut self,
        low: AutoDownloadNetSettings,
        medium: AutoDownloadNetSettings,
        high: AutoDownloadNetSettings,
    ) {
        self.roaming = low;
        self.mobile = medium;
        self.wifi = high;
        self.seeded = true;
    }

    /// Slice S4: one-line summary for a network row, e.g.
    /// "On · photos 15 MB · videos 50 MB · files Off".
    pub fn network_summary(&self, network: NetworkKind) -> String {
        let s = self.for_network(network);
        if !self.seeded {
            return "Loading…".to_string();
        }
        if !s.is_auto_download_enabled {
            return "Off".to_string();
        }
        format!(
            "On · photos {} · videos {} · files {}",
            size_cap_label(s.max_photo_file_size),
            size_cap_label(s.max_video_file_size),
            size_cap_label(s.max_other_file_size),
        )
    }
}

fn data_storage_prefs_path(paths: &AccountPaths) -> PathBuf {
    paths.root.join("data_storage.json")
}

/// Load data-storage prefs; missing or corrupt files fall back to
/// unseeded defaults (never a hard error — prefs must not block
/// startup). Same pattern as `load_media_prefs`.
pub fn load_data_storage_prefs(paths: &AccountPaths) -> DataStoragePrefs {
    std::fs::read(data_storage_prefs_path(paths))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Persist data-storage prefs; failures are returned to the caller to
/// surface in the status note.
pub fn save_data_storage_prefs(
    paths: &AccountPaths,
    prefs: &DataStoragePrefs,
) -> std::io::Result<()> {
    crate::settings::write_json_atomic(&data_storage_prefs_path(paths), prefs)
}

/// Slice S4: top-N chats by storage size, descending (the usage
/// section's per-chat breakdown). `rows` are
/// `(chat_id, size, count)`; titles resolve via the session's chat map.
pub fn top_chats_by_size(rows: &[(i64, i64, i32)], n: usize) -> Vec<(i64, i64, i32)> {
    let mut rows = rows.to_vec();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    rows.truncate(n);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_settings() -> Value {
        json!({
            "@type": "autoDownloadSettings",
            "is_auto_download_enabled": true,
            "max_photo_file_size": 15728640,
            "max_video_file_size": 52428800,
            "max_other_file_size": 0,
            "video_upload_bitrate": 0,
            "preload_large_videos": false,
            "preload_next_audio": true,
            "preload_stories": false,
            "use_less_data_for_calls": true,
        })
    }

    #[test]
    fn parse_round_trips_through_to_json() {
        let parsed = AutoDownloadNetSettings::parse(&sample_settings());
        assert!(parsed.is_auto_download_enabled);
        assert_eq!(parsed.max_photo_file_size, 15728640);
        assert_eq!(parsed.max_other_file_size, 0);
        assert!(parsed.preload_next_audio);
        assert!(parsed.use_less_data_for_calls);
        let back = AutoDownloadNetSettings::parse(&parsed.to_json());
        assert_eq!(parsed, back);
    }

    #[test]
    fn parse_presets_maps_low_medium_high() {
        let presets = json!({
            "@type": "autoDownloadSettingsPresets",
            "low": sample_settings(),
            "medium": sample_settings(),
            "high": sample_settings(),
        });
        let (low, medium, high) = AutoDownloadNetSettings::parse_presets(&presets).unwrap();
        assert!(
            low.is_auto_download_enabled
                && medium.is_auto_download_enabled
                && high.is_auto_download_enabled
        );
        assert!(AutoDownloadNetSettings::parse_presets(&json!({})).is_none());
    }

    #[test]
    fn seed_assigns_wifi_high_mobile_medium_roaming_low() {
        let mut prefs = DataStoragePrefs::default();
        assert!(!prefs.seeded);
        let (low, medium, high) = (
            AutoDownloadNetSettings {
                max_photo_file_size: 1,
                ..Default::default()
            },
            AutoDownloadNetSettings {
                max_photo_file_size: 2,
                ..Default::default()
            },
            AutoDownloadNetSettings {
                max_photo_file_size: 3,
                ..Default::default()
            },
        );
        prefs.seed_from_presets(low, medium, high);
        assert!(prefs.seeded);
        assert_eq!(prefs.wifi.max_photo_file_size, 3);
        assert_eq!(prefs.mobile.max_photo_file_size, 2);
        assert_eq!(prefs.roaming.max_photo_file_size, 1);
    }

    #[test]
    fn size_caps_label_and_switch() {
        let mut net = AutoDownloadNetSettings::default();
        net.set_cap(CapKind::Photo, i64::MAX);
        assert_eq!(net.cap(CapKind::Photo), NO_LIMIT_SIZE_CAP);
        net.set_cap_enabled(CapKind::Photo, false);
        assert!(!net.cap_enabled(CapKind::Photo));
        net.set_cap_enabled(CapKind::Photo, true);
        assert_eq!(net.cap(CapKind::Photo), DEFAULT_ENABLED_CAP);
        net.set_cap(CapKind::Video, 50 * 1024 * 1024);
        net.set_cap_enabled(CapKind::Video, true);
        assert_eq!(net.cap(CapKind::Video), 50 * 1024 * 1024);
        assert_eq!(size_cap_label(0), "Off");
        assert_eq!(size_cap_label(NO_LIMIT_SIZE_CAP), "No limit");
        assert_eq!(size_cap_label(15 * 1024 * 1024), "15 MB");
    }

    #[test]
    fn network_summary_states() {
        let mut prefs = DataStoragePrefs::default();
        assert_eq!(prefs.network_summary(NetworkKind::WiFi), "Loading…");
        prefs.seeded = true;
        assert_eq!(prefs.network_summary(NetworkKind::WiFi), "Off");
        prefs.wifi.is_auto_download_enabled = true;
        prefs.wifi.max_photo_file_size = 15 * 1024 * 1024;
        prefs.wifi.max_video_file_size = 50 * 1024 * 1024;
        prefs.wifi.max_other_file_size = 0;
        assert_eq!(
            prefs.network_summary(NetworkKind::WiFi),
            "On · photos 15 MB · videos 50 MB · files Off"
        );
    }

    #[test]
    fn top_chats_sorts_descending_and_truncates() {
        let rows = vec![(1, 100, 2), (2, 900, 5), (3, 500, 3)];
        assert_eq!(top_chats_by_size(&rows, 2), vec![(2, 900, 5), (3, 500, 3)]);
    }

    #[test]
    fn prefs_persist_round_trip() {
        let dir = std::env::temp_dir().join(format!("quill-ds-test-{}", std::process::id()));
        let paths = AccountPaths {
            root: dir.clone(),
            tdlib_database: dir.join("tdlib"),
            tdlib_files: dir.join("files"),
            app_thumbnails: dir.join("thumbnails"),
            exports: dir.join("exports"),
        };
        let mut prefs = DataStoragePrefs {
            seeded: true,
            ..Default::default()
        };
        prefs.wifi.is_auto_download_enabled = true;
        save_data_storage_prefs(&paths, &prefs).unwrap();
        assert_eq!(load_data_storage_prefs(&paths), prefs);
        std::fs::remove_dir_all(&dir).ok();
    }
}
