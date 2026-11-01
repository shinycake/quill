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
#![deny(clippy::indexing_slicing)]

use crate::power_saving::{BATTERY_IGNORE, BATTERY_STATE, THERMAL_HOT, slot_get, slot_set};

const UNKNOWN: u32 = 0;
const OFF: u32 = 1;
const ON: u32 = 2;

/// The last probed answer: `Some(saving)`, or `None` when the platform has
/// no signal (tdesktop's empty `std::optional`).
pub fn state() -> Option<bool> {
    match slot_get(BATTERY_STATE) {
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
    slot_set(BATTERY_STATE, value);
}

/// tdesktop `Settings::ignoreBatterySaving`: the "Save Power on Low
/// Battery" switch is off.
pub fn set_ignore(ignore: bool) {
    slot_set(BATTERY_IGNORE, ignore.into());
}

pub fn ignored() -> bool {
    slot_get(BATTERY_IGNORE) != 0
}

/// The system thermal pressure, mirroring GPUI's `ThermalState` without
/// tying this module to the UI feature. tdesktop has no such input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thermal {
    Nominal,
    Fair,
    Serious,
    Critical,
}

/// Whether thermal pressure counts as "hot" next, given whether it did
/// before. Enters at Serious or Critical and leaves only at Nominal, so
/// Fair holds the previous answer and a state hovering around the
/// Serious/Fair boundary does not flap the whole UI between modes.
pub const fn thermal_hot_next(was_hot: bool, thermal: Thermal) -> bool {
    match thermal {
        Thermal::Serious | Thermal::Critical => true,
        Thermal::Fair => was_hot,
        Thermal::Nominal => false,
    }
}

/// Record a thermal reading; returns whether the hot latch is now set.
pub fn set_thermal(thermal: Thermal) -> bool {
    let hot = thermal_hot_next(thermal_hot(), thermal);
    slot_set(THERMAL_HOT, hot.into());
    hot
}

/// The latched thermal answer.
pub fn thermal_hot() -> bool {
    slot_get(THERMAL_HOT) != 0
}

/// Whether every flag is forced on: the battery saver is on, or the system
/// is thermally constrained, and the "Save Power on Low Battery" switch
/// (`ignore`) is not off. Thermal pressure reuses that switch, so there is
/// one control for "let the system force power saving".
pub const fn forcing_for(saving: Option<bool>, thermal_hot: bool, ignore: bool) -> bool {
    (matches!(saving, Some(true)) || thermal_hot) && !ignore
}

/// [`forcing_for`] with the current state.
pub fn forcing() -> bool {
    forcing_for(state(), thermal_hot(), ignored())
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
    #[cfg(feature = "ui")]
    let announced = crate::battery_signal::refresh();
    #[cfg(not(feature = "ui"))]
    let announced = None;
    probe_linux(announced, |args| {
        crate::child_timeout::stdout_within(
            "gdbus",
            args,
            crate::child_timeout::DESKTOP_QUERY_LIMIT,
        )
    })
}

/// Start following the platform's announcements where it makes them, so
/// [`probe`] needs no process (Linux, see `battery_signal`).
pub fn start_events() {
    #[cfg(all(target_os = "linux", feature = "ui"))]
    crate::battery_signal::start();
}

/// Whether the answer arrives by signal, so the watcher need not poll.
#[cfg(all(target_os = "linux", feature = "ui"))]
pub fn event_driven() -> bool {
    crate::battery_signal::latest().is_some()
}

#[cfg(not(all(target_os = "linux", feature = "ui")))]
pub fn event_driven() -> bool {
    false
}

/// The value the D-Bus listener read (`announced`), or else ask with
/// `run`, which spawns `gdbus`.
#[cfg(target_os = "linux")]
pub(crate) fn probe_linux(
    announced: Option<Option<bool>>,
    run: impl Fn(&[&str]) -> Option<String>,
) -> Option<bool> {
    if let Some(answer) = announced {
        return answer;
    }
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
    let end = start + text.get(start..).unwrap_or_default().find('\'')?;
    Some(text.get(start..end).unwrap_or_default() == "power-saver")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_only_while_the_saver_is_on_and_not_ignored() {
        assert!(forcing_for(Some(true), false, false));
        assert!(!forcing_for(Some(true), false, true));
        assert!(!forcing_for(Some(false), false, false));
        assert!(!forcing_for(None, false, false), "no signal forces nothing");
    }

    #[test]
    fn thermal_pressure_forces_like_the_saver_and_obeys_the_switch() {
        for saving in [None, Some(false), Some(true)] {
            for hot in [false, true] {
                for ignore in [false, true] {
                    let want = (saving == Some(true) || hot) && !ignore;
                    assert_eq!(forcing_for(saving, hot, ignore), want);
                }
            }
        }
        assert!(
            forcing_for(None, true, false),
            "no battery signal still heats"
        );
        assert!(!forcing_for(Some(false), true, true), "ignore wins");
    }

    #[test]
    fn thermal_latch_enters_on_serious_and_leaves_only_at_nominal() {
        use Thermal::*;
        assert!(!thermal_hot_next(false, Nominal));
        assert!(!thermal_hot_next(false, Fair), "Fair alone does not enter");
        assert!(thermal_hot_next(false, Serious));
        assert!(thermal_hot_next(false, Critical));
        assert!(thermal_hot_next(true, Serious));
        assert!(thermal_hot_next(true, Fair), "Fair holds while hot");
        assert!(!thermal_hot_next(true, Nominal));
        let mut hot = false;
        for state in [Serious, Fair, Serious, Fair, Fair] {
            hot = thermal_hot_next(hot, state);
            assert!(hot);
        }
        assert!(!thermal_hot_next(hot, Nominal));
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
