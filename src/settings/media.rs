//! Media preferences: auto-download flags and limits, grouping, instant view.
use super::*;

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
    #[serde(default)]
    pub recent_emoji: Vec<String>,
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
            autoplay_gifs: true,
            autoplay_videos: true,
            dynamic_emoji_pack_order: true,
            recent_emoji_packs: Vec::new(),
            recent_custom_emoji_ids: Vec::new(),
            big_emoji: true,
            loop_animated_stickers: true,
            corner_reply: true,
            corner_reaction: true,
            recent_emoji: Vec::new(),
            remember_media_grouping: false,
            group_media: false,
            hq_round_videos: false,
            prefer_video_mode: false,
            data_saver: false,
            auto_download_private: AUTO_DOWNLOAD_DEFAULT,
            auto_download_groups: AUTO_DOWNLOAD_DEFAULT,
            auto_download_channels: AUTO_DOWNLOAD_DEFAULT,
            instant_view_mode: InstantViewMode::default(),
            sticker_suggest_mode: StickerSuggestMode::default(),
            subsection_tabs_modes: std::collections::BTreeMap::new(),
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

/// Load media prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_media_prefs(paths: &AccountPaths) -> MediaPrefs {
    load_json_prefs(paths, "media_prefs.json")
}

/// Persist media prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_media_prefs(paths: &AccountPaths, prefs: &MediaPrefs) -> std::io::Result<()> {
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
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        assert_eq!(load_media_prefs(&paths), MediaPrefs::default());
        assert!(MediaPrefs::default().default_grouping());
        let prefs = MediaPrefs {
            autoplay_gifs: false,
            autoplay_videos: false,
            dynamic_emoji_pack_order: false,
            recent_emoji_packs: vec![2, 1],
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
        assert!(loaded.autoplay_gifs);
        assert!(loaded.dynamic_emoji_pack_order);
        assert!(loaded.recent_emoji_packs.is_empty());
        assert!(loaded.remember_media_grouping);
        assert!(loaded.hq_round_videos);
        assert!(!loaded.data_saver);
        assert_eq!(loaded.auto_download_private, AUTO_DOWNLOAD_DEFAULT);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
