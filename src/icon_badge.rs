//! Unread badge on the app/taskbar icon (parity:platform-app-icon-badge).
//!
//! Linux only: the Unity `com.canonical.Unity.LauncherEntry` `Update`
//! signal — the de-facto standard Telegram Desktop uses on Linux, honored
//! by Ubuntu Dock, Dash-to-Dock, and KDE's task manager. The signal is
//! emitted through `dbus-send`, which ships with the base D-Bus install:
//! no new dependency and no hand-rolled D-Bus wire protocol. It is a
//! silent no-op when `dbus-send` or the session bus is absent (same
//! convention as the tray module).
//!
//! The count is [`tray::badge_count`] — the same `BadgePrefs`-governed
//! total as the system tray badge (muted chats included by default,
//! archived excluded, saturating sum). There is deliberately no second
//! unread definition here.
//!
//! `sync_icon_badge` is called from the 1s UI-thread timer in `main.rs`
//! and emits only when the count changes. Unread = 0 sends
//! `count-visible: false`, which also clears any stale badge left by a
//! previous run on the first tick.
//!
//! The dock resolves the badge against the app's installed desktop file;
//! Quill ships no `.desktop` file yet (packaging is a separate slice), so
//! [`APP_URI`] names the `quill.desktop` id a future install will satisfy.

use crate::state::Session;
#[cfg(target_os = "linux")]
use crate::tray;

/// App URI carried by the LauncherEntry signal: `application://` +
/// the desktop-file id docks match against.
pub const APP_URI: &str = "application://quill.desktop";

#[cfg(target_os = "linux")]
thread_local! {
    /// Last count pushed to the dock. Starts unset so the first tick
    /// always emits (clearing a stale badge when unread is 0).
    static LAST_SENT: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Sync the taskbar/dock app-icon badge with the session's unread count.
/// Linux-only; a no-op on every other OS.
#[cfg(target_os = "linux")]
pub fn sync_icon_badge(session: Option<&Session>) {
    let unread = session
        .map(|s| tray::badge_count(s, &s.badge_prefs))
        .unwrap_or(0);
    LAST_SENT.with(|last| {
        if last.get() == Some(unread) {
            return;
        }
        last.set(Some(unread));
        emit_launcher_entry(unread);
    });
}

/// Non-Linux stub: Windows/macOS badge APIs are out of scope for this slice.
#[cfg(not(target_os = "linux"))]
pub fn sync_icon_badge(_session: Option<&Session>) {}

/// `dbus-send` argv for the broadcast `Update` signal:
/// `(string app_uri, dict<string, variant> {count: int64, count-visible: bool})`.
#[cfg(any(target_os = "linux", test))]
fn launcher_entry_args(unread: u32) -> Vec<String> {
    vec![
        "--session".to_string(),
        "--type=signal".to_string(),
        "/".to_string(),
        "com.canonical.Unity.LauncherEntry.Update".to_string(),
        format!("string:{APP_URI}"),
        format!(
            "dict:string:variant:\"count\",int64:{unread},\"count-visible\",boolean:{}",
            unread > 0
        ),
    ]
}

#[cfg(target_os = "linux")]
fn emit_launcher_entry(unread: u32) {
    // Fire-and-wait: dbus-send exits immediately for signals, and this runs
    // only when the count changed. Any failure (no dbus-send, no session
    // bus, no listening dock) is silently ignored.
    let _ = std::process::Command::new("dbus-send")
        .args(launcher_entry_args(unread))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_target_launcher_entry_update_signal() {
        let args = launcher_entry_args(5);
        assert_eq!(
            &args[..5],
            [
                "--session",
                "--type=signal",
                "/",
                "com.canonical.Unity.LauncherEntry.Update",
                "string:application://quill.desktop",
            ]
        );
    }

    #[test]
    fn args_encode_count_and_visibility() {
        let body = launcher_entry_args(5)[5].clone();
        assert!(body.contains("\"count\",int64:5"), "{body}");
        assert!(body.contains("\"count-visible\",boolean:true"), "{body}");

        // Zero unread hides the badge (and clears a stale one).
        let body = launcher_entry_args(0)[5].clone();
        assert!(body.contains("\"count\",int64:0"), "{body}");
        assert!(body.contains("\"count-visible\",boolean:false"), "{body}");
    }
}
