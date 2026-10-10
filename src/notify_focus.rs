//! System Focus / Do Not Disturb awareness and the alert plan.
//!
//! tdesktop (`window/notifications_manager.cpp`, `platform/*/notifications_
//! manager_*`) lets the OS decide about banners, but skips its own sound and
//! its Dock bounce / taskbar flash while the system is in Do Not Disturb:
//! macOS reads the `doNotDisturb` preference of `com.apple.notificationcenterui`,
//! Windows asks `SHQueryUserNotificationState` plus Focus Assist, Linux reads
//! the `org.freedesktop.Notifications` `Inhibited` property. Quill does the
//! same, and also looks at the macOS Focus assertion file and GNOME's
//! `show-banners` switch.
//!
//! Every query shells out or calls the OS, so none of them runs on the UI
//! thread: [`dnd_active`] returns the last answer and, when it is stale,
//! refreshes it on a short-lived worker. The parsers are pure and tested.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// How long one answer is trusted. tdesktop re-queries macOS every few
/// seconds as well (`kQuerySettingsEachMs`).
const FRESH_MS: u64 = 5_000;

static DND: AtomicBool = AtomicBool::new(false);
static CHECKED_AT_MS: AtomicU64 = AtomicU64::new(0);
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether the OS is in Do Not Disturb / Focus right now (last known
/// answer; `false` until the first refresh lands). Never blocks.
pub fn dnd_active() -> bool {
    let checked = CHECKED_AT_MS.load(Ordering::Relaxed);
    if now_ms().saturating_sub(checked) > FRESH_MS && !REFRESHING.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new()
            .name("quill-dnd".to_string())
            .spawn(|| {
                DND.store(query_os_dnd(), Ordering::Relaxed);
                CHECKED_AT_MS.store(now_ms(), Ordering::Relaxed);
                REFRESHING.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            REFRESHING.store(false, Ordering::Release);
        }
    }
    DND.load(Ordering::Relaxed)
}

/// Start the first query early so the first notification already knows.
pub fn warm() {
    let _ = dnd_active();
}

/// What a new notification does besides the OS banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertInput {
    /// "Bounce the Dock icon / Flash the taskbar icon" setting.
    pub flash_enabled: bool,
    /// The decision already made for the sound (`Some` when one plays).
    pub sound_wanted: bool,
    /// The OS is in Do Not Disturb / Focus.
    pub dnd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertPlan {
    pub flash: bool,
    pub sound: bool,
}

/// tdesktop's `doMaybePlaySound` / `doMaybeFlashBounce`: both are skipped
/// while Do Not Disturb is on.
pub fn plan_alert(input: AlertInput) -> AlertPlan {
    AlertPlan {
        flash: input.flash_enabled && !input.dnd,
        sound: input.sound_wanted && !input.dnd,
    }
}

/// The settings row text, as tdesktop words it per platform
/// (`lng_settings_alert_*`).
pub fn attention_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Bounce the Dock icon"
    } else if cfg!(windows) {
        "Flash the taskbar icon"
    } else {
        "Draw attention to the window"
    }
}

/// `defaults read com.apple.notificationcenterui doNotDisturb` prints `1`
/// or `0`; a missing key is an error with no stdout.
pub fn parse_defaults_bool(output: &str) -> bool {
    matches!(output.trim(), "1" | "true" | "YES")
}

/// `gdbus call ... Properties.Get ... Inhibited` prints `(<true>,)`.
pub fn parse_gdbus_bool(output: &str) -> bool {
    output.contains("<true>")
}

/// `gsettings get org.gnome.desktop.notifications show-banners` prints
/// `true` or `false`; banners off is GNOME's Do Not Disturb.
pub fn gnome_banners_off(output: &str) -> bool {
    output.trim() == "false"
}

/// `~/Library/DoNotDisturb/DB/Assertions.json`: a Focus is on when the
/// first store has assertion records. (The file needs Full Disk Access;
/// without it the read fails and the preference check decides.)
pub fn focus_assertions_active(json: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return false;
    };
    value
        .get("data")
        .and_then(|data| data.as_array())
        .and_then(|stores| stores.first())
        .and_then(|store| store.get("storeAssertionRecords"))
        .and_then(|records| records.as_array())
        .is_some_and(|records| !records.is_empty())
}

/// `QUERY_USER_NOTIFICATION_STATE` values that silence alerts: no user at the
/// machine (1), presentation mode (4) and quiet time / Focus Assist (6).
/// Full-screen apps (2, 3) only hide banners, which Windows handles itself.
pub fn windows_state_is_dnd(state: i32) -> bool {
    matches!(state, 1 | 4 | 6)
}

#[cfg(target_os = "macos")]
fn query_os_dnd() -> bool {
    let pref = std::process::Command::new("defaults")
        .args(["read", "com.apple.notificationcenterui", "doNotDisturb"])
        .output()
        .ok()
        .is_some_and(|out| parse_defaults_bool(&String::from_utf8_lossy(&out.stdout)));
    if pref {
        return true;
    }
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join("Library/DoNotDisturb/DB/Assertions.json"))
        .and_then(|path| std::fs::read_to_string(path).ok())
        .is_some_and(|json| focus_assertions_active(&json))
}

#[cfg(windows)]
fn query_os_dnd() -> bool {
    use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
    let mut state: i32 = 0;
    // SAFETY: `state` is a valid, writable out parameter for the call.
    let hr = unsafe { SHQueryUserNotificationState(&mut state) };
    hr >= 0 && windows_state_is_dnd(state)
}

#[cfg(target_os = "linux")]
fn query_os_dnd() -> bool {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let inhibited = run(
        "gdbus",
        &[
            "call",
            "--session",
            "--dest",
            "org.freedesktop.Notifications",
            "--object-path",
            "/org/freedesktop/Notifications",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.freedesktop.Notifications",
            "Inhibited",
        ],
    )
    .is_some_and(|out| parse_gdbus_bool(&out));
    inhibited
        || run(
            "gsettings",
            &["get", "org.gnome.desktop.notifications", "show-banners"],
        )
        .is_some_and(|out| gnome_banners_off(&out))
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn query_os_dnd() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnd_silences_sound_and_flash() {
        let plan = plan_alert(AlertInput {
            flash_enabled: true,
            sound_wanted: true,
            dnd: true,
        });
        assert_eq!(
            plan,
            AlertPlan {
                flash: false,
                sound: false
            }
        );
    }

    #[test]
    fn flash_and_sound_are_independent_outside_dnd() {
        let plan = plan_alert(AlertInput {
            flash_enabled: true,
            sound_wanted: false,
            dnd: false,
        });
        assert!(plan.flash && !plan.sound);
        let plan = plan_alert(AlertInput {
            flash_enabled: false,
            sound_wanted: true,
            dnd: false,
        });
        assert!(!plan.flash && plan.sound);
    }

    #[test]
    fn parses_defaults_output() {
        assert!(parse_defaults_bool("1\n"));
        assert!(!parse_defaults_bool("0\n"));
        assert!(!parse_defaults_bool(""));
    }

    #[test]
    fn parses_gdbus_inhibited() {
        assert!(parse_gdbus_bool("(<true>,)\n"));
        assert!(!parse_gdbus_bool("(<false>,)\n"));
        assert!(!parse_gdbus_bool(""));
    }

    #[test]
    fn gnome_banners_off_is_dnd() {
        assert!(gnome_banners_off("false\n"));
        assert!(!gnome_banners_off("true\n"));
        assert!(!gnome_banners_off("No such schema"));
    }

    #[test]
    fn focus_assertions_need_records() {
        assert!(focus_assertions_active(
            r#"{"data":[{"storeAssertionRecords":[{"assertionDetails":{}}]}]}"#
        ));
        assert!(!focus_assertions_active(
            r#"{"data":[{"storeAssertionRecords":[]}]}"#
        ));
        assert!(!focus_assertions_active(r#"{"data":[{}]}"#));
        assert!(!focus_assertions_active("not json"));
    }

    #[test]
    fn windows_states() {
        assert!(windows_state_is_dnd(4));
        assert!(windows_state_is_dnd(6));
        assert!(windows_state_is_dnd(1));
        assert!(!windows_state_is_dnd(2));
        assert!(!windows_state_is_dnd(5));
    }
}
