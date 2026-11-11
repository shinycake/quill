//! Watches the system battery saver (tdesktop `base::BatterySaving`) and
//! forces every power-saving flag on while it is on.
//!
//! tdesktop is notified by the platform, and so is this watcher: macOS and
//! Windows through GPUI's power mode. GPUI's Linux backend reads only the
//! profile daemon, which a sandboxed app cannot reach, so Linux follows the
//! portal or the daemon itself (`quill::battery_signal`). Without either, a
//! task asks every half minute while the switch is on, and not at all while
//! it is off (the first probe still runs so the switch is offered when the
//! platform has a saver). While an agent drives the window it also asks
//! every half minute, because the takeover glow's sweep stops in the
//! system's low-power mode even with the switch off
//! (`agent_takeover_glow::sweep_allowed`). The question can block (it runs
//! `gdbus`), so it runs on the background executor.
//!
//! Only a change touches the UI: the flags and GPUI's reduced motion are
//! updated and the windows redraw.
//!
//! Thermal pressure (macOS `NSProcessInfo.thermalState`; Linux and Windows
//! always report Nominal) feeds the same force path. It is event-driven
//! through `App::on_thermal_state_change`, with no polling.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
#[cfg(target_os = "linux")]
use std::time::Duration;

/// Between probes while "Save Power on Low Battery" is on. Each probe may
/// spawn a process (Linux), and a battery saver is switched by hand, so half
/// a minute is prompt enough.
#[cfg(target_os = "linux")]
const POLL: Duration = Duration::from_secs(30);
/// Between checks while the switch is off: nothing depends on the answer,
/// so the task only waits to be poked when the switch comes back on.
#[cfg(target_os = "linux")]
const PARKED: Duration = Duration::from_secs(600);

/// Start the watcher; later calls do nothing.
pub(crate) fn watch(cx: &mut App) {
    static STARTED: std::sync::Once = std::sync::Once::new();
    let mut first = false;
    STARTED.call_once(|| first = true);
    if !first {
        return;
    }
    watch_thermal(cx);
    watch_saver(cx);
}

/// Follow the saver through GPUI: read it once, then on each change.
#[cfg(not(target_os = "linux"))]
fn watch_saver(cx: &mut App) {
    apply_power_mode(cx);
    cx.on_power_mode_change(apply_power_mode).detach();
}

#[cfg(not(target_os = "linux"))]
fn apply_power_mode(cx: &mut App) {
    // Unknown is the normal state on macOS: it does not tell Automatic from
    // High Power, only whether Low Power Mode is on.
    apply_answer(Some(cx.power_mode() == PowerMode::LowPower), cx);
}

#[cfg(target_os = "linux")]
fn watch_saver(cx: &mut App) {
    quill::battery_saving::start_events();
    cx.spawn(async move |cx| {
        loop {
            let seen = quill::wake::BATTERY.epoch();
            let answer = cx
                .background_executor()
                .spawn(async { quill::battery_saving::probe() })
                .await;
            cx.update(|cx| apply_answer(answer, cx));
            // The agent takeover's sweep follows the system's low-power
            // mode whatever the switch says, so it is asked while an agent
            // drives (a new session pokes the wake as well).
            let takeover_shown = quill::agent::ui_driver::takeover::current()
                != quill::agent::ui_driver::takeover::Phase::Idle;
            // Hidden or behind another app, nothing draws, so nothing needs
            // the answer: activation pokes the wake and the probe runs then.
            let in_front = cx.update(|cx| cx.active_window().is_some());
            let wait = probe_wait(
                quill::battery_saving::event_driven(),
                quill::battery_saving::ignored(),
                takeover_shown,
                in_front,
            );
            super::wake_wait::sleep_or_wake(
                cx.background_executor(),
                &quill::wake::BATTERY,
                seen,
                wait,
            )
            .await;
        }
    })
    .detach();
}

/// How long the probe loop rests before asking again.
#[cfg(target_os = "linux")]
fn probe_wait(event_driven: bool, ignored: bool, takeover_shown: bool, in_front: bool) -> Duration {
    if event_driven || (ignored && !takeover_shown) || !in_front {
        PARKED
    } else {
        POLL
    }
}

/// Follow the system thermal state: read it once, then on each change.
fn watch_thermal(cx: &mut App) {
    apply_thermal(cx);
    cx.on_thermal_state_change(apply_thermal).detach();
}

fn apply_thermal(cx: &mut App) {
    let thermal = match cx.thermal_state() {
        ThermalState::Nominal => quill::battery_saving::Thermal::Nominal,
        ThermalState::Fair => quill::battery_saving::Thermal::Fair,
        ThermalState::Serious => quill::battery_saving::Thermal::Serious,
        ThermalState::Critical => quill::battery_saving::Thermal::Critical,
    };
    let before = quill::battery_saving::forcing();
    quill::battery_saving::set_thermal(thermal);
    quill::battery_saving::apply();
    if before != quill::battery_saving::forcing() {
        cx.set_reduce_motion(quill::power_saving::reduce_motion_now());
        cx.refresh_windows();
    }
}

/// Take the platform's answer in.
fn apply_answer(answer: Option<bool>, cx: &mut App) {
    let before = (
        quill::battery_saving::state(),
        quill::battery_saving::forcing(),
    );
    quill::battery_saving::set_state(answer);
    quill::battery_saving::apply();
    let after = (
        quill::battery_saving::state(),
        quill::battery_saving::forcing(),
    );
    if before != after {
        cx.set_reduce_motion(quill::power_saving::reduce_motion_now());
        cx.refresh_windows();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{PARKED, POLL, probe_wait};

    #[test]
    fn the_probe_polls_only_while_something_is_shown_and_asks() {
        assert_eq!(probe_wait(false, false, false, true), POLL);
        assert_eq!(probe_wait(false, false, false, false), PARKED, "hidden");
        assert_eq!(probe_wait(true, false, false, true), PARKED, "signalled");
        assert_eq!(probe_wait(false, true, false, true), PARKED, "switch off");
        assert_eq!(probe_wait(false, true, true, true), POLL, "agent drives");
        assert_eq!(probe_wait(false, true, true, false), PARKED, "agent hidden");
    }
}
