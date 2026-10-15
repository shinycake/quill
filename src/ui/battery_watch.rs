//! Watches the system battery saver (tdesktop `base::BatterySaving`) and
//! forces every power-saving flag on while it is on.
//!
//! tdesktop is notified by the platform; here a task asks every few seconds.
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

const POLL: Duration = Duration::from_secs(5);

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
            let answer = cx
                .background_executor()
                .spawn(async { quill::battery_saving::probe() })
                .await;
            let changed = cx.update(|cx| apply_answer(answer, cx));
            let _ = changed;
            cx.background_executor().timer(POLL).await;
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
