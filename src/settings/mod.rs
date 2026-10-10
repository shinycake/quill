//! Small non-message preferences. Message history lives in TDLib.

use crate::ids::AccountKey;
use crate::sticker_suggest::StickerSuggestMode;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod accounts;
mod appearance;
mod badge;
mod chat_prefs;
mod media;
pub use accounts::*;
pub use appearance::*;
pub use badge::*;
pub use chat_prefs::*;
pub use media::*;

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
    /// tdesktop `desktopNotify` ("Desktop notifications" / the tray's
    /// "Disable notifications"): when off, no OS notification is shown.
    #[serde(default = "default_true")]
    pub desktop_notifications: bool,
    /// Parity slice (platform-custom-keybindings): user-overridden shortcuts,
    /// one per rebindable action id.
    #[serde(default)]
    pub custom_keybindings: Vec<CustomKeybinding>,
}

/// Parity slice (platform-custom-keybindings): one user-overridden shortcut.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomKeybinding {
    /// Stable action id from `REBINDABLE_ACTIONS` (e.g. "focus-composer").
    pub id: String,
    /// Keystroke string as parsed by `KeyBinding::new` (e.g. "ctrl-shift-l").
    pub keystroke: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: PREFS_VERSION,
            account: AccountKey::primary(),
            hide_notification_previews: true,
            inapp_sounds_enabled: true,
            desktop_notifications: true,
            custom_keybindings: Vec::new(),
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
    /// Group-call push-to-talk (key + release delay). Client-side only.
    #[serde(default)]
    pub push_to_talk: crate::calls::ptt::PttConfig,
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
    write_json_atomic(&paths.root.join(file_name), prefs)
}

/// Write `value` as JSON to `path` atomically: a sibling temp file is
/// written, synced, and renamed over the target, so a crash mid-write
/// leaves the previous file intact instead of a truncated one that would
/// silently reset the settings to defaults.
pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no parent dir"))?;
    std::fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prefs.json");
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = parent.join(format!(".{file_name}.tmp-{}-{seq}", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // `std::fs::rename` replaces an existing target on every platform
        // (POSIX rename; MoveFileExW with MOVEFILE_REPLACE_EXISTING on Windows).
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    sync_dir(parent);
    result
}

/// Persist the rename itself (directory entry) where the platform lets a
/// directory be opened and synced; best effort, a no-op on Windows.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = std::fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

/// Main-window geometry, app-wide (`window_state.json` at the app root):
/// restored on launch so the window reopens where the user left it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
}

fn default_sidebar_width() -> f32 {
    DEFAULT_SIDEBAR_WIDTH
}

/// Chat list column width when nothing is stored.
pub const DEFAULT_SIDEBAR_WIDTH: f32 = 300.;
/// Resizable chat list column bounds.
pub const MIN_SIDEBAR_WIDTH: f32 = 240.;
pub const MAX_SIDEBAR_WIDTH: f32 = 560.;

impl WindowState {
    /// Sizes that can't be a real window (corrupt file, 0×0 from a
    /// minimized save) are rejected; the sidebar width is clamped.
    pub fn sanitized(mut self) -> Option<Self> {
        let finite = [self.x, self.y, self.width, self.height, self.sidebar_width]
            .iter()
            .all(|v| v.is_finite());
        if !finite || self.width < 480. || self.height < 360. {
            return None;
        }
        self.sidebar_width = self
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        Some(self)
    }
}

/// The stored main-window geometry, when there is a usable one.
pub fn load_window_state() -> Option<WindowState> {
    let path = safe_app_root()?.join("window_state.json");
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<WindowState>(&bytes)
        .ok()?
        .sanitized()
}

pub fn save_window_state(state: &WindowState) -> std::io::Result<()> {
    let root = safe_app_root().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no app data directory")
    })?;
    write_json_atomic(&root.join("window_state.json"), state)
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

/// `parity:proxy-settings`: client-side auto-switch preferences
/// (`proxy_prefs.json`); the proxy list itself lives in TDLib.
pub fn load_proxy_prefs(paths: &AccountPaths) -> crate::proxy::ProxyPrefs {
    load_json_prefs(paths, "proxy_prefs.json")
}

pub fn save_proxy_prefs(
    paths: &AccountPaths,
    prefs: &crate::proxy::ProxyPrefs,
) -> std::io::Result<()> {
    save_json_prefs(paths, "proxy_prefs.json", prefs)
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn atomic_json_write_replaces_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("quill-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("prefs.json");
        write_json_atomic(&path, &serde_json::json!({"a": 1})).unwrap();
        write_json_atomic(&path, &serde_json::json!({"a": 2})).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["a"], 2);
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn window_state_rejects_unusable_geometry_and_clamps_sidebar() {
        let state = WindowState {
            x: 10.,
            y: 20.,
            width: 1200.,
            height: 800.,
            maximized: false,
            sidebar_width: 9000.,
        };
        assert_eq!(state.sanitized().unwrap().sidebar_width, MAX_SIDEBAR_WIDTH);
        assert!(WindowState { width: 0., ..state }.sanitized().is_none());
        assert!(
            WindowState {
                x: f32::NAN,
                ..state
            }
            .sanitized()
            .is_none()
        );
        // A file without the sidebar field still loads.
        let legacy: WindowState =
            serde_json::from_str(r#"{"x":0,"y":0,"width":1000,"height":700}"#).unwrap();
        assert_eq!(legacy.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
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
            push_to_talk: crate::calls::ptt::PttConfig {
                enabled: true,
                key: "f13".into(),
                release_delay_ms: 500,
            },
        };
        save_call_prefs(&paths, &prefs).expect("save works");
        assert_eq!(load_call_prefs(&paths), prefs);
        // Corrupt file → defaults, never a panic.
        std::fs::write(dir.join("accounts/primary/call_prefs.json"), b"not json").unwrap();
        assert_eq!(load_call_prefs(&paths), CallPrefs::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
