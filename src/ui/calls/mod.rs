//! 1:1 call state + actions.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use super::*;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::calls::engine::EngineError;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with, trn_with};
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
        .unwrap_or_else(|| tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())]));
    Some((user_id, name))
}

impl QuillApp {
    /// The camera and microphone picked in the call menu, read from the
    /// driver that owns the pick (demo mode has no device names: defaults).
    pub(crate) fn capture_choice(&self) -> quill::capture_choice::CaptureChoice {
        self.live
            .as_ref()
            .map(|live| live.driver.capture_choice())
            .unwrap_or_default()
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
            live.driver.save_call_prefs();
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.calls.prefs = prefs;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_CALL_SETTINGS_ARE_NOT,
            )));
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
            self.connection
                .set_note(match live.driver.set_call_muted(muted) {
                    Ok(()) => {
                        if muted {
                            StatusNote::silent(tr(keys::Q_OPS_MICROPHONE_MUTED_2))
                        } else {
                            StatusNote::silent(tr(keys::Q_OPS_MICROPHONE_UNMUTED))
                        }
                    }
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_OPS_COULD_NOT_CHANGE_MUTE_STATE,
                        &[("err", &err.to_string())],
                    )),
                });
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
        // Turning the camera on asks macOS (or checks Windows privacy)
        // first, so a refusal is named instead of sending a black camera.
        let turning_on = self.live.as_ref().is_some_and(|live| {
            live.driver
                .session
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| !call.camera_on)
        });
        if turning_on {
            self.with_camera_access(Self::flip_call_camera, cx);
        } else {
            self.flip_call_camera(cx);
        }
    }

    fn flip_call_camera(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.calls.active_call.as_ref() else {
                return;
            };
            let (call_id, want_on) = (call.id, !call.camera_on);
            let result = live.driver.set_call_camera(call_id, want_on);
            let camera_on = live
                .driver
                .session
                .calls
                .active_call
                .as_ref()
                .is_some_and(|call| call.camera_on);
            self.connection
                .set_note(camera_toggle_note(result, camera_on));
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
        {
            call.camera_on = !call.camera_on;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_CAMERA_TOGGLE_NO_LIVE,
            )));
        }
        cx.notify();
    }

    /// Phase C2i: 1:1 screen-share send toggle for a video call.
    /// Live: flips the intent and pushes it to the engine; an engine
    /// error surfaces in the status note without flipping the flag
    /// (driver contract). Demo: flips the flag only, no live
    /// Telegram.
    ///
    /// Where Quill captures the screen itself, the button opens the
    /// chooser instead, sharing or not (tdesktop `calls_panel.cpp`); its
    /// Share Screen and Stop Sharing come back through
    /// [`Self::share_call_screen`].
    pub(super) fn toggle_call_screen_share(&mut self, cx: &mut Context<Self>) {
        let Some((call_id, sharing)) = self
            .session()
            .and_then(|session| session.calls.active_call.as_ref())
            .map(|call| (call.id, call.screen_sharing))
        else {
            return;
        };
        if self.open_screen_chooser(super::screen_share_state::ScreenFor::Private(call_id), cx) {
            return;
        }
        self.share_call_screen(!sharing, cx);
    }

    /// Start (`sharing`) or stop sharing the screen in the 1:1 call.
    pub(super) fn share_call_screen(&mut self, sharing: bool, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let Some(call) = live.driver.session.calls.active_call.as_ref() else {
                return;
            };
            let call_id = call.id;
            self.connection
                .set_note(match live.driver.set_call_screen_share(call_id, sharing) {
                    Ok(()) => {
                        if sharing {
                            StatusNote::silent(tr(keys::Q_OPS_SCREEN_SHARE_ON))
                        } else {
                            StatusNote::silent(tr(keys::Q_OPS_SCREEN_SHARE_OFF))
                        }
                    }
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_OPS_COULD_NOT_CHANGE_SCREEN_SHARE,
                        &[("err", &err.to_string())],
                    )),
                });
        } else if let Some(call) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.active_call.as_mut())
        {
            call.screen_sharing = sharing;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_SCREEN_SHARE_TOGGLE_NO,
            )));
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
                self.connection.set_note(
                    match live.driver.select_call_camera(Some(device_id.to_string())) {
                        Ok(()) => StatusNote::silent(tr(keys::Q_OPS_CAMERA_SELECTED)),
                        Err(err) => StatusNote::error(tr_with(
                            keys::Q_OPS_COULD_NOT_SELECT_CAMERA,
                            &[("err", &err.to_string())],
                        )),
                    },
                );
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
            self.connection
                .set_note(match live.driver.select_call_devices(microphone, speaker) {
                    Ok(()) => StatusNote::silent(tr(keys::Q_OPS_AUDIO_DEVICE_SELECTED)),
                    Err(err) => StatusNote::error(tr_with(
                        keys::Q_OPS_COULD_NOT_SELECT_AUDIO_DEVICE,
                        &[("err", &err.to_string())],
                    )),
                });
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
        // As for dialing: the microphone, and the camera for a video call
        // that will send it, are allowed before answering.
        let camera = self.live.as_ref().and_then(|live| {
            live.driver
                .session
                .calls
                .active_call
                .as_ref()
                .map(|call| call.camera_on)
        });
        match camera {
            Some(camera) => self.with_capture_access(camera, Self::answer_call, cx),
            None => self.answer_call(cx),
        }
    }

    fn answer_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.accept_call();
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_ANSWERING)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_ANSWER_THE_CALL)),
            });
        } else if self.demo_session.is_some() {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_DEMO_CALL_ACCEPT_NO_LIVE)));
        }
        cx.notify();
    }

    /// Phase C1: `discardCall` for the tracked call (decline an
    /// incoming call, or hang up an active one).
    pub(super) fn hang_up_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.discard_call();
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_HANGING_UP_2)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_HANG_UP_THE)),
            });
        } else if self.demo_session.is_some() {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_DEMO_CALL_HANG_UP_NO)));
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
            problems: [false; CALL_PROBLEMS.len()],
        });
        self.dialogs.rating_comment_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });
        cx.notify();
    }

    pub(super) fn toggle_rating_problem(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(detail) = self.dialogs.rating_detail.as_mut()
            && let Some(problem) = detail.problems.get_mut(index)
        {
            *problem = !*problem;
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
            .filter(|(index, _)| detail.problems.get(*index).copied().unwrap_or(false))
            .map(|(_, (constructor, _))| *constructor)
            .collect();
        let stars = detail.stars;
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.send_call_rating(stars, &comment, &problems);
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_THANKS_FOR_YOUR_FEEDBACK_2)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_SEND_THE_RATING)),
            });
        } else if self.demo_session.is_some() {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_DEMO_CALL_RATING_NO_LIVE)));
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
            self.connection
                .set_note(match live.driver.send_call_debug_information() {
                    Ok(_) => StatusNote::silent(tr(keys::Q_OPS_DIAGNOSTICS_UPLOAD_SENT)),
                    Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_UPLOAD_DIAGNOSTICS)),
                });
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.summary.as_mut())
        {
            summary.debug_information_sent = true;
            summary.debug_information_error = None;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_DIAGNOSTICS_UPLOAD_NO_LIVE,
            )));
        }
        cx.notify();
    }

    /// Phase C2i: `sendCallLog` from the call-end card — uploads the
    /// ended call's log file (schema 1.8.67 :14240).
    pub(super) fn upload_call_log(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection.set_note(match live.driver.send_call_log() {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_CALL_LOG_UPLOAD_SENT)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_UPLOAD_THE_CALL)),
            });
        } else if let Some(summary) = self
            .demo_session
            .as_mut()
            .and_then(|session| session.calls.summary.as_mut())
        {
            summary.log_sent = true;
            summary.log_error = None;
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_DEMO_CALL_LOG_UPLOAD_NO)));
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
            .chain(self.demo_session.as_deref_mut())
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
            .chain(self.demo_session.as_deref_mut())
        {
            session.calls.error = None;
        }
        cx.notify();
    }

    /// Redraw what the call's clock shows: the call window's panel and
    /// the call bar over the open chat (in every window), while the rest
    /// of the app keeps its frame.
    pub(super) fn notify_call_clock(&self, cx: &mut Context<Self>) {
        self.notify_conversations(cx);
        if let Some(panel) = self.calls.panel_view.get() {
            let app: &mut App = cx;
            app.notify(panel);
        }
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
                // Frames wake their own pictures (`call_video`): the
                // tick only keeps the clock fresh.
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let cont = this
                    .update(cx, |this, cx| {
                        let still_active = this
                            .session()
                            .is_some_and(|s| s.calls.active_call.is_some());
                        if still_active {
                            this.notify_call_clock(cx);
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
                    .title(crate::ui::shell::dialog_title(tr(keys::Q_OPS_CONFIRM_CALL)))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())]));
            let kind = if is_video {
                tr(keys::Q_OPS_VIDEO_CALL_2)
            } else {
                "call"
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_YOU_ASKED_TO_CONFIRM_BEFORE)),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-confirm-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("call-confirm-ok")
                        .label(if is_video {
                            tr(keys::Q_OPS_VIDEO_CALL_3)
                        } else {
                            tr(keys::Q_OPS_CALL)
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_pending_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr_with(
                    keys::Q_OPS_START_WITH,
                    &[("kind", kind), ("name", &name.to_string())],
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
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.accept_swap_call();
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_ENDING_CURRENT_CALL_ANSWERING)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_ANSWER_THE_CALL)),
            });
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
                    remote_battery_low: false,
                    signal_bars: None,
                });
            }
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_SWAP_ACCEPTED_NO_LIVE,
            )));
        }
        cx.notify();
    }

    /// Swap prompt: decline the pending incoming call as busy.
    pub(super) fn decline_swap_call(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let result = live.driver.decline_swap_call();
            self.connection.set_note(match result {
                Ok(_) => StatusNote::silent(tr(keys::Q_OPS_INCOMING_CALL_DECLINED)),
                Err(_) => StatusNote::error(tr(keys::Q_OPS_COULD_NOT_DECLINE_THE_CALL)),
            });
        } else if let Some(session) = self.demo_session.as_mut() {
            session.calls.swap_pending = None;
            self.connection.set_note(StatusNote::silent(tr(
                keys::Q_OPS_DEMO_SWAP_DECLINED_NO_LIVE,
            )));
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
                    .title(crate::ui::shell::dialog_title(tr(keys::CALL_INCOMING)))
                    .on_close(on_close.clone());
            };
            let name = this
                .session()
                .and_then(|session| session.user(user_id))
                .map(|user| user.display_name())
                .unwrap_or_else(|| tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())]));
            let kind = if is_video {
                tr(keys::Q_OPS_VIDEO_CALL_2)
            } else {
                "call"
            };
            let body = div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(tr(keys::Q_OPS_YOURE_ALREADY_IN_A_CALL)),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-swap-decline")
                        .label(tr(keys::CALL_DECLINE))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.decline_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                )
                .child(
                    Button::new("call-swap-answer")
                        .label(tr(keys::Q_OPS_END_ANSWER))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.answer_swap_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallSwap, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr_with(
                    keys::Q_OPS_IS_CALLING,
                    &[("name", &name.to_string()), ("kind", kind)],
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
                    .unwrap_or_else(|| {
                        tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())])
                    });
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
                .child(div().text_sm().flex_1().child(trn_with(
                    keys::Q_LEFTOVERS_MISSED_CALLS_BUSY,
                    declined.len(),
                    &[("names", &names.join(", "))],
                )))
                .child(
                    Button::new("call-busy-dismiss")
                        .label(tr(keys::Q_OPS_DISMISS))
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

#[cfg(feature = "demo-capture")]
mod add_people_demo;
mod call_again;
pub(super) mod call_video;
#[cfg(feature = "demo-capture")]
pub(super) mod call_video_demo;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod call_video_tests;
#[cfg(test)]
mod camera_note_tests;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod clock_notify_tests;
pub(super) mod demo_feed;
mod dial;
pub(in crate::ui) use dial::join_error_note;
#[cfg(feature = "demo-capture")]
mod history_demo;
pub(super) mod live_pip;
#[cfg(feature = "demo-capture")]
mod live_pip_demo;
mod live_pip_follow;
pub(super) mod stream_title;
pub(super) mod stream_view;
pub(super) mod stream_volume;
#[cfg(test)]
#[cfg(feature = "demo-capture")]
mod stream_volume_tests;

/// The status note after a camera toggle, read from the call's resulting
/// `camera_on` rather than from what the toggle asked for: the driver may
/// keep the camera off (video not ready), so the request is not the state.
pub(super) fn camera_toggle_note(result: Result<(), EngineError>, camera_on: bool) -> StatusNote {
    match result {
        Ok(()) if camera_on => StatusNote::silent(tr(keys::Q_OPS_CAMERA_ON_2)),
        Ok(()) => StatusNote::silent(tr(keys::Q_OPS_CAMERA_OFF_2)),
        Err(EngineError::NoCamera) => StatusNote::error(tr(keys::Q_OPS_NO_CAMERA_FOUND)),
        Err(err) => StatusNote::error(tr_with(
            keys::Q_OPS_COULD_NOT_CHANGE_CAMERA_STATE,
            &[("err", &err.to_string())],
        )),
    }
}
