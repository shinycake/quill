//! Joining the stream a live stream window shows: the Join button, the join
//! you asked for once its call is tracked, and the wait on the join's own
//! stage (`GroupTransport`) until it lands, fails or times out.

use super::app::QuillApp;
use super::group_call_state::{JoinSent, StreamJoining};
use super::group_call_stream_layout::{StreamJoinIntent, uses_stream_layout};
use gpui_kit::*;
use quill::lang::{keys, tr};
use std::time::Duration;

/// A join that neither lands nor reports an error gives up after this.
const JOIN_TIMEOUT: Duration = Duration::from_secs(20);

impl QuillApp {
    /// Join the tracked stream from its window: the Join button, or the
    /// join you asked for.
    pub(super) fn join_stream(&mut self, id: i32, cx: &mut Context<Self>) {
        self.dismiss_group_call_error(cx);
        if let Some(sent) = self.send_group_call_join() {
            let timeout = self.spawn_screen(cx, async move |this, cx| {
                cx.background_executor().timer(JOIN_TIMEOUT).await;
                let _ = this.update(cx, |app, cx| app.stream_join_ended(id, cx));
            });
            self.group_call_screen
                .start_join(StreamJoining::new(id, sent, timeout));
        } else {
            self.stream_join_failed();
        }
        cx.notify();
    }

    /// The join was not sent or did not land: stop waiting and say so.
    pub(super) fn stream_join_failed(&mut self) {
        self.group_call_screen.clear_stream_joining();
        let reason = tr(keys::Q_OPS_STREAM_JOIN_FAILED).to_string();
        if let Some(live) = self.live.as_mut() {
            live.driver.session.calls.group_call_error = Some(reason);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.calls.group_call_error = Some(reason);
        }
    }

    /// The join on its way landed or failed: stop waiting, and say so when
    /// it failed. Run with the window sync, outside the draw.
    pub(super) fn settle_stream_join(&mut self) {
        let Some(joining) = self.group_call_screen.joining() else {
            return;
        };
        let (joining_call, sent) = (joining.call, joining.sent);
        let Some(facts) = self.group_call_stream_facts() else {
            return;
        };
        if joining_call != facts.id {
            self.stream_join_failed();
        } else if facts.joined
            || self.session().is_some_and(|session| {
                // The transport now waits on another join's request (a
                // rejoin took its place): this join's request is no longer
                // the one in flight, so it ends with no failure of its own.
                matches!(sent, JoinSent::Request(mine) if session
                    .calls
                    .active_group_call
                    .as_ref()
                    .and_then(|call| call.transport.join_request())
                    .is_some_and(|waits| waits != mine))
            })
        {
            self.group_call_screen.clear_stream_joining();
        } else if self.session().is_some_and(|session| {
            // The call's transport failed: a join clears it, so while a
            // join waits it is that join's failure (a failed screen share
            // is kept apart, `screen_share_error`).
            session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.id == facts.id && call.transport.error().is_some())
        }) {
            // The connect failed without a join error.
            self.stream_join_failed();
        } else if self.session().is_some_and(|session| {
            // The join's own stage is the transport's: it left `Joining`
            // with no answer and no connect (the request's error, or it
            // never went out). An error of anything else in the call is not
            // this join's end.
            session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.id == facts.id && call.transport.is_empty())
        }) {
            // The join's refusal is already noted; say so only when not.
            if self
                .session()
                .is_some_and(|s| s.calls.group_call_error.is_some())
            {
                self.group_call_screen.clear_stream_joining();
            } else {
                self.stream_join_failed();
            }
        }
    }

    /// The join of `id` timed out, or the window saw it end: unless it
    /// landed, the Join button comes back with the failure.
    pub(super) fn stream_join_ended(&mut self, id: i32, cx: &mut Context<Self>) {
        if self.group_call_screen.joining_call() != Some(id) {
            return;
        }
        let joined = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
            .is_some_and(|call| call.id == id && call.is_joined);
        if joined {
            self.group_call_screen.clear_stream_joining();
        } else {
            self.stream_join_failed();
        }
        cx.notify();
    }

    /// Run the join you asked for once its call is tracked. Any other
    /// unjoined stream waits for its Join button.
    pub(super) fn consume_stream_join_intent(&mut self, cx: &mut Context<Self>) {
        let Some((intent, tracked_then)) = self.group_call_screen.stream_wish() else {
            return;
        };
        // The fetch of the call it asked for failed: no call will come for
        // this wish, whatever other call is tracked meanwhile.
        if let StreamJoinIntent::Call(id) = intent {
            let failed = if let Some(live) = self.live.as_mut() {
                live.driver.session.calls.take_group_call_fetch_failure(id)
            } else {
                self.demo_session
                    .as_mut()
                    .is_some_and(|session| session.calls.take_group_call_fetch_failure(id))
            };
            if failed {
                self.group_call_screen.clear_stream_wish();
                return;
            }
        }
        let Some(call) = self
            .session()
            .and_then(|s| s.calls.active_group_call.as_ref())
        else {
            // The request that fetches the call failed: no call will come
            // for this wish, so it must not join one much later.
            if self
                .session()
                .is_some_and(|s| s.calls.group_call_error.is_some())
            {
                self.group_call_screen.clear_stream_wish();
            }
            return;
        };
        let chat = self.group_call_chat(call).map(|chat| chat.id.0);
        if !intent.matches(call.id, chat) {
            // Another call took the place of the one tracked when you
            // asked: the wish is stale and must not join much later.
            if tracked_then != Some(call.id) {
                self.group_call_screen.clear_stream_wish();
            }
            return;
        }
        let (id, stream, joined) = (call.id, uses_stream_layout(call), call.is_joined);
        self.group_call_screen.clear_stream_wish();
        if joined {
            return;
        }
        match intent {
            StreamJoinIntent::Call(_) if stream => self.join_stream(id, cx),
            StreamJoinIntent::Call(_) => {}
            StreamJoinIntent::Created { join_as, .. } => {
                if join_as.is_some() {
                    if let Some(live) = self.live.as_mut() {
                        live.driver.session.set_group_call_join_as(join_as);
                    } else if let Some(session) = self.demo_session.as_mut() {
                        session.set_group_call_join_as(join_as);
                    }
                }
                if stream {
                    self.join_stream(id, cx);
                } else {
                    self.send_group_call_join();
                    cx.notify();
                }
            }
        }
    }
}
