//! Starting a call: the confirm-before-calling pref, then microphone
//! (and camera) permission, then `createCall`.
use super::super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::connect::ConnectSendError;
use quill::lang::{Key, keys, tr};

/// The note for a call join that did not go out: without the call library
/// it is the same "calls are unavailable" a 1:1 call shows, else
/// `fallback`.
pub(in crate::ui) fn join_error_note(err: ConnectSendError, fallback: Key) -> StatusNote {
    match err {
        ConnectSendError::CallsUnavailable => {
            StatusNote::error(tr(keys::Q_OPS_CALLS_ARE_UNAVAILABLE_THE_AUDIO))
        }
        _ => StatusNote::error(tr(fallback)),
    }
}

impl QuillApp {
    /// With a live connection but no call engine (libquill_calls did not
    /// load), says calls are unavailable and returns true: nothing would
    /// play or send, so no call of any kind starts or is joined.
    pub(in crate::ui) fn refuse_without_call_engine(&mut self, cx: &mut Context<Self>) -> bool {
        if !self
            .live
            .as_ref()
            .is_some_and(|live| !live.driver.has_call_engine())
        {
            return false;
        }
        self.connection.set_note(StatusNote::error(tr(
            keys::Q_OPS_CALLS_ARE_UNAVAILABLE_THE_AUDIO,
        )));
        cx.notify();
        true
    }

    /// Phase C1b: `createCall` from a user profile. `is_video: true`
    /// starts video-call *signaling* — media transport is still Phase
    /// C2, so the call carries no audio or video and the UI says so.
    /// The outgoing call is tracked once the `callId` answer arrives
    /// (with `is_video` derived from the request args); its states
    /// arrive as `updateCall`.
    /// Phase C2i: preference-aware entry point for starting a call
    /// from a profile / user panel / history row. When the
    /// confirm-before-calling pref is on, the call waits for the user
    /// to confirm in the dialog — the actual `startCall` goes through
    /// `dial_user`.
    pub(in crate::ui) fn start_call_for_user(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_without_call_engine(cx) {
            return;
        }
        // Refuse offline before the confirm dialog — a call can't be
        // queued, so confirming then failing would be dishonest.
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline())
        {
            self.connection.set_note(StatusNote::error(tr(
                keys::Q_OPS_YOURE_OFFLINE_CANT_START_A,
            )));
            cx.notify();
            return;
        }
        if self
            .session()
            .is_some_and(|session| session.calls.prefs.confirm_before_calling)
        {
            self.calls.confirm = Some((user_id, is_video));
            cx.notify();
            return;
        }
        self.dial_user(user_id, is_video, cx);
    }

    /// Phase C2i: the actual `startCall` send, after any confirmation.
    /// Slice parity:platform-offline-errors — a call can't be queued
    /// like a message, so refuse while offline with an honest note.
    pub(in crate::ui) fn dial_user(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        // tdesktop asks for the microphone (and the camera for video)
        // before calling, so a refusal never becomes a silent call.
        if self.live.is_some() {
            self.calls.dialing = Some((user_id, is_video));
            self.with_capture_access(is_video, Self::dial_allowed, cx);
            return;
        }
        self.send_dial(user_id, is_video, cx);
    }

    fn dial_allowed(&mut self, cx: &mut Context<Self>) {
        if let Some((user_id, is_video)) = self.calls.dialing.take() {
            self.send_dial(user_id, is_video, cx);
        }
    }

    fn send_dial(&mut self, user_id: i64, is_video: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            if live.driver.session.is_offline() {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_OPS_YOURE_OFFLINE_CANT_START_A,
                )));
                cx.notify();
                return;
            }
            let result = live.driver.start_call(user_id, is_video);
            self.connection.set_note(match result {
                Ok(_) => {
                    if is_video {
                        StatusNote::silent(tr(keys::Q_OPS_STARTING_VIDEO_CALL))
                    } else {
                        StatusNote::silent(tr(keys::Q_OPS_CALLING_2))
                    }
                }
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_START_THE_CALL)),
            });
        } else if self.demo_session.is_some() {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_DEMO_CALL_START_NO_LIVE)));
        }
        cx.notify();
    }
}
