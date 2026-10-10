//! System-wide push-to-talk: the pure parts (key tables, edge filter, hook
//! lifecycle) plus the platform backends.
//!
//! tdesktop (`calls/group/calls_group_settings.cpp`, `base::GlobalShortcuts`):
//! the push-to-talk shortcut works while the app is in the background. On
//! macOS that needs Input Monitoring, and tdesktop explains it in a box with
//! an "Open Settings" button instead of asking silently.
//!
//! Backends only *observe* keys (they never swallow them) and only run while
//! the caller keeps a handle alive, that is, while a group call is joined with
//! push-to-talk on. Key events are never logged.

use std::sync::Arc;

#[cfg(target_os = "macos")]
#[path = "ptt_global_mac.rs"]
mod platform;
#[cfg(windows)]
#[path = "ptt_global_win.rs"]
mod platform;
#[cfg(all(unix, not(target_os = "macos")))]
#[path = "ptt_global_x11.rs"]
mod platform;
#[cfg(not(any(unix, windows)))]
mod platform {
    use super::*;
    #[derive(Debug)]
    pub struct Handle;
    pub fn start(_key: &str, _sink: Sink) -> Result<Handle, StartError> {
        Err(StartError::Unsupported("No system-wide keys here.".into()))
    }
    pub fn static_limit() -> Option<String> {
        Some("System-wide shortcuts are not available on this system.".into())
    }
    pub fn permission_granted() -> bool {
        true
    }
    pub fn open_permission_settings() {}
}

/// Live hook; dropping it stops listening.
pub type Handle = platform::Handle;

/// A press or release of the configured key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEdge {
    Down,
    Up,
}

/// Receives edges on the hook thread; must be quick and non-blocking.
pub type Sink = Arc<dyn Fn(KeyEdge) + Send + Sync>;

/// Why a hook could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartError {
    /// macOS Input Monitoring is not granted.
    PermissionDenied,
    /// This system (or key) has no system-wide support.
    Unsupported(String),
    Failed(String),
}

/// Where the hook stands, for Settings.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HookState {
    /// No hook wanted right now (no joined call, or push-to-talk off).
    #[default]
    Off,
    Active,
    NeedsPermission,
    Unsupported(String),
    Failed(String),
}

impl From<&StartError> for HookState {
    fn from(err: &StartError) -> Self {
        match err {
            StartError::PermissionDenied => HookState::NeedsPermission,
            StartError::Unsupported(why) => HookState::Unsupported(why.clone()),
            StartError::Failed(why) => HookState::Failed(why.clone()),
        }
    }
}

/// Collapses repeated edges (OS auto-repeat sends many downs).
#[derive(Debug, Clone, Default)]
pub struct KeyFilter {
    target: u32,
    held: bool,
}

impl KeyFilter {
    pub fn new(target: u32) -> Self {
        KeyFilter {
            target,
            held: false,
        }
    }

    /// Feed one raw event; returns an edge only when the state changes.
    pub fn feed(&mut self, code: u32, pressed: bool) -> Option<KeyEdge> {
        if code != self.target || self.held == pressed {
            return None;
        }
        self.held = pressed;
        Some(if pressed { KeyEdge::Down } else { KeyEdge::Up })
    }
}

/// macOS virtual key code (`kVK_*`, ANSI positions) for a GPUI key name.
pub fn mac_keycode(key: &str) -> Option<u32> {
    let code = match key {
        "a" => 0,
        "s" => 1,
        "d" => 2,
        "f" => 3,
        "h" => 4,
        "g" => 5,
        "z" => 6,
        "x" => 7,
        "c" => 8,
        "v" => 9,
        "b" => 11,
        "q" => 12,
        "w" => 13,
        "e" => 14,
        "r" => 15,
        "y" => 16,
        "t" => 17,
        "1" => 18,
        "2" => 19,
        "3" => 20,
        "4" => 21,
        "6" => 22,
        "5" => 23,
        "=" => 24,
        "9" => 25,
        "7" => 26,
        "-" => 27,
        "8" => 28,
        "0" => 29,
        "]" => 30,
        "o" => 31,
        "u" => 32,
        "[" => 33,
        "i" => 34,
        "p" => 35,
        "enter" => 36,
        "l" => 37,
        "j" => 38,
        "'" => 39,
        "k" => 40,
        ";" => 41,
        "\\" => 42,
        "," => 43,
        "/" => 44,
        "n" => 45,
        "m" => 46,
        "." => 47,
        "tab" => 48,
        "space" => 49,
        "`" => 50,
        "backspace" => 51,
        "escape" => 53,
        "f17" => 64,
        "f18" => 79,
        "f19" => 80,
        "f20" => 90,
        "f5" => 96,
        "f6" => 97,
        "f7" => 98,
        "f3" => 99,
        "f8" => 100,
        "f9" => 101,
        "f11" => 103,
        "f13" => 105,
        "f16" => 106,
        "f14" => 107,
        "f10" => 109,
        "f12" => 111,
        "f15" => 113,
        "insert" => 114,
        "home" => 115,
        "pageup" => 116,
        "delete" => 117,
        "f4" => 118,
        "end" => 119,
        "f2" => 120,
        "pagedown" => 121,
        "f1" => 122,
        "left" => 123,
        "right" => 124,
        "down" => 125,
        "up" => 126,
        _ => return None,
    };
    Some(code)
}

/// Windows virtual-key code for a GPUI key name.
pub fn windows_vk(key: &str) -> Option<u32> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return match first {
            'a'..='z' => Some(0x41 + (first as u32 - 'a' as u32)),
            '0'..='9' => Some(0x30 + (first as u32 - '0' as u32)),
            ';' => Some(0xBA),
            '=' => Some(0xBB),
            ',' => Some(0xBC),
            '-' => Some(0xBD),
            '.' => Some(0xBE),
            '/' => Some(0xBF),
            '`' => Some(0xC0),
            '[' => Some(0xDB),
            '\\' => Some(0xDC),
            ']' => Some(0xDD),
            '\'' => Some(0xDE),
            _ => None,
        };
    }
    if let Some(n) = function_key(key) {
        return Some(0x70 + n - 1);
    }
    let code = match key {
        "backspace" => 0x08,
        "tab" => 0x09,
        "enter" => 0x0D,
        "escape" => 0x1B,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "insert" => 0x2D,
        "delete" => 0x2E,
        _ => return None,
    };
    Some(code)
}

/// X11 keysym for a GPUI key name (matched against the live keymap).
pub fn x11_keysym(key: &str) -> Option<u32> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return first.is_ascii_graphic().then_some(first as u32);
    }
    if let Some(n) = function_key(key) {
        return Some(0xffbd + n);
    }
    let sym = match key {
        "space" => 0x20,
        "backspace" => 0xff08,
        "tab" => 0xff09,
        "enter" => 0xff0d,
        "escape" => 0xff1b,
        "home" => 0xff50,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "pageup" => 0xff55,
        "pagedown" => 0xff56,
        "end" => 0xff57,
        "insert" => 0xff63,
        "delete" => 0xffff,
        _ => return None,
    };
    Some(sym)
}

/// `f1`..`f24` as 1..=24.
fn function_key(key: &str) -> Option<u32> {
    let digits = key.strip_prefix('f')?;
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: u32 = digits.parse().ok()?;
    (1..=24).contains(&n).then_some(n)
}

/// What kind of Linux session this is (X11 hooks see nothing on Wayland).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxSession {
    X11,
    Wayland,
    None,
}

/// Decide from `XDG_SESSION_TYPE`, `WAYLAND_DISPLAY` and `DISPLAY`.
pub fn linux_session(
    session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> LinuxSession {
    let non_empty = |v: Option<&str>| v.is_some_and(|s| !s.is_empty());
    if session_type == Some("wayland") || non_empty(wayland_display) {
        LinuxSession::Wayland
    } else if non_empty(display) {
        LinuxSession::X11
    } else {
        LinuxSession::None
    }
}

/// Owns at most one live hook and decides when to (re)start or stop it.
#[derive(Debug)]
pub struct Controller<H> {
    running: Option<(String, H)>,
    /// Key whose last start failed; not retried until [`Controller::retry`].
    failed_for: Option<String>,
    state: HookState,
}

impl<H> Default for Controller<H> {
    fn default() -> Self {
        Controller {
            running: None,
            failed_for: None,
            state: HookState::Off,
        }
    }
}

impl<H> Controller<H> {
    pub fn state(&self) -> &HookState {
        &self.state
    }

    /// Whether a hook is delivering system-wide key events.
    pub fn is_active(&self) -> bool {
        self.running.is_some()
    }

    /// Allow a failed start to be attempted again on the next `sync`.
    pub fn retry(&mut self) {
        self.failed_for = None;
    }

    /// Bring the hook in line with `want` (the key while joined with
    /// push-to-talk on, else `None`). Returns whether the state changed.
    pub fn sync(
        &mut self,
        want: Option<&str>,
        start: impl FnOnce(&str) -> Result<H, StartError>,
    ) -> bool {
        let before = self.state.clone();
        match want {
            None => {
                self.running = None;
                self.failed_for = None;
                self.state = HookState::Off;
            }
            Some(key) => {
                if self.running.as_ref().is_some_and(|(k, _)| k == key)
                    || self.failed_for.as_deref() == Some(key)
                {
                    return false;
                }
                // Stop the old key's hook before starting the new one.
                self.running = None;
                match start(key) {
                    Ok(handle) => {
                        self.running = Some((key.to_string(), handle));
                        self.failed_for = None;
                        self.state = HookState::Active;
                    }
                    Err(err) => {
                        self.failed_for = Some(key.to_string());
                        self.state = HookState::from(&err);
                    }
                }
            }
        }
        self.state != before
    }
}

/// The real controller.
pub type PlatformController = Controller<Handle>;

/// Start the platform hook for `key`.
pub fn start_platform(key: &str, sink: Sink) -> Result<Handle, StartError> {
    platform::start(key, sink)
}

/// Fixed reason this system can't do system-wide keys at all (Wayland).
pub fn static_limit() -> Option<String> {
    platform::static_limit()
}

/// Whether the OS lets us listen system-wide (macOS Input Monitoring;
/// always true elsewhere). Never prompts.
pub fn permission_granted() -> bool {
    platform::permission_granted()
}

/// Open the OS pane where the permission is granted (macOS only).
pub fn open_permission_settings() {
    platform::open_permission_settings();
}

/// One line for Settings under the shortcut row.
pub fn status_note(state: &HookState, limit: Option<&str>) -> String {
    const FALLBACK: &str = "Push-to-talk works while the voice chat window is in front.";
    match state {
        HookState::NeedsPermission => "Quill does not have access to system wide keyboard input required for Push to Talk. Allow Input Monitoring for Quill in Privacy Settings, then check again. You may need to restart the app.".into(),
        HookState::Unsupported(why) | HookState::Failed(why) => format!("{why} {FALLBACK}"),
        HookState::Active | HookState::Off => match limit {
            Some(limit) => format!("{limit} {FALLBACK}"),
            None => "Works system-wide while you are in a voice chat.".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Controller, HookState, KeyEdge, KeyFilter, LinuxSession, StartError, function_key,
        linux_session, mac_keycode, status_note, windows_vk, x11_keysym,
    };

    #[test]
    fn key_tables_cover_common_keys() {
        assert_eq!(mac_keycode("space"), Some(49));
        assert_eq!(mac_keycode("f13"), Some(105));
        assert_eq!(mac_keycode("t"), Some(17));
        assert_eq!(mac_keycode("nonsense"), None);
        assert_eq!(windows_vk("space"), Some(0x20));
        assert_eq!(windows_vk("a"), Some(0x41));
        assert_eq!(windows_vk("0"), Some(0x30));
        assert_eq!(windows_vk("f1"), Some(0x70));
        assert_eq!(windows_vk("f13"), Some(0x7C));
        assert_eq!(windows_vk(";"), Some(0xBA));
        assert_eq!(windows_vk("f25"), None);
        assert_eq!(x11_keysym("space"), Some(0x20));
        assert_eq!(x11_keysym("t"), Some('t' as u32));
        assert_eq!(x11_keysym("f1"), Some(0xffbe));
        assert_eq!(x11_keysym("f13"), Some(0xffca));
        assert_eq!(x11_keysym("delete"), Some(0xffff));
        assert_eq!(x11_keysym("fn"), None);
    }

    #[test]
    fn function_keys_parse_strictly() {
        assert_eq!(function_key("f24"), Some(24));
        assert_eq!(function_key("f0"), None);
        assert_eq!(function_key("f+1"), None);
        assert_eq!(function_key("fx"), None);
    }

    #[test]
    fn filter_ignores_other_keys_and_auto_repeat() {
        let mut filter = KeyFilter::new(49);
        assert_eq!(filter.feed(10, true), None);
        assert_eq!(filter.feed(49, true), Some(KeyEdge::Down));
        assert_eq!(filter.feed(49, true), None);
        assert_eq!(filter.feed(49, false), Some(KeyEdge::Up));
        assert_eq!(filter.feed(49, false), None);
    }

    #[test]
    fn controller_starts_once_restarts_on_key_change_and_stops() {
        let mut ctl: Controller<u32> = Controller::default();
        let starts = std::cell::Cell::new(0);
        let start = |_: &str| {
            starts.set(starts.get() + 1);
            Ok(starts.get())
        };
        assert!(ctl.sync(Some("space"), start));
        assert!(ctl.is_active());
        assert!(!ctl.sync(Some("space"), start));
        assert!(!ctl.sync(Some("t"), start));
        assert_eq!(starts.get(), 2);
        assert_eq!(ctl.state(), &HookState::Active);
        assert!(ctl.sync(None, start));
        assert!(!ctl.is_active());
        assert_eq!(ctl.state(), &HookState::Off);
        assert_eq!(starts.get(), 2);
    }

    #[test]
    fn controller_does_not_retry_failures_until_asked() {
        let mut ctl: Controller<u32> = Controller::default();
        let tries = std::cell::Cell::new(0);
        let deny = |_: &str| {
            tries.set(tries.get() + 1);
            Err::<u32, _>(StartError::PermissionDenied)
        };
        assert!(ctl.sync(Some("space"), deny));
        assert_eq!(ctl.state(), &HookState::NeedsPermission);
        assert!(!ctl.sync(Some("space"), deny));
        assert_eq!(tries.get(), 1);
        ctl.retry();
        ctl.sync(Some("space"), |_| Ok(7));
        assert!(ctl.is_active());
        // A new key is tried even after a failure on the old one.
        ctl.sync(None, |_| Ok(0));
        ctl.sync(Some("a"), |_| Err::<u32, _>(StartError::Failed("x".into())));
        ctl.sync(Some("b"), |_| Ok(1));
        assert!(ctl.is_active());
    }

    #[test]
    fn linux_session_prefers_wayland() {
        assert_eq!(
            linux_session(Some("wayland"), None, Some(":0")),
            LinuxSession::Wayland
        );
        assert_eq!(
            linux_session(Some("x11"), Some("wayland-0"), Some(":0")),
            LinuxSession::Wayland
        );
        assert_eq!(
            linux_session(Some("x11"), None, Some(":0")),
            LinuxSession::X11
        );
        assert_eq!(linux_session(None, Some(""), Some(":1")), LinuxSession::X11);
        assert_eq!(linux_session(None, None, None), LinuxSession::None);
    }

    #[test]
    fn notes_explain_limits_honestly() {
        let off = status_note(&HookState::Off, None);
        assert!(off.contains("system-wide"));
        let wl = status_note(&HookState::Off, Some("Wayland blocks system-wide keys."));
        assert!(wl.contains("in front"));
        assert!(status_note(&HookState::NeedsPermission, None).contains("Input Monitoring"));
        assert!(status_note(&HookState::Failed("Oops.".into()), None).starts_with("Oops."));
    }
}
