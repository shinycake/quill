//! Media preferences: auto-download settings, grouping, instant view.
use super::*;

/// MED1: local-only media preferences, persisted as JSON next to the
/// account root (`media_prefs.json`). Client-side only (no TDLib setting):
/// - `media_grouping`: whether the composer's "group media" choice is
///   remembered between sends (TGX `RememberAlbumSetting`), and the last
///   choice then. Saved as `remember_media_grouping` and `group_media`.
/// - `hq_round_videos`: MED2 — "Record HQ Round Videos" (TGX
///   `UseHqRoundVideos`): capture round video notes at 480px instead of 280px;
/// - `prefer_video_mode`: MED2 — the record button's mode (TGX
///   `preferVideoMode`); right-click on the record button flips it.
/// - `data_saver`: MED3 — pause-all auto-downloads (TGX `settings_datasaver`
///   master bit). User-initiated downloads are unaffected.
/// - `auto_download`: Telegram Desktop's automatic media download, the
///   byte limits per chat source and the chat exceptions
///   ([`AutoDownloadSettings`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaPrefs {
    #[serde(default = "default_true")]
    pub autoplay_gifs: bool,
    /// Telegram Desktop's "Autoplay videos": downloaded videos play muted
    /// and looped in the chat.
    #[serde(default = "default_true")]
    pub autoplay_videos: bool,
    #[serde(default = "default_true")]
    pub dynamic_emoji_pack_order: bool,
    #[serde(default)]
    pub recent_emoji_packs: Vec<i64>,
    #[serde(default)]
    pub recent_custom_emoji_ids: Vec<i64>,
    #[serde(default = "default_true")]
    pub big_emoji: bool,
    #[serde(default = "default_true")]
    pub loop_animated_stickers: bool,
    /// Telegram Desktop's "Reply button on messages" (`cornerReply`,
    /// default on): a reply button beside the hovered bubble.
    #[serde(default = "default_true")]
    pub corner_reply: bool,
    /// Telegram Desktop's "Reaction button on messages" (`cornerReaction`,
    /// default on): a reaction button beside the hovered bubble.
    #[serde(default = "default_true")]
    pub corner_reaction: bool,
    /// Telegram Desktop's "Swipe to the next unread channel"
    /// (`pullToNextChannel`, default on): pulling past the bottom of a
    /// channel opens the next unread one (`pull_next`).
    #[serde(default = "default_true")]
    pub pull_to_next_channel: bool,
    /// Telegram Desktop's `chatQuickAction`: what double-clicking a message
    /// does (reply by default).
    #[serde(default)]
    pub quick_action: crate::quick_action::QuickAction,
    #[serde(default)]
    pub recent_emoji: Vec<String>,
    #[serde(flatten, default)]
    pub media_grouping: MediaGrouping,
    pub hq_round_videos: bool,
    pub prefer_video_mode: bool,
    #[serde(default)]
    pub data_saver: bool,
    #[serde(flatten)]
    pub auto_download: AutoDownloadPrefs,
    /// MED4: Instant View mode (TGX `Settings.INSTANT_VIEW_MODE`:
    /// None / Telegram-internal / All, `Settings.java:796-798`).
    /// Default `Telegram` — the reader opens for links TDLib flagged
    /// with `instant_view_version > 0`.
    #[serde(default)]
    pub instant_view_mode: InstantViewMode,
    /// Slice S12: sticker-suggestion mode for the composer
    /// (`sticker_suggest::StickerSuggestMode`; client-side, no TDLib
    /// setting exists). Default: installed + recommended.
    #[serde(default)]
    pub sticker_suggest_mode: StickerSuggestMode,
    /// Subsection tabs: per-chat tab layout (Telegram Desktop's
    /// `subsectionTabsMode(peerId)` session setting), keyed by chat id.
    /// Absent = Top, the Telegram Desktop default.
    #[serde(default)]
    pub subsection_tabs_modes:
        std::collections::BTreeMap<i64, crate::subsection_tabs::SubsectionTabsMode>,
    /// The viewer's video quality (Telegram Desktop's `videoQuality`):
    /// 0 is Auto (the original), otherwise the chosen short side in pixels.
    #[serde(default)]
    pub video_quality: u32,
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

impl Default for MediaPrefs {
    fn default() -> Self {
        Self {
            autoplay_gifs: true,
            autoplay_videos: true,
            dynamic_emoji_pack_order: true,
            recent_emoji_packs: Vec::new(),
            recent_custom_emoji_ids: Vec::new(),
            big_emoji: true,
            loop_animated_stickers: true,
            corner_reply: true,
            corner_reaction: true,
            pull_to_next_channel: true,
            quick_action: crate::quick_action::QuickAction::default(),
            recent_emoji: Vec::new(),
            media_grouping: MediaGrouping::AlwaysGrouped,
            hq_round_videos: false,
            prefer_video_mode: false,
            data_saver: false,
            auto_download: AutoDownloadPrefs::default(),
            instant_view_mode: InstantViewMode::default(),
            sticker_suggest_mode: StickerSuggestMode::default(),
            subsection_tabs_modes: std::collections::BTreeMap::new(),
            video_quality: 0,
        }
    }
}

/// The composer's "group media" default: always grouped, or the last
/// choice. There is no remembered choice while remembering is off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "MediaGroupingFile", into = "MediaGroupingFile")]
pub enum MediaGrouping {
    /// Remembering is off: every fresh composer groups (the historical
    /// behavior).
    #[default]
    AlwaysGrouped,
    /// Remembering is on: the last choice is the default.
    Remembered { grouped: bool },
}

/// The two keys of `media_prefs.json` a [`MediaGrouping`] is saved as.
#[derive(Serialize, Deserialize)]
struct MediaGroupingFile {
    #[serde(default)]
    remember_media_grouping: bool,
    #[serde(default)]
    group_media: bool,
}

impl From<MediaGroupingFile> for MediaGrouping {
    fn from(file: MediaGroupingFile) -> Self {
        if file.remember_media_grouping {
            Self::Remembered {
                grouped: file.group_media,
            }
        } else {
            Self::AlwaysGrouped
        }
    }
}

impl From<MediaGrouping> for MediaGroupingFile {
    fn from(grouping: MediaGrouping) -> Self {
        Self {
            remember_media_grouping: grouping.is_remembered(),
            group_media: grouping.grouped(),
        }
    }
}

impl MediaGrouping {
    pub fn is_remembered(self) -> bool {
        matches!(self, Self::Remembered { .. })
    }

    /// Effective grouping for a fresh composer: the remembered choice when
    /// remembering is on, grouped otherwise.
    pub fn grouped(self) -> bool {
        match self {
            Self::AlwaysGrouped => true,
            Self::Remembered { grouped } => grouped,
        }
    }

    /// The composer's choice was flipped: kept only while remembering.
    pub fn choose(&mut self, grouped: bool) {
        if let Self::Remembered { grouped: last } = self {
            *last = grouped;
        }
    }

    /// "Remember media grouping" was switched; turning it on keeps the
    /// current choice.
    pub fn remember(&mut self, on: bool, current: bool) {
        *self = if on {
            Self::Remembered { grouped: current }
        } else {
            Self::AlwaysGrouped
        };
    }
}

impl MediaPrefs {
    /// Effective grouping for a fresh composer.
    pub fn default_grouping(&self) -> bool {
        self.media_grouping.grouped()
    }
}

/// Load media prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_media_prefs(paths: &AccountPaths) -> MediaPrefs {
    load_json_prefs(paths, "media_prefs.json")
}

/// Persist media prefs; a failure reaches the status note through
/// [`take_deferred_write_failure`].
pub fn save_media_prefs(paths: &AccountPaths, prefs: &MediaPrefs) {
    save_json_prefs(paths, "media_prefs.json", prefs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_prefs_roundtrip_and_default_grouping() {
        // MED1: the stored grouping choice survives a load, and the
        // effective default follows the remember flag.
        let dir =
            std::env::temp_dir().join(format!("quill-media-prefs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        assert!(MediaPrefs::default().default_grouping());
        let prefs = MediaPrefs {
            autoplay_gifs: false,
            autoplay_videos: false,
            dynamic_emoji_pack_order: false,
            recent_emoji_packs: vec![2, 1],
            media_grouping: MediaGrouping::Remembered { grouped: false },
            hq_round_videos: true,
            prefer_video_mode: true,
            quick_action: crate::quick_action::QuickAction::React,
            ..Default::default()
        };
        save_media_prefs(&paths, &prefs);
        assert_eq!(load_media_prefs(&paths), prefs);
        assert!(!prefs.default_grouping());
        // Remember off → grouped regardless of the stored choice.
        let prefs = MediaPrefs {
            media_grouping: MediaGrouping::AlwaysGrouped,
            ..Default::default()
        };
        assert!(prefs.default_grouping());
        let _ = std::fs::remove_dir_all(&dir);
        let corrupt =
            std::env::temp_dir().join(format!("quill-media-prefs-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&corrupt);
        let paths = AccountPaths::for_root(&corrupt, &AccountKey::primary());
        std::fs::create_dir_all(corrupt.join("accounts/primary")).unwrap();
        std::fs::write(
            corrupt.join("accounts/primary/media_prefs.json"),
            b"not json",
        )
        .unwrap();
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        let _ = std::fs::remove_dir_all(&corrupt);
    }

    #[test]
    fn saving_does_not_wait_for_the_disk() {
        let dir =
            std::env::temp_dir().join(format!("quill-media-prefs-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        let file = dir.join("accounts/primary/media_prefs.json");
        let open = crate::settings::deferred::test_gate::hold(&file);
        let prefs = MediaPrefs {
            autoplay_gifs: false,
            ..Default::default()
        };
        save_media_prefs(&paths, &prefs);
        // The writer is held before this file: the save already returned.
        assert!(!file.exists());
        assert_eq!(load_media_prefs(&paths), prefs);
        drop(open);
        crate::settings::flush_deferred_writes();
        let on_disk: MediaPrefs = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(on_disk, prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn auto_download_defaults_match_tdesktop_and_old_files_still_load() {
        // Telegram Desktop's defaults; data saver off.
        let defaults = MediaPrefs::default();
        assert!(!defaults.data_saver);
        assert_eq!(*defaults.auto_download, AutoDownloadSettings::default());
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
        assert!(loaded.autoplay_gifs);
        assert!(loaded.dynamic_emoji_pack_order);
        assert!(loaded.recent_emoji_packs.is_empty());
        assert_eq!(
            loaded.media_grouping,
            MediaGrouping::Remembered { grouped: true }
        );
        let json = serde_json::to_value(&loaded).unwrap();
        assert_eq!(json["remember_media_grouping"], true);
        assert_eq!(json["group_media"], true);
        assert!(loaded.hq_round_videos);
        assert!(!loaded.data_saver);
        assert_eq!(*loaded.auto_download, AutoDownloadSettings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
