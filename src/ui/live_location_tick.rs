//! Keeps a running live location's "expires in" line current. Telegram
//! Desktop re-arms a per-message timer for the same job
//! (`Location::updateLiveStatus`); Quill has no per-row timers, so one task
//! per open chat redraws the window when the soonest label would change.
//! That is once a minute for most of a share and once a second in its last
//! minute, and the task ends with the last running share.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use std::time::Duration;

/// The wait before the next redraw: the label's own pace, but never a
/// busy loop.
fn wait_for(refresh_in: u32) -> Duration {
    Duration::from_secs(u64::from(refresh_in.max(1)))
}

impl QuillApp {
    /// Start the refresh task for the open chat when it shows a running
    /// live location (at most one task, guarded by
    /// `live_location_tick_chat`). Called from `render`, like the other
    /// countdown ticks.
    pub(super) fn ensure_live_location_tick(&mut self, cx: &mut Context<Self>) {
        let now = quill::local_time::now_unix();
        let Some((chat_id, _)) = self.session().and_then(|session| {
            Some((
                session.open_chat?,
                session.open_chat_live_location_refresh(now)?,
            ))
        }) else {
            return;
        };
        if self.live_location_tick_chat == Some(chat_id) {
            return;
        }
        self.live_location_tick_chat = Some(chat_id);
        cx.spawn(async move |this, cx| {
            loop {
                let wait = this
                    .update(cx, |this, _| this.live_location_wait(chat_id))
                    .ok()
                    .flatten();
                let Some(wait) = wait else {
                    break;
                };
                cx.background_executor().timer(wait).await;
                let keep = this
                    .update(cx, |this, cx| {
                        if this.live_location_wait(chat_id).is_none() {
                            return false;
                        }
                        // A window behind another app catches up when it
                        // is activated again.
                        if this.window_active.get() {
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.live_location_tick_chat == Some(chat_id) {
                    this.live_location_tick_chat = None;
                }
                // The last share ended: draw its "ended" state.
                cx.notify();
            });
        })
        .detach();
    }

    /// How long to wait before `chat_id`'s next refresh, or `None` when
    /// it is no longer the open chat or shows no running live location.
    fn live_location_wait(&self, chat_id: ChatId) -> Option<Duration> {
        let session = self.session()?;
        if session.open_chat != Some(chat_id) {
            return None;
        }
        session
            .open_chat_live_location_refresh(quill::local_time::now_unix())
            .map(wait_for)
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
