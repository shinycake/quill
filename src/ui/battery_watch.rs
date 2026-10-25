//! Watches the system battery saver (tdesktop `base::BatterySaving`) and
//! forces every power-saving flag on while it is on.
//!
//! tdesktop is notified by the platform; here a task asks every half minute
//! while the switch is on, and not at all while it is off (the first probe
//! still runs so the switch is offered when the platform has a saver).
//! While an agent drives the window it also asks every half minute, because
//! the takeover glow's sweep stops in the system's low-power mode even with
//! the switch off (`agent_takeover_glow::sweep_allowed`).
//! The question can block (Linux runs `gdbus`), so it runs on the background
//! executor. Only a change touches the UI: the flags and GPUI's reduced
//! motion are updated and the windows redraw.
//!
//! Thermal pressure (macOS `NSProcessInfo.thermalState`; Linux and Windows
//! always report Nominal) feeds the same force path. It is event-driven
//! through `App::on_thermal_state_change`, with no polling.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use std::time::Duration;

/// Between probes while "Save Power on Low Battery" is on. Each probe may
/// spawn a process (Linux), and a battery saver is switched by hand, so half
/// a minute is prompt enough.
const POLL: Duration = Duration::from_secs(30);
/// Between checks while the switch is off: nothing depends on the answer,
/// so the task only waits to be poked when the switch comes back on.
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
    cx.spawn(async move |cx| {
        loop {
            let seen = quill::wake::BATTERY.epoch();
            let answer = cx
                .background_executor()
                .spawn(async { quill::battery_saving::probe() })
                .await;
            let changed = cx.update(|cx| apply_answer(answer, cx));
            let _ = changed;
            // The agent takeover's sweep follows the system's low-power
            // mode whatever the switch says, so it is asked while an agent
            // drives (a new session pokes the wake as well).
            let takeover_shown = quill::agent::ui_driver::takeover::current()
                != quill::agent::ui_driver::takeover::Phase::Idle;
            let wait = if quill::battery_saving::ignored() && !takeover_shown {
                PARKED
            } else {
                POLL
            };
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

/// Take a probe answer in. Returns whether anything visible changed.
fn apply_answer(answer: Option<bool>, cx: &mut App) -> bool {
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
    if before == after {
        return false;
    }
    cx.set_reduce_motion(quill::power_saving::reduce_motion_now());
    cx.refresh_windows();
    true
}
