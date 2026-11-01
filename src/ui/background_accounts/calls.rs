//! Incoming calls on an account that is not on screen. Telegram Desktop's
//! `Calls::Instance` takes calls from every account: the call rings with
//! the incoming-call prompt, the ringtone and the OS notification, and a
//! call that comes in while another is in progress is declined as busy.
//! Here the prompt names the account; answering switches to it (its
//! client comes forward with the call still ringing, the call engine
//! loaded on demand when the call came in) and answers there.

use super::*;
use crate::ui::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use quill::lang::{keys, tr, tr_with};
use quill::notify_accounts::{BackgroundCallPlan, plan_background_call, ringing_incoming_call};
use quill::notify_call::{CallNotificationAction, CallNotifyInput, decide_call_notify};

/// A call ringing on a background account.
pub(super) struct BackgroundRing {
    pub(super) account: AccountKey,
    pub(super) call_id: i32,
    pub(super) caller: String,
    pub(super) is_video: bool,
    /// The account's own name, for the prompt.
    pub(super) account_name: String,
    /// Closed with Esc: no prompt and no ringtone (the OS notification and
    /// the caller's side keep ringing until the call ends).
    pub(super) dismissed: bool,
    /// The OS notification went out for this call.
    pub(super) notified: bool,
}

impl QuillApp {
    /// The account whose call rings in the background, if one does.
    pub(in crate::ui) fn background_ring_account(&self) -> Option<&AccountKey> {
        self.background.ringing.as_ref().map(|ring| &ring.account)
    }

    /// Whether the background ring should sound in the app.
    pub(in crate::ui) fn background_ring_audible(&self) -> bool {
        self.background
            .ringing
            .as_ref()
            .is_some_and(|ring| !ring.dismissed)
    }

    /// After a background poll: start ringing for a new incoming call,
    /// decline one as busy when a call is already going, and stop ringing
    /// once the call stopped (answered elsewhere, missed, hung up).
    pub(super) fn sync_background_calls(&mut self, cx: &mut Context<Self>) {
        if let Some(ring) = &self.background.ringing {
            let still = self
                .background
                .client(&ring.account)
                .and_then(|client| client.live.as_ref())
                .and_then(|live| ringing_incoming_call(&live.driver.session))
                .is_some_and(|(call_id, _, _)| call_id == ring.call_id);
            if !still {
                self.end_background_ring(cx);
            }
        }
        let on_screen_in_call = self.session().is_some_and(|session| {
            session.calls.active_call.is_some() || session.calls.active_group_call.is_some()
        });
        let ringing = self
            .background
            .ringing
            .as_ref()
            .map(|ring| (ring.account.clone(), ring.call_id));
        let mut new_ring = None;
        for client in &mut self.background.clients {
            let Some(live) = client.live.as_mut() else {
                continue;
            };
            let Some((call_id, user_id, is_video)) = ringing_incoming_call(&live.driver.session)
            else {
                continue;
            };
            if ringing.as_ref() == Some(&(client.key.clone(), call_id)) {
                continue;
            }
            let another = ringing.is_some() || new_ring.is_some();
            match plan_background_call(on_screen_in_call, another) {
                BackgroundCallPlan::DeclineBusy => {
                    let _ = live.driver.discard_call();
                }
                BackgroundCallPlan::Ring => {
                    // Answering needs the engine to know the call already.
                    live.attach_call_engine(&DiscardSink);
                    let session = &live.driver.session;
                    let caller = session
                        .user(user_id)
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| {
                            tr_with(keys::Q_OPS_USER, &[("user_id", &user_id.to_string())])
                        });
                    let account_name = session
                        .my_user_id
                        .and_then(|me| session.user(me))
                        .map(|user| user.display_name())
                        .unwrap_or_else(|| client.key.to_string());
                    new_ring = Some(BackgroundRing {
                        account: client.key.clone(),
                        call_id,
                        caller,
                        is_video,
                        account_name,
                        dismissed: false,
                        notified: false,
                    });
                }
            }
        }
        if new_ring.is_some() {
            self.background.ringing = new_ring;
            // The ringing prompt in the main window.
            cx.notify();
        }
        self.notify_background_ring(cx);
        // Starts, keeps looping or stops the ringtone.
        self.sync_call_sounds();
    }

    /// The OS notification for the ringing call, once, when no Quill window
    /// is in front (the prompt shows otherwise).
    fn notify_background_ring(&mut self, cx: &mut Context<Self>) {
        let app_active = cx.active_window().is_some();
        let locked = self.account.passcode.locked;
        let Some(ring) = self.background.ringing.as_mut() else {
            return;
        };
        if ring.notified {
            return;
        }
        let Some(notification) = decide_call_notify(&CallNotifyInput {
            call_id: ring.call_id,
            caller: &ring.caller,
            is_video: ring.is_video,
            app_active,
            last_notified: None,
            locked,
        }) else {
            return;
        };
        ring.notified = true;
        let account = ring.account.clone();
        self.spawn_call_notification_as(&account, notification, cx);
    }

    /// The call stopped ringing: no prompt, no ringtone, no toast.
    fn end_background_ring(&mut self, cx: &mut Context<Self>) {
        let Some(ring) = self.background.ringing.take() else {
            return;
        };
        if ring.notified && quill::notify::current_backend() == quill::notify::NotifyBackend::Native
        {
            cx.dismiss_system_notification(&quill::notify_call::call_notification_tag(
                &ring.account.0,
                ring.call_id,
            ));
        }
        cx.notify();
    }

    /// Answer: switch to the account, then answer there.
    pub(in crate::ui) fn answer_background_call(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Agents never answer a call for the user; the ring stays as it was.
        if quill::agent::ui_driver::guard::refuse_if_injecting("answer calls for the user") {
            return;
        }
        let Some(account) = self.background_ring_account().cloned() else {
            return;
        };
        self.background.ringing = None;
        self.switch_account(&account, "", window, cx);
        let ready = self.session().is_some_and(|session| {
            session.account == account && ringing_incoming_call(session).is_some()
        });
        if ready {
            self.accept_incoming_call(cx);
        } else {
            self.connection
                .set_note(StatusNote::silent(tr(keys::Q_OPS_THE_CALL_ENDED_BEFORE_IT)));
        }
        cx.notify();
    }

    /// Decline on the background account; nothing switches.
    pub(super) fn decline_background_call(&mut self, cx: &mut Context<Self>) {
        let Some(ring) = self.background.ringing.as_ref() else {
            return;
        };
        let account = ring.account.clone();
        if let Some(live) = self
            .background
            .client_mut(&account)
            .and_then(|client| client.live.as_mut())
        {
            let _ = live.driver.discard_call();
        }
        self.end_background_ring(cx);
    }

    /// Picks on the ringing call's OS notification (they arrive in
    /// `calls.notify_clicks` with the other call picks). Runs at render,
    /// before the on-screen account's call picks.
    pub(super) fn flush_background_call_picks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(call_id) = self.background.ringing.as_ref().map(|ring| ring.call_id) else {
            return;
        };
        let picks: Vec<CallNotificationAction> = match self.calls.notify_clicks.lock() {
            Ok(mut guard) => {
                let (mine, rest): (Vec<_>, Vec<_>) =
                    guard.drain(..).partition(|(id, _)| *id == call_id);
                *guard = rest;
                mine.into_iter().map(|(_, action)| action).collect()
            }
            Err(_) => return,
        };
        for action in picks {
            match action {
                CallNotificationAction::Accept => self.answer_background_call(window, cx),
                CallNotificationAction::Decline => self.decline_background_call(cx),
                CallNotificationAction::Open => {
                    if let Some(ring) = self.background.ringing.as_mut() {
                        ring.dismissed = false;
                    }
                    cx.activate(true);
                    window.activate_window();
                }
            }
        }
    }

    /// The incoming-call prompt for a background account.
    fn build_background_call_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::BackgroundCall, |this, _, cx| {
                if let Some(ring) = this.background.ringing.as_mut() {
                    ring.dismissed = true;
                }
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let (title, body) = this
                .background
                .ringing
                .as_ref()
                .map(|ring| {
                    let kind = if ring.is_video {
                        tr(keys::Q_OPS_INCOMING_VIDEO_CALL)
                    } else {
                        tr(keys::Q_OPS_INCOMING_VOICE_CALL)
                    };
                    (
                        kind.to_string(),
                        tr_with(
                            keys::Q_OPS_IS_CALLING_2,
                            &[
                                ("caller", &ring.caller.to_string()),
                                ("account_name", &ring.account_name.to_string()),
                            ],
                        ),
                    )
                })
                .unwrap_or_default();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("background-call-decline")
                        .label(tr(keys::CALL_DECLINE))
                        .danger()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.decline_background_call(cx);
                            this.close_kit_dialog_if_done(DialogKind::BackgroundCall, window, cx);
                        })),
                )
                .child(
                    Button::new("background-call-answer")
                        .label(tr(keys::Q_OPS_ANSWER))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.answer_background_call(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::BackgroundCall, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(title))
                .child(div().text_sm().child(body))
                .footer(footer)
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// An incoming call on an account that is not on screen
    /// (`parity:notify-all-accounts` follow-up).
    BackgroundCall => DialogSpec::new(
        // A ringing call comes right after what the server says about the
        // account, ahead of every settings dialog.
        150,
        |app| app.background_ring_audible() && !app.account.passcode.locked,
        QuillApp::build_background_call_dialog,
    )
    .closes(|app, _, cx| { if let Some(ring) = app.background.ringing.as_mut() { ring.dismissed = true; } cx.notify(); }),
}
