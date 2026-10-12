//! The system battery saver, which forces every power-saving flag on
//! (tdesktop `base::BatterySaving` and `Core::Application`'s
//! `PowerSaving::SetForceAll(saving && !ignore)`).
//!
//! tdesktop reads the signal from the platform: macOS Low Power Mode, the
//! Windows battery saver, and on Linux the power-saver profile. When the
//! platform gives no answer the "Save Power on Low Battery" switch is not
//! offered and nothing is forced, which is `state() == None` here.
//!
//! The probe may block (Linux runs `gdbus`), so callers run it on a
//! background thread and hand the answer to [`set_state`].

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

const UNKNOWN: u8 = 0;
const OFF: u8 = 1;
const ON: u8 = 2;

static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);
/// tdesktop `Settings::ignoreBatterySaving`: the user turned the switch off.
static IGNORE: AtomicBool = AtomicBool::new(false);

/// The last probed answer: `Some(saving)`, or `None` when the platform has
/// no signal (tdesktop's empty `std::optional`).
pub fn state() -> Option<bool> {
    match STATE.load(Ordering::Relaxed) {
        UNKNOWN => None,
        value => Some(value == ON),
    }
}

/// Record a probe answer.
pub fn set_state(saving: Option<bool>) {
    let value = match saving {
        None => UNKNOWN,
        Some(false) => OFF,
        Some(true) => ON,
    };
    STATE.store(value, Ordering::Relaxed);
}

/// The "Save Power on Low Battery" switch is off.
pub fn set_ignore(ignore: bool) {
    IGNORE.store(ignore, Ordering::Relaxed);
}

pub fn ignored() -> bool {
    IGNORE.load(Ordering::Relaxed)
}

/// Whether every flag is forced on: the saver is on and not ignored.
pub const fn forcing_for(saving: Option<bool>, ignore: bool) -> bool {
    matches!(saving, Some(true)) && !ignore
}

/// [`forcing_for`] with the current state.
pub fn forcing() -> bool {
    forcing_for(state(), ignored())
}

/// Push the current answer into the power-saving flags.
pub fn apply() {
    crate::power_saving::set_force_all(forcing());
}

/// Ask the platform. May block; run it off the UI thread.
pub fn probe() -> Option<bool> {
    probe_native()
}

#[cfg(all(target_os = "macos", feature = "ui"))]
fn probe_native() -> Option<bool> {
    use objc2_foundation::NSProcessInfo;
    Some(NSProcessInfo::processInfo().isLowPowerModeEnabled())
}

#[cfg(windows)]
fn probe_native() -> Option<bool> {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    // SAFETY: the pointer references a live local the call fills in.
    let ok = unsafe { GetSystemPowerStatus(&mut status) } != 0;
    ok.then(|| status.SystemStatusFlag == 1)
}

#[cfg(target_os = "linux")]
fn probe_native() -> Option<bool> {
    let run = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("gdbus")
            .args(args)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    // `org.freedesktop.portal.PowerProfileMonitor`, the property GIO's
    // `GPowerProfileMonitor` is backed by.
    run(&[
        "call",
        "--session",
        "--dest",
        "org.freedesktop.portal.Desktop",
        "--object-path",
        "/org/freedesktop/portal/desktop",
        "--method",
        "org.freedesktop.DBus.Properties.Get",
        "org.freedesktop.portal.PowerProfileMonitor",
        "power-saver-enabled",
    ])
    .and_then(|text| parse_gdbus_bool(&text))
    .or_else(|| {
        // Without the portal, the profile daemon itself.
        run(&[
            "call",
            "--system",
            "--dest",
            "net.hadess.PowerProfiles",
            "--object-path",
            "/net/hadess/PowerProfiles",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "net.hadess.PowerProfiles",
            "ActiveProfile",
        ])
        .and_then(|text| parse_active_profile(&text))
    })
}

#[cfg(not(any(all(target_os = "macos", feature = "ui"), windows, target_os = "linux")))]
fn probe_native() -> Option<bool> {
    None
}

/// `(<true>,)` from `gdbus call`.
pub fn parse_gdbus_bool(text: &str) -> Option<bool> {
    let text = text.trim();
    if text.contains("true") {
        Some(true)
    } else if text.contains("false") {
        Some(false)
    } else {
        None
    }
}

/// `(<'power-saver'>,)` from `gdbus call`.
pub fn parse_active_profile(text: &str) -> Option<bool> {
    let start = text.find('\'')? + 1;
    let end = start + text[start..].find('\'')?;
    Some(&text[start..end] == "power-saver")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_only_while_the_saver_is_on_and_not_ignored() {
        assert!(forcing_for(Some(true), false));
        assert!(!forcing_for(Some(true), true));
        assert!(!forcing_for(Some(false), false));
        assert!(!forcing_for(None, false), "no signal forces nothing");
    }

    #[test]
    fn gdbus_answers_parse() {
        assert_eq!(parse_gdbus_bool("(<true>,)\n"), Some(true));
        assert_eq!(parse_gdbus_bool("(<false>,)"), Some(false));
        assert_eq!(parse_gdbus_bool("Error: no such interface"), None);
        assert_eq!(parse_active_profile("(<'power-saver'>,)"), Some(true));
        assert_eq!(parse_active_profile("(<'balanced'>,)"), Some(false));
        assert_eq!(parse_active_profile(""), None);
    }
}
