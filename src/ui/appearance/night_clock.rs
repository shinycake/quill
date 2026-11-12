//! The wake for scheduled auto-night: one timer to the next time the
//! schedule changes day to night or back, held by its stage. Nothing runs
//! while the schedule is off, and a locked, hidden or logged-out app wakes
//! at most twice a day. The OS appearance and accent are not polled: the
//! window's appearance and activation observers re-apply them
//! (`app_demo.rs`).

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::settings::{AutoNight, local_minutes_since_midnight, secs_until_night_change};
use std::time::Duration;

/// The timer of the schedule, owned by the preferences. Replacing or
/// dropping the stage cancels the wait.
#[derive(Default)]
pub(in crate::ui) enum NightClock {
    #[default]
    Idle,
    /// Waiting for the next boundary; the wake applies the appearance,
    /// which arms the next one.
    Waiting { _task: Task<()> },
}

impl QuillApp {
    /// Arm the wake for the next boundary of the schedule, or stop waiting
    /// when auto-night is not scheduled. Called from every appearance pass.
    pub(in crate::ui) fn sync_night_clock(&mut self, cx: &mut Context<Self>) {
        let appearance = &self.appearance;
        let wake = (appearance.auto_night == AutoNight::Scheduled)
            .then(|| {
                let now = quill::local_time::now_unix();
                secs_until_night_change(
                    appearance.night_start_minutes,
                    appearance.night_end_minutes,
                    local_minutes_since_midnight(),
                    u16::try_from(now.rem_euclid(60)).unwrap_or(0),
                )
            })
            .flatten();
        self.preferences.night_clock = match wake {
            None => NightClock::Idle,
            Some(secs) => {
                // screen-scope: the schedule is an app-wide setting; it applies to whichever account is on screen.
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs(secs))
                        .await;
                    let _ = this.update(cx, |this, cx| this.apply_appearance(cx));
                });
                NightClock::Waiting { _task: task }
            }
        };
    }
}
