//! 1:1 call state + actions.

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::settings::CallPrefs;
use quill::state::{ActiveCall, Session};
use quill::telegram::envelope::{
    CallDiscardReason, CallState, ChatKind, MessageContent, ParsedMessage, call_entry_label,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;

/// Phase C2i: the peer (user id + display name) of a call entry,
/// when the call message is in a 1:1 chat. Group calls have no single
/// peer — `None` hides "Call again".
pub(super) fn call_message_peer(
    session: Option<&Session>,
    chat_id: ChatId,
) -> Option<(i64, String)> {
    let session = session?;
    let chat = session.chats.get(&chat_id.0)?;
    let user_id = match chat.kind {
        ChatKind::Private { user_id } | ChatKind::Secret { user_id, .. } => user_id.0,
        _ => return None,
    };
    let name = session
        .user(user_id)
        .map(|user| user.display_name())
        .unwrap_or_else(|| format!("User {user_id}"));
    Some((user_id, name))
}

impl QuillApp {
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
    pub(super) fn start_call_for_user(
        &mut self,
        user_id: i64,
        is_video: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .live
            .as_ref()
            .is_some_and(|live| !live.driver.has_call_engine())
        {
            self.connection.status_note =
                "Calls are unavailable. The audio component could not start.".into();
            cx.notify();
            return;
        }
        // Refuse offline before the confirm dialog — a call can't be
        // queued, so confirming then failing would be dishonest.
        if self
            .live
            .as_ref()
            .is_some_and(|live| live.driver.session.is_offline())
        {
            self.connection.status_note = "You're offline — can't start a call".into();
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
    pub(super) fn dial_user(&mut self, user_id: i64, is_video: bool, cx: &mut Context<Self>) {
        if self.live.is_some() {
            if self
                .live
                .as_ref()
                .expect("live")
                .driver
                .session
                .is_offline()
            {
                self.connection.status_note = "You're offline — can't start a call".into();
                cx.notify();
                return;
            }
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .start_call(user_id, is_video);
            self.connection.status_note = match result {
                Ok(_) => {
                    if is_video {
                        "starting video call…".into()
                    } else {
                        "calling…".into()
                    }
                }
                Err(_) => "could not start the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: call start (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: the user confirmed the pending call in the
    /// confirm-before-calling dialog.
    pub(super) fn confirm_pending_call(&mut self, cx: &mut Context<Self>) {
        if let Some((user_id, is_video)) = self.calls.confirm.take() {
            self.dial_user(user_id, is_video, cx);
        } else {
            cx.notify();
        }
    }

    /// Phase C2i: the user cancelled the pending call in the
    /// confirm-before-calling dialog.
    pub(super) fn cancel_pending_call(&mut self, cx: &mut Context<Self>) {
        self.calls.confirm = None;
        cx.notify();
    }

    /// Phase C2i: update one call pref in the session and persist it
    /// to the account dir (via `CallDriver::save_call_prefs`).
    pub(super) fn set_call_pref(
        &mut self,
        update: impl FnOnce(&mut CallPrefs),
        cx: &mut Context<Self>,
    ) {
        let mut prefs = self
            .session()
            .map(|session| session.calls.prefs.clone())
            .unwrap_or_default();
        update(&mut prefs);
        if let Some(live) = self.live.as_mut() {
            live.driver.session.calls.prefs = prefs;
            if let Err(err) = live.driver.save_call_prefs() {
                self.connection.status_note = format!("couldn’t save call settings: {err}");
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.calls.prefs = prefs;
            self.connection.status_note = "demo: call settings are not saved".into();
        }
        self.sync_ptt_with_call(cx);
        cx.notify();
    }

    pub(super) fn toggle_call_mute(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let muted = live
                .driver
                .session
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| !call.muted);
            self.connection.status_note = match live.driver.set_call_muted(muted) {
                Ok(()) => {
                    if muted {
                        "microphone muted".into()
                    } else {
                        "microphone unmuted".into()
                    }
                }
                Err(err) => format!("could not change mute state: {err}"),
            };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
        {
            call.muted = !call.muted;
        }
        cx.notify();
    }

    /// Phase C2e: camera on/off toggle for a video call. Live: flips the
    /// camera intent and pushes it to the engine; an engine error
    /// surfaces in the status note without flipping the flag (driver
    /// contract). Demo: flips the flag only, no live Telegram.
    pub(super) fn toggle_call_camera(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.calls.active_call.as_ref() else {
                return;
            };
            let (call_id, camera_on) = (call.id, !call.camera_on);
            let ready = live.driver.call_video_ready();
            self.connection.status_note =
                match live.driver.set_call_camera(call_id, camera_on && ready) {
                    Ok(()) => {
                        if camera_on {
                            "camera on".into()
                        } else {
                            "camera off".into()
                        }
                    }
                    Err(err) => format!("could not change camera state: {err}"),
                };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
        {
            call.camera_on = !call.camera_on;
            self.connection.status_note = "demo: camera toggle (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: 1:1 screen-share send toggle for a video call.
    /// Live: flips the intent and pushes it to the engine; an engine
    /// error surfaces in the status note without flipping the flag
    /// (driver contract). Demo: flips the flag only, no live
    /// Telegram.
    pub(super) fn toggle_call_screen_share(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.calls.active_call.as_ref() else {
                return;
            };
            let (call_id, sharing) = (call.id, !call.screen_sharing);
            self.connection.status_note = match live.driver.set_call_screen_share(call_id, sharing)
            {
                Ok(()) => {
                    if sharing {
                        "screen share on".into()
                    } else {
                        "screen share off".into()
                    }
                }
                Err(err) => format!("could not change screen share state: {err}"),
            };
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
        {
            call.screen_sharing = !call.screen_sharing;
            self.connection.status_note = "demo: screen share toggle (no live Telegram)".into();
        }
        cx.notify();
    }

    pub(super) fn select_call_device(
        &mut self,
        kind: quill::calls::engine::MediaDeviceKind,
        device_id: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            // Phase C2e: camera selection goes to the driver, which
            // stores it and re-applies the camera on the active call.
            if kind == quill::calls::engine::MediaDeviceKind::Camera {
                self.connection.status_note =
                    match live.driver.select_call_camera(Some(device_id.to_string())) {
                        Ok(()) => "camera selected".into(),
                        Err(err) => format!("could not select camera: {err}"),
                    };
                cx.notify();
                return;
            }
            let (microphone, speaker) = live.driver.selected_call_devices();
            let (microphone, speaker) = match kind {
                quill::calls::engine::MediaDeviceKind::Microphone => {
                    (Some(device_id.to_string()), speaker.map(str::to_owned))
                }
                quill::calls::engine::MediaDeviceKind::Speaker => {
                    (microphone.map(str::to_owned), Some(device_id.to_string()))
                }
                _ => return,
            };
            self.connection.status_note = match live.driver.select_call_devices(microphone, speaker)
            {
                Ok(()) => "audio device selected".into(),
                Err(err) => format!("could not select audio device: {err}"),
            };
        } else {
            match kind {
                quill::calls::engine::MediaDeviceKind::Microphone => {
                    self.demo_ui.selected_devices.0 = Some(device_id.into())
                }
                quill::calls::engine::MediaDeviceKind::Speaker => {
                    self.demo_ui.selected_devices.1 = Some(device_id.into())
                }
                quill::calls::engine::MediaDeviceKind::Camera => {
                    self.demo_ui.selected_camera = Some(device_id.into())
                }
                _ => return,
            }
        }
        cx.notify();
    }

    /// Phase C1: `acceptCall` for the ringing incoming call.
    pub(super) fn accept_incoming_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.accept_call();
            self.connection.status_note = match result {
                Ok(_) => "answering…".into(),
                Err(_) => "could not answer the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: call accept (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call, or hang up an active one).
    pub(super) fn hang_up_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.discard_call();
            self.connection.status_note = match result {
                Ok(_) => "hanging up…".into(),
                Err(_) => "could not hang up the call".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: call hang up (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: star tap opens the rating-detail editor instead of
    /// sending immediately — the user picks problems + an optional
    /// comment, then submits via `submit_call_rating`.
    pub(super) fn open_rating_detail(
        &mut self,
        rating: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.rating_detail = Some(RatingDetail {
            stars: rating,
            problems: [false; 9],
        });
        self.dialogs.rating_comment_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn toggle_rating_problem(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.dialogs.rating_detail.as_mut() {
            detail.problems[index] = !detail.problems[index];
        }
        cx.notify();
    }

    /// Phase C2i: `sendCallRating` with problems + comment (schema
    /// 1.8.67 :14234). Sends only from the detail editor's Submit
    /// button — `open_rating_detail` never sends on its own.
    pub(super) fn submit_call_rating(&mut self, cx: &mut Context<Self>) {
        let detail = match self.dialogs.rating_detail.take() {
            Some(detail) => detail,
            None => {
                cx.notify();
                return;
            }
        };
        let comment = self
            .dialogs
            .rating_comment_input
            .read(cx)
            .value()
            .to_string();
        let problems: Vec<&str> = CALL_PROBLEMS
            .iter()
            .enumerate()
            .filter(|(index, _)| detail.problems[*index])
            .map(|(_, (constructor, _))| *constructor)
            .collect();
        let stars = detail.stars;
        if self.live.is_some() {
            let result = self
                .live
                .as_mut()
                .expect("live")
                .driver
                .send_call_rating(stars, &comment, &problems);
            self.connection.status_note = match result {
                Ok(_) => "thanks for your feedback".into(),
                Err(_) => "could not send the rating".into(),
            };
        } else if self.demo_session.is_some() {
            self.connection.status_note = "demo: call rating (no live Telegram)".into();
            if let Some(session) = self.demo_session.as_mut()
                && let Some(summary) = session.calls.summary.as_mut()
            {
                summary.rating_sent = true;
            }
        }
        cx.notify();
    }

    pub(super) fn upload_call_diagnostics(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.send_call_debug_information() {
                Ok(_) => "diagnostics upload sent".into(),
                Err(_) => "could not upload diagnostics".into(),
            };
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.summary.as_mut())
        {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
            self.connection.status_note = "demo: diagnostics upload (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C2i: `sendCallLog` from the call-end card — uploads the
    /// ended call's log file (schema 1.8.67 :14240).
    pub(super) fn upload_call_log(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.status_note = match live.driver.send_call_log() {
                Ok(_) => "call log upload sent".into(),
                Err(_) => "could not upload the call log".into(),
            };
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.summary.as_mut())
        {
            summary.log_sent = true;
            summary.log_error = None;
            self.connection.status_note = "demo: call log upload (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Phase C1: dismiss the call-end screen (rating skipped or
    /// acknowledged).
    pub(super) fn dismiss_call_summary(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.calls.summary = None;
        }
        cx.notify();
    }

    /// Phase C1: dismiss a shown call-request error.
    pub(super) fn dismiss_call_error(&mut self, cx: &mut Context<Self>) {
        for session in self
            .live
            .as_mut()
            .map(|live| &mut live.driver.session)
            .into_iter()
            .chain(self.demo_session.as_mut())
        {
            session.calls.error = None;
        }
        cx.notify();
    }

    /// Phase C1: 1s tick while a call is tracked, keeping the overlay's
    /// ringing / connected clock fresh. Mirrors the Phase A1 slow-mode
    /// tick (at most one task; exits when no call is active).
    pub(super) fn ensure_call_tick(&mut self, cx: &mut Context<Self>) {
        let call_active = self
            .session()
            .is_some_and(|s| s.calls.active_call.is_some());
        if !call_active || self.calls.tick_active {
            return;
        }
        self.calls.tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                // Phase C2e: video calls tick at 100ms so incoming
                // frames reach the tiles; everything else stays at 1s.
                let interval = this
                    .update(cx, |this, _| this.call_tick_interval())
                    .unwrap_or(Duration::from_secs(1));
                cx.background_executor().timer(interval).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_active = this
                            .session()
                            .is_some_and(|s| s.calls.active_call.is_some());
                        if still_active {
                            cx.notify();
                            true
                        } else {
                            this.calls.tick_active = false;
                            false
                        }
                    })
                    .unwrap_or(false);
                if !cont {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| {
                this.calls.tick_active = false;
            });
        })
        .detach();
    }

    /// Phase C2e: tick interval for the call overlay — 100ms while a
    /// video call is Ready and a video feed is live (peer streaming or
    /// local camera on), 1s otherwise.
    pub(super) fn call_tick_interval(&self) -> Duration {
        let fast = self
            .session()
            .and_then(|s| s.calls.active_call.as_ref())
            .is_some_and(|call| {
                call.is_video
                    && matches!(call.state, CallState::Ready)
                    && (call.remote_video != quill::calls::engine::RemoteVideoState::Inactive
                        || self.call_camera_effective(call))
            });
        if fast {
            Duration::from_millis(100)
        } else {
            Duration::from_secs(1)
        }
    }

    /// Phase C2e: the local camera actually contributes a feed — the
    /// intent is on and video can run (live: driver ready; demo:
    /// fixtures carry the frames).
    pub(super) fn call_camera_effective(&self, call: &ActiveCall) -> bool {
        call.camera_on
            && (self.live.is_none()
                || self
                    .live
                    .as_ref()
                    .is_some_and(|live| live.driver.call_video_ready()))
    }

    /// kit Phase 2 (redo): call confirm hosted in a kit `Dialog` via
    /// `window.open_dialog`. Esc / backdrop / ✕ clear state via `on_close`.
    pub(super) fn build_call_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallConfirm, |this, _, cx| {
                this.cancel_pending_call(cx);
            });
        app.update(cx, |this, cx| {
            let Some((user_id, is_video)) = this.calls.confirm else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Confirm call"))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let kind = if is_video { "video call" } else { "call" };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("You asked to confirm before calling."),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-confirm-cancel")
                        .label("Cancel")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("call-confirm-ok")
                        .label(if is_video { "Video call" } else { "Call" })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(format!(
                    "Start {kind} with {name}?"
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Swap prompt: answer the pending incoming call — the driver
    /// ends the current call first (TDLib allows a single active call)
    /// and accepts the incoming one once the discard lands.
    pub(super) fn answer_swap_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.accept_swap_call();
            self.connection.status_note = match result {
                Ok(_) => "ending current call, answering…".into(),
                Err(_) => "could not answer the call".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            if let Some((call_id, user_id, is_video)) = session.calls.swap_pending.take() {
                session.calls.active_call = None;
                session.calls.summary = None;
                session.calls.active_call = Some(ActiveCall {
                    id: call_id,
                    user_id,
                    is_outgoing: false,
                    is_video,
                    state: CallState::Pending {
                        is_created: false,
                        is_received: true,
                    },
                    started_at: Instant::now(),
                    ready_at: None,
                    ready: None,
                    transport: None,
                    transport_error: None,
                    signaling_queue: Vec::new(),
                    signaling_dropped: 0,
                    muted: false,
                    camera_on: is_video,
                    screen_sharing: false,
                    remote_video: quill::calls::engine::RemoteVideoState::Inactive,
                    remote_screen: quill::calls::engine::RemoteVideoState::Inactive,
                    remote_audio_muted: false,
                });
            }
            self.connection.status_note = "demo: swap accepted (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Swap prompt: decline the pending incoming call as busy.
    pub(super) fn decline_swap_call(&mut self, cx: &mut Context<Self>) {
        if self.live.is_some() {
            let result = self.live.as_mut().expect("live").driver.decline_swap_call();
            self.connection.status_note = match result {
                Ok(_) => "incoming call declined".into(),
                Err(_) => "could not decline the call".into(),
            };
        } else if let Some(session) = self.demo_session.as_mut() {
            session.calls.swap_pending = None;
            self.connection.status_note = "demo: swap declined (no live Telegram)".into();
        }
        cx.notify();
    }

    /// Swap prompt hosted in a kit `Dialog` via `window.open_dialog`
    /// (the kit Phase 2 redo pattern). Esc / backdrop / ✕ declines the
    /// incoming call as busy — the same honest outcome the old
    /// auto-decline produced.
    pub(super) fn build_call_swap_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallSwap, |this, _, cx| {
                this.decline_swap_call(cx);
            });
        app.update(cx, |this, cx| {
            let Some((_call_id, user_id, is_video)) =
                this.session().and_then(|s| s.calls.swap_pending)
            else {
                return dialog
                    .overlay(true)
                    .title(crate::ui::shell::dialog_title("Incoming call"))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| format!("User {user_id}"));
            let kind = if is_video { "video call" } else { "call" };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("You're already in a call. Telegram doesn't support putting a call on hold — answering ends the current one."),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-swap-decline")
                        .label("Decline")
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.decline_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                )
                .child(
                    Button::new("call-swap-answer")
                        .label("End & answer")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.answer_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(format!("{name} is calling ({kind})")))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body.into_any_element())));
                    move |content, _, _| {
                        let body = body
                            .borrow_mut()
                            .take()
                            .unwrap_or_else(|| div().into_any_element());
                        content.child(body)
                    }
                }))
                .footer(footer)
                .on_close(on_close)
        })
    }

    /// Phase C2i: the peer (user id + display name) of a call entry,
    /// when the call message is in a 1:1 chat. Group calls have no
    /// single peer — `None` hides "Call again".
    pub(super) fn recent_call_peer(&self, chat_id: ChatId) -> Option<(i64, String)> {
        call_message_peer(self.session(), chat_id)
    }

    /// Phase C2i: busy-decline banner for `call_busy_declined` — the
    /// incoming calls declined while another call was active.
    pub(super) fn call_busy_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let declined = self.session()?.calls.busy_declined.clone();
        if declined.is_empty() {
            return None;
        }
        let names: Vec<String> = declined
            .iter()
            .map(|(user_id, is_video)| {
                let name = self
                    .session()
                    .and_then(|session| session.user(*user_id))
                    .map(|user| user.display_name())
                    .unwrap_or_else(|| format!("User {user_id}"));
                format!("{name} ({})", if *is_video { "video" } else { "voice" })
            })
            .collect();
        Some(
            div()
                .id("call-busy-banner")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().accent.opacity(0.12))
                .child(div().text_sm().flex_1().child(format!(
                    "Missed call{} from {} — declined because another call was active.",
                    if declined.len() == 1 { "" } else { "s" },
                    names.join(", ")
                )))
                .child(
                    Button::new("call-busy-dismiss")
                        .label("Dismiss")
                        .ghost()
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(live) = this.live.as_mut() {
                                live.driver.session.calls.busy_declined.clear();
                            } else if let Some(session) = this.demo_session.as_mut() {
                                session.calls.busy_declined.clear();
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}

crate::ui::shell::register_dialogs! {
    CallConfirm => DialogSpec::new(
        1100,
        |app| app.calls.confirm.is_some(),
        QuillApp::build_call_confirm_dialog,
    ),

    /// Swap prompt: incoming call while another call is active.
    CallSwap => DialogSpec::new(
        // Swap prompt is call-urgent: same priority band as CallConfirm.
        1200,
        |app| app.session().is_some_and(|s| s.calls.swap_pending.is_some()),
        QuillApp::build_call_swap_dialog,
    ),
}

mod call_again;
