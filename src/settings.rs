//! Small non-message preferences. Message history lives in TDLib.

use crate::ids::AccountKey;
use crate::sticker_suggest::StickerSuggestMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const APP_DIR_NAME: &str = "Quill";
pub const PREFS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preferences {
    pub version: u32,
    pub account: AccountKey,
    /// Application API credentials are never stored here. See env / local untracked file.
    pub hide_notification_previews: bool,
    /// Parity slice: tdesktop "Play sounds" — in-app notification sounds.
    /// Client-side (no TDLib setting exists; `in-app-sounds` is only a
    /// `SettingsSection` deep-link name, schema line 9322).
    pub inapp_sounds_enabled: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: PREFS_VERSION,
            account: AccountKey::primary(),
            hide_notification_previews: true,
            inapp_sounds_enabled: true,
        }
    }
}

/// Load general prefs (`prefs.json`); missing or corrupt files fall back
/// to defaults (never a hard error — prefs must not block startup).
pub fn load_preferences(paths: &AccountPaths) -> Preferences {
    load_json_prefs(paths, "prefs.json")
}

/// Persist general prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_preferences(paths: &AccountPaths, prefs: &Preferences) -> std::io::Result<()> {
    save_json_prefs(paths, "prefs.json", prefs)
}

/// Phase C2i: local-only call preferences, persisted as JSON next to
/// the account root (`call_prefs.json`). These are client-side (no
/// TDLib setting exists for them):
/// - `confirm_before_calling`: ask before placing an outgoing call.
/// - `use_proxy_for_calls`: route call media through the enabled proxy.
///   The TDLib schema (1.8.67) has no such option — the only
///   `use-for-calls` mention is the `proxy/use-for-calls` settings
///   deep-link subsection (`schema/td_api.tl:9276`) — so, like the
///   official clients, this is a client-side toggle: when on, the client
///   hands the enabled proxy to its VoIP engine (SOCKS5 only; MTProto
///   and HTTP proxies cannot carry call media). Default off, matching
///   the official clients (opt-in).
///
/// Slice S4: the old `less_data_for_calls` flag was deleted — "Use less
/// data for calls" is a real TDLib setting now
/// (`autoDownloadSettings.use_less_data_for_calls`, schema 1.8.67
/// :9856), surfaced in Data & Storage and the call settings; the stale
/// local-only flag would have been a second, divergent toggle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallPrefs {
    pub confirm_before_calling: bool,
    pub use_proxy_for_calls: bool,
}

/// Shared load: a missing or corrupt prefs file falls back to defaults —
/// prefs must never block startup or panic.
fn load_json_prefs<T>(paths: &AccountPaths, file_name: &str) -> T
where
    T: Default + for<'de> Deserialize<'de>,
{
    std::fs::read(paths.root.join(file_name))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Shared save: creates the parent dir; failures are returned to the
/// caller to surface in the status note.
fn save_json_prefs<T>(paths: &AccountPaths, file_name: &str, prefs: &T) -> std::io::Result<()>
where
    T: Serialize,
{
    let path = paths.root.join(file_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

/// Load call prefs; missing or corrupt files fall back to defaults
/// (never a hard error — prefs must not block startup).
pub fn load_call_prefs(paths: &AccountPaths) -> CallPrefs {
    load_json_prefs(paths, "call_prefs.json")
}

/// Persist call prefs; failures are returned to the caller to surface
/// in the status note.
pub fn save_call_prefs(paths: &AccountPaths, prefs: &CallPrefs) -> std::io::Result<()> {
    save_json_prefs(paths, "call_prefs.json", prefs)
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
    /// Slice S12: sticker-suggestion mode for the composer
    /// (`sticker_suggest::StickerSuggestMode`; client-side, no TDLib
    /// setting exists). Default: installed + recommended.
    #[serde(default)]
    pub sticker_suggest_mode: StickerSuggestMode,
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
            sticker_suggest_mode: StickerSuggestMode::default(),
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

/// Settings → Appearance slice: client-side look-and-feel, persisted as
/// JSON next to the account root (`appearance_prefs.json`). Entirely
/// client-side — no TDLib setting exists for the app theme, accent,
/// font size, or chat-list style (TDLib's `accentColor` is per-peer
/// name/profile tinting, not app chrome; chat backgrounds exist
/// server-side as `setChatBackground`, schema 1.8.67 :13473, but this
/// slice paints local solid colors only — see DECISIONS.md).
///
/// TGX reference (`Settings.java`, `SettingsThemeController.java`):
/// night modes None/Auto(lux)/Scheduled/System, per-theme accent color
/// ids, solid wallpapers via the backgrounds API, a text-size slider,
/// and bubble chat style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    Light,
    Dark,
}

/// Auto-night mode (TGX `NIGHT_MODE_*`, `Settings.java:460-464`). `Auto`
/// (ambient-light sensor) has no desktop equivalent, so Quill offers
/// Off / System (OS appearance) / Scheduled (local-time window).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoNight {
    #[default]
    Off,
    System,
    Scheduled,
}

/// Message text size bounds, in px. The default 14px matches the
/// previous hardcoded `text_sm()` at the 16px rem base.
pub const FONT_SIZE_MIN: u8 = 12;
pub const FONT_SIZE_MAX: u8 = 20;
pub const FONT_SIZE_DEFAULT: u8 = 14;

/// Clamp a font size into the supported range (the stepper can't
/// produce out-of-range values, but prefs files are user-editable).
pub fn clamp_font_size(px: u8) -> u8 {
    px.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePrefs {
    /// Manual theme choice; auto-night overrides it while active.
    #[serde(default)]
    pub theme: ThemeChoice,
    #[serde(default)]
    pub auto_night: AutoNight,
    /// Scheduled auto-night window, minutes since local midnight
    /// (defaults 22:00 → 07:00, TGX's usual night window).
    #[serde(default = "default_night_start")]
    pub night_start_minutes: u16,
    #[serde(default = "default_night_end")]
    pub night_end_minutes: u16,
    /// Accent color as 0xRRGGBB; 0 = the theme's default accent.
    #[serde(default)]
    pub accent_rgb: u32,
    /// Chat wallpaper as 0xRRGGBB; None = the theme background.
    #[serde(default)]
    pub wallpaper_rgb: Option<u32>,
    /// Message text size in px.
    #[serde(default = "default_font_size")]
    pub font_size_px: u8,
    /// Bubble style (true) vs plain full-width rows (false).
    #[serde(default = "default_true")]
    pub bubbles: bool,
    /// Slice chatlist-list-style: chat-list preview lines — 2 (title +
    /// preview) or 3 (title + sender line + preview line).
    #[serde(default = "default_preview_lines")]
    pub preview_lines: u8,
    /// Slice chatlist-list-style: show a media-type icon before the
    /// chat-list preview text.
    #[serde(default)]
    pub chat_list_media_icons: bool,
    /// Slice chatlist-list-style: render formatted text (bold/italic/…)
    /// in the chat-list preview instead of plain text.
    #[serde(default)]
    pub chat_list_rich_preview: bool,
}

fn default_night_start() -> u16 {
    22 * 60
}

fn default_night_end() -> u16 {
    7 * 60
}

fn default_font_size() -> u8 {
    FONT_SIZE_DEFAULT
}

fn default_preview_lines() -> u8 {
    crate::chatlist_style::PREVIEW_LINES_DEFAULT
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::Light,
            auto_night: AutoNight::Off,
            night_start_minutes: default_night_start(),
            night_end_minutes: default_night_end(),
            accent_rgb: 0,
            wallpaper_rgb: None,
            font_size_px: FONT_SIZE_DEFAULT,
            bubbles: true,
            preview_lines: crate::chatlist_style::PREVIEW_LINES_DEFAULT,
            chat_list_media_icons: false,
            chat_list_rich_preview: false,
        }
    }
}

/// Load appearance prefs; missing or corrupt files fall back to
/// defaults (never a hard error — prefs must not block startup).
/// Out-of-range values from hand-edited JSON are sanitized here, at
/// the single load path: the font size clamps to 12–20 px and night
/// times wrap into 0..1440.
pub fn load_appearance_prefs(paths: &AccountPaths) -> AppearancePrefs {
    let mut prefs: AppearancePrefs = load_json_prefs(paths, "appearance_prefs.json");
    prefs.font_size_px = clamp_font_size(prefs.font_size_px);
    prefs.preview_lines = crate::chatlist_style::clamp_preview_lines(prefs.preview_lines);
    prefs.night_start_minutes %= 24 * 60;
    prefs.night_end_minutes %= 24 * 60;
    prefs
}

/// Persist appearance prefs; failures are returned to the caller to
/// surface in the status note.
pub fn save_appearance_prefs(paths: &AccountPaths, prefs: &AppearancePrefs) -> std::io::Result<()> {
    save_json_prefs(paths, "appearance_prefs.json", prefs)
}

/// Pure predicate behind scheduled auto-night: is `now` (minutes since
/// local midnight) inside the [start, end) night window? Windows that
/// wrap past midnight (22:00 → 07:00) are the normal case; start == end
/// is the empty window (never night).
pub fn night_active(start_minutes: u16, end_minutes: u16, now_minutes: u16) -> bool {
    if start_minutes <= end_minutes {
        now_minutes >= start_minutes && now_minutes < end_minutes
    } else {
        now_minutes >= start_minutes || now_minutes < end_minutes
    }
}

/// Minutes since local midnight, for scheduled auto-night. A thin
/// `libc::localtime_r` wrapper — no chrono/time dependency for one
/// call; the testable predicate is `night_active`.
pub fn local_minutes_since_midnight() -> u16 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as libc::time_t)
        .unwrap_or(0);
    let mut broken: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut broken) }.is_null() {
        return 0;
    }
    (broken.tm_hour.max(0) as u16) * 60 + (broken.tm_min.max(0) as u16)
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

/// Chat-composer behavior prefs, persisted as JSON next to the account
/// root (`chat_prefs.json`). Client-side only (no TDLib setting):
/// - `send_key_mode`: which keystroke sends a message
///   (`composer::SendKeyMode`; parity:settings-enter-send,
///   parity:settings-ctrlenter-send).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatPrefs {
    #[serde(default)]
    pub send_key_mode: crate::composer::SendKeyMode,
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

/// Multi-account registry (`accounts.json` at the app root, next to
/// `accounts/`). Missing or corrupt files fall back to `[primary]` — prefs
/// must not block startup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountRecord {
    pub key: AccountKey,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccountRegistry {
    accounts: Vec<AccountRecord>,
    #[serde(default)]
    active: Option<AccountKey>,
}

impl Default for AccountRegistry {
    fn default() -> Self {
        Self {
            accounts: vec![AccountRecord {
                key: AccountKey::primary(),
                display_name: "Primary".to_string(),
            }],
            active: Some(AccountKey::primary()),
        }
    }
}

fn registry_path(app_root: &Path) -> PathBuf {
    app_root.join("accounts.json")
}

/// Load the registry; missing or corrupt ⇒ `[primary]` (never a hard error).
fn load_registry(app_root: &Path) -> AccountRegistry {
    let raw = std::fs::read(registry_path(app_root)).ok();
    let mut registry: AccountRegistry = raw
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    // Drop hand-edited garbage and duplicate keys.
    registry
        .accounts
        .retain(|r| AccountKey::new(&r.key.0).is_some());
    let mut seen = std::collections::HashSet::new();
    registry.accounts.retain(|r| seen.insert(r.key.clone()));
    if registry.accounts.is_empty() {
        return AccountRegistry::default();
    }
    if registry
        .active
        .as_ref()
        .is_none_or(|k| !registry.accounts.iter().any(|r| &r.key == k))
    {
        registry.active = Some(registry.accounts[0].key.clone());
    }
    registry
}

fn save_registry(app_root: &Path, registry: &AccountRegistry) -> std::io::Result<()> {
    std::fs::create_dir_all(app_root)?;
    std::fs::write(
        registry_path(app_root),
        serde_json::to_vec_pretty(registry)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    )
}

/// All known accounts; missing or corrupt registry ⇒ `[primary]`.
pub fn list_accounts(app_root: &Path) -> Vec<AccountRecord> {
    load_registry(app_root).accounts
}

/// The account to connect at startup; persisted in the registry,
/// default `primary`.
pub fn active_account(app_root: &Path) -> AccountKey {
    load_registry(app_root)
        .active
        .unwrap_or_else(AccountKey::primary)
}

/// Persist the startup account. Errors when `key` is not in the registry.
pub fn set_active_account(app_root: &Path, key: &AccountKey) -> std::io::Result<()> {
    let mut registry = load_registry(app_root);
    if !registry.accounts.iter().any(|r| &r.key == key) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("no such account: {key}"),
        ));
    }
    registry.active = Some(key.clone());
    save_registry(app_root, &registry)
}

/// Add an account with a collision-free key (`account-<n>`); returns the key.
pub fn add_account(app_root: &Path, display_name: &str) -> std::io::Result<AccountKey> {
    let mut registry = load_registry(app_root);
    let mut n = 1u32;
    loop {
        let candidate = AccountKey::new(&format!("account-{n}")).expect("generated id is valid");
        if !registry.accounts.iter().any(|r| r.key == candidate) {
            registry.accounts.push(AccountRecord {
                key: candidate.clone(),
                display_name: display_name.to_string(),
            });
            save_registry(app_root, &registry)?;
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Refusing to remove the last remaining account, or deleting an account's
/// directory tree, must not panic — typed error instead.
#[derive(Debug)]
pub enum RemoveAccountError {
    /// The registry must always keep at least one account.
    LastAccount,
    Io(std::io::Error),
}

impl std::fmt::Display for RemoveAccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LastAccount => write!(f, "cannot remove the last remaining account"),
            Self::Io(e) => write!(f, "failed to remove account: {e}"),
        }
    }
}

impl std::error::Error for RemoveAccountError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for RemoveAccountError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Remove an account and delete its directory tree (`accounts/<key>`).
/// Refuses to remove the last remaining account.
pub fn remove_account(app_root: &Path, key: &AccountKey) -> Result<(), RemoveAccountError> {
    let mut registry = load_registry(app_root);
    if registry.accounts.len() <= 1 {
        return Err(RemoveAccountError::LastAccount);
    }
    registry.accounts.retain(|r| &r.key != key);
    if registry.active.as_ref() == Some(key) {
        registry.active = registry.accounts.first().map(|r| r.key.clone());
    }
    // The DB tree is gone once the record is; a missing dir is not an error.
    let dir = app_root.join("accounts").join(&key.0);
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return Err(RemoveAccountError::Io(e));
    }
    save_registry(app_root, &registry)?;
    Ok(())
}

/// Platform data directory, or `None` when the OS provides none. There is
/// deliberately no `./quill-data` fallback: live startup must refuse rather
/// than scatter account databases and encryption keys under whatever
/// directory the process was launched from (a broad `git add` there would
/// commit them).
pub fn safe_app_root() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "shinycake", APP_DIR_NAME)
        .map(|dirs| dirs.data_dir().to_path_buf())
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
            use_proxy_for_calls: true,
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

    #[test]
    fn gitignore_backstops_account_data() {
        // The .gitignore entries are the last line of defense against a
        // broad `git add` committing live account state. If someone edits
        // them away, this fails loudly.
        let ignore =
            fs::read(format!("{}/.gitignore", env!("CARGO_MANIFEST_DIR"))).expect(".gitignore");
        let ignore = String::from_utf8(ignore).expect("utf8");
        for entry in [
            "quill-data/",
            "db-encryption.key",
            "td.binlog",
            "db.sqlite",
            "db.sqlite-wal",
            "db.sqlite-shm",
        ] {
            assert!(
                ignore.lines().any(|line| line.trim() == entry),
                ".gitignore must contain exact entry: {entry}"
            );
        }
    }

    /// Settings → Appearance: prefs survive a save/load roundtrip and a
    /// missing file falls back to defaults (never an error).
    #[test]
    fn appearance_prefs_roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("quill-appearance-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());

        // Missing file → defaults (light theme, auto-night off, 14px, bubbles).
        let defaults = load_appearance_prefs(&paths);
        assert_eq!(defaults, AppearancePrefs::default());
        assert_eq!(defaults.theme, ThemeChoice::Light);
        assert_eq!(defaults.auto_night, AutoNight::Off);
        assert_eq!(defaults.font_size_px, FONT_SIZE_DEFAULT);
        assert!(defaults.bubbles);

        let prefs = AppearancePrefs {
            theme: ThemeChoice::Dark,
            auto_night: AutoNight::Scheduled,
            night_start_minutes: 23 * 60,
            night_end_minutes: 6 * 60,
            accent_rgb: 0x2f81f7,
            wallpaper_rgb: Some(0x0e1621),
            font_size_px: 17,
            bubbles: false,
            preview_lines: 3,
            chat_list_media_icons: true,
            chat_list_rich_preview: true,
        };
        save_appearance_prefs(&paths, &prefs).unwrap();
        assert_eq!(load_appearance_prefs(&paths), prefs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Settings → Appearance: a corrupt prefs file falls back to
    /// defaults instead of blocking startup.
    #[test]
    fn appearance_prefs_corrupt_file_falls_back() {
        let dir =
            std::env::temp_dir().join(format!("quill-appearance-corrupt-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        fs::create_dir_all(&paths.root).unwrap();
        fs::write(
            paths.root.join("appearance_prefs.json"),
            b"{ this is not json",
        )
        .unwrap();
        assert_eq!(load_appearance_prefs(&paths), AppearancePrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Settings → Appearance: scheduled auto-night boundaries, including
    /// the wrap-past-midnight case and the empty window.
    #[test]
    fn night_active_cases() {
        // Wrap-past-midnight window 22:00 → 07:00.
        assert!(night_active(22 * 60, 7 * 60, 23 * 60));
        assert!(night_active(22 * 60, 7 * 60, 3 * 60));
        assert!(!night_active(22 * 60, 7 * 60, 12 * 60));
        // Boundaries: inclusive start, exclusive end.
        assert!(night_active(22 * 60, 7 * 60, 22 * 60));
        assert!(!night_active(22 * 60, 7 * 60, 7 * 60));
        // Same-day window 09:00 → 17:00.
        assert!(night_active(9 * 60, 17 * 60, 12 * 60));
        assert!(!night_active(9 * 60, 17 * 60, 8 * 60));
        assert!(!night_active(9 * 60, 17 * 60, 17 * 60));
        // Degenerate window (start == end) is never night.
        assert!(!night_active(7 * 60, 7 * 60, 7 * 60));
        assert!(!night_active(7 * 60, 7 * 60, 12 * 60));
    }

    /// Settings → Appearance: out-of-range font sizes clamp to 12–20px.
    #[test]
    fn font_size_clamps_to_range() {
        assert_eq!(clamp_font_size(0), FONT_SIZE_MIN);
        assert_eq!(clamp_font_size(11), FONT_SIZE_MIN);
        assert_eq!(clamp_font_size(14), 14);
        assert_eq!(clamp_font_size(20), 20);
        assert_eq!(clamp_font_size(255), FONT_SIZE_MAX);
    }

    /// Settings → Appearance: the load path sanitizes hand-edited
    /// `appearance_prefs.json` — this validates that
    /// `load_appearance_prefs` clamps an out-of-range `font_size_px`
    /// to 12–20 and wraps night times into 0..1440, while in-range
    /// values pass through untouched (roundtrip, corrupt-fallback,
    /// night-case and pure-clamp behavior are covered by the tests
    /// above).
    #[test]
    fn appearance_prefs_load_sanitizes_out_of_range() {
        let dir =
            std::env::temp_dir().join(format!("quill-appearance-sanitize-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let paths = AccountPaths::for_root(&dir, &AccountKey::primary());
        fs::create_dir_all(&paths.root).unwrap();

        // Out-of-range values: font clamps, night times wrap.
        fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"font_size_px": 255, "night_start_minutes": 5000, "night_end_minutes": 1500}"#,
        )
        .unwrap();
        let prefs = load_appearance_prefs(&paths);
        assert_eq!(prefs.font_size_px, FONT_SIZE_MAX);
        assert_eq!(prefs.night_start_minutes, 5000 % (24 * 60));
        assert_eq!(prefs.night_end_minutes, 1500 % (24 * 60));

        // In-range values are not touched.
        fs::write(
            paths.root.join("appearance_prefs.json"),
            br#"{"font_size_px": 16, "night_start_minutes": 1380, "night_end_minutes": 420}"#,
        )
        .unwrap();
        let prefs = load_appearance_prefs(&paths);
        assert_eq!(prefs.font_size_px, 16);
        assert_eq!(prefs.night_start_minutes, 1380);
        assert_eq!(prefs.night_end_minutes, 420);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Slice parity:chatlist-badge-settings: local-only app badge counter
/// preferences, persisted as JSON next to the account root
/// (`badge_prefs.json`). Client-side only — the tray badge count is a
/// desktop-client concern (TDLib 1.8.67 has no badge settings):
/// - `include_muted`: count muted chats (Telegram Desktop default: ON —
///   its `_includeMutedCounter` is true)
/// - `include_archived`: count archived chats (default OFF, both clients)
/// - `count_messages`: sum unread messages vs count unread chats
///   (default: messages)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadgePrefs {
    #[serde(default = "default_true")]
    pub include_muted: bool,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default = "default_true")]
    pub count_messages: bool,
}

impl Default for BadgePrefs {
    fn default() -> Self {
        Self {
            include_muted: true,
            include_archived: false,
            count_messages: true,
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
    let path = badge_prefs_path(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(prefs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod badge_prefs_tests {
    use super::*;

    #[test]
    fn badge_prefs_default_matches_telegram_desktop() {
        let prefs = BadgePrefs::default();
        assert!(prefs.include_muted);
        assert!(!prefs.include_archived);
        assert!(prefs.count_messages);
    }

    #[test]
    fn badge_prefs_serde_roundtrip() {
        let prefs = BadgePrefs {
            include_muted: false,
            include_archived: true,
            count_messages: false,
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

#[cfg(test)]
mod account_registry_tests {
    use super::*;

    fn tmp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-accounts-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn missing_registry_falls_back_to_primary() {
        let root = tmp_root("missing");
        let accounts = list_accounts(&root);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].key, AccountKey::primary());
        assert_eq!(active_account(&root), AccountKey::primary());
    }

    #[test]
    fn corrupt_registry_falls_back_to_primary() {
        let root = tmp_root("corrupt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("accounts.json"), b"not json{{").unwrap();
        assert_eq!(
            list_accounts(&root),
            list_accounts(&tmp_root("missing-never-written"))
        );
        assert_eq!(active_account(&root), AccountKey::primary());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn add_list_remove_roundtrip() {
        let root = tmp_root("roundtrip");
        let k1 = add_account(&root, "Work").expect("add works");
        let k2 = add_account(&root, "Personal").expect("add works");
        // Collision-free keys.
        assert_ne!(k1, k2);
        assert_ne!(k1, AccountKey::primary());
        let keys: Vec<String> = list_accounts(&root)
            .iter()
            .map(|r| r.key.0.clone())
            .collect();
        assert_eq!(
            keys,
            vec!["primary".to_string(), k1.0.clone(), k2.0.clone()]
        );
        // display names survive the round-trip.
        let records = list_accounts(&root);
        let names: Vec<&str> = records.iter().map(|r| r.display_name.as_str()).collect();
        assert_eq!(names, vec!["Primary", "Work", "Personal"]);

        // Removing deletes the account's directory tree.
        let dir = root.join("accounts").join(&k2.0);
        std::fs::create_dir_all(dir.join("tdlib")).unwrap();
        remove_account(&root, &k2).expect("remove works");
        assert!(!root.join("accounts").join(&k2.0).exists());
        let keys: Vec<String> = list_accounts(&root)
            .iter()
            .map(|r| r.key.0.clone())
            .collect();
        assert_eq!(keys, vec!["primary".to_string(), k1.0.clone()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn remove_refuses_last_remaining_account() {
        let root = tmp_root("last");
        // Fresh registry holds only primary.
        assert!(matches!(
            remove_account(&root, &AccountKey::primary()),
            Err(RemoveAccountError::LastAccount)
        ));
        let k1 = add_account(&root, "Work").expect("add works");
        remove_account(&root, &AccountKey::primary())
            .expect("removing primary is fine while others remain");
        // Now k1 is the last one — removal is refused, no panic.
        assert!(matches!(
            remove_account(&root, &k1),
            Err(RemoveAccountError::LastAccount)
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn active_account_persists_and_rejects_unknown() {
        let root = tmp_root("active");
        let k1 = add_account(&root, "Work").expect("add works");
        set_active_account(&root, &k1).expect("set works");
        assert_eq!(active_account(&root), k1);
        // Unknown key ⇒ NotFound, and the active account is unchanged.
        let err = set_active_account(&root, &AccountKey::new("nope").unwrap()).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(active_account(&root), k1);
        // Removing the active account falls back to a remaining one.
        remove_account(&root, &k1).expect("remove works");
        assert_eq!(active_account(&root), AccountKey::primary());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn account_paths_are_isolated_per_account() {
        let root = PathBuf::from("/tmp/quill-accounts-test-isolation");
        let p1 = AccountPaths::for_root(&root, &AccountKey::primary());
        let p2 = AccountPaths::for_root(&root, &AccountKey::new("account-1").unwrap());
        assert!(p1.tdlib_database.ends_with("accounts/primary/tdlib"));
        assert!(p2.tdlib_database.ends_with("accounts/account-1/tdlib"));
        assert_ne!(p1.tdlib_database, p2.tdlib_database);
        assert_ne!(p1.tdlib_files, p2.tdlib_files);
    }
}
