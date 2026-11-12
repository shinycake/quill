//! Keeps a running live location's "expires in" line current. Telegram
//! Desktop re-arms a per-message timer for the same job
//! (`Location::updateLiveStatus`); Quill has no per-row timers, so one task
//! per open chat redraws the window when the soonest label would change.
//! That is once a minute for most of a share and once a second in its last
//! minute, and the task ends with the last running share.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use std::time::{Duration, Instant};

/// The wait before the next redraw: the label's own pace, but never a
/// busy loop.
fn wait_for(refresh_in: u32) -> Duration {
    Duration::from_secs(u64::from(refresh_in.max(1)))
}

impl QuillApp {
    /// Start the refresh task for the open chat when it shows a running
    /// live location (at most one task, guarded by
    /// `live_location_tick`). Called from `render`, like the other
    /// countdown ticks.
    pub(super) fn ensure_live_location_tick(&mut self, cx: &mut Context<Self>) {
        let now = quill::local_time::now_unix();
        let Some(chat_id) = self.session().and_then(|session| session.open_chat_id()) else {
            return;
        };
        // A running tick keeps itself alive: no need to scan the history
        // again every frame.
        if self
            .history
            .live_location_tick
            .as_ref()
            .map(|(chat, _)| *chat)
            == Some(chat_id)
        {
            return;
        }
        let Some(refresh_in) = self
            .session()
            .and_then(|session| session.open_chat_live_location_refresh(now))
        else {
            return;
        };
        self.arm_live_location_wake(chat_id, Instant::now() + wait_for(refresh_in), cx);
    }

    /// Wake once at `due`: redraw, and arm the next wake while a share
    /// still runs. The stage holds the one task, so closing the chat or the
    /// window ends it, and each wake replaces the task that ran it.
    fn arm_live_location_wake(&mut self, chat_id: ChatId, due: Instant, cx: &mut Context<Self>) {
        let task = self.spawn_scoped(cx, async move |this, cx| {
            let wait = due.saturating_duration_since(Instant::now());
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                match this.live_location_due(chat_id) {
                    Some(next) => {
                        // A window behind another app catches up when it
                        // is activated again.
                        if this.frame.window_active.get() {
                            cx.notify();
                        }
                        this.arm_live_location_wake(chat_id, next, cx);
                    }
                    None => {
                        if this
                            .history
                            .live_location_tick
                            .as_ref()
                            .map(|(chat, _)| *chat)
                            == Some(chat_id)
                        {
                            this.history.live_location_tick = None;
                        }
                        // The last share ended: draw its "ended" state.
                        cx.notify();
                    }
                }
            });
        });
        self.history.live_location_tick = Some((chat_id, task));
    }

    /// When `chat_id`'s next refresh is due, or `None` when it is no longer
    /// the open chat or shows no running live location.
    fn live_location_due(&self, chat_id: ChatId) -> Option<Instant> {
        let session = self.session()?;
        if session.open_chat_id() != Some(chat_id) {
            return None;
        }
        session
            .open_chat_live_location_refresh(quill::local_time::now_unix())
            .map(|refresh_in| Instant::now() + wait_for(refresh_in))
    }
}

#[cfg(test)]
mod tests {
    use super::wait_for;
    use std::time::Duration;

    #[test]
    fn waits_are_whole_seconds_and_never_zero() {
        assert_eq!(wait_for(0), Duration::from_secs(1));
        assert_eq!(wait_for(1), Duration::from_secs(1));
        assert_eq!(wait_for(60), Duration::from_secs(60));
    }
}
