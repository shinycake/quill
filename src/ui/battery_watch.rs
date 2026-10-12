//! Watches the system battery saver (tdesktop `base::BatterySaving`) and
//! forces every power-saving flag on while it is on.
//!
//! tdesktop is notified by the platform; here a task asks every few seconds.
//! The question can block (Linux runs `gdbus`), so it runs on the background
//! executor. Only a change touches the UI: the flags and GPUI's reduced
//! motion are updated and the windows redraw.

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
