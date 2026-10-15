//! UI half of the voice chat, video chat and live stream links
//! (`parity:deeplink-voice-chat`, see `quill::call_link`).
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::link_info_boxes::finish;
use super::shell::{DialogKind, QuillShell};
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::*;
use quill::call_link::{BAD_LINK_NOTE, CallLinkStep, step_note};
use quill::ids::ChatId;
use quill::lang::{keys, tr};

impl QuillApp {
    /// The link's chat is open: offer its call. The call screen's Join
    /// button joins, with the link's hash.
    pub(super) fn offer_call_link(
        &mut self,
        chat_id: ChatId,
        invite_hash: &str,
        live_stream: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(live) = self.live.as_mut() else {
            self.connection.status_note = tr(keys::Q_OPS_CALLS_NEED_A_SIGNED_IN).into();
            cx.notify();
            return;
        };
        match live
            .driver
            .offer_call_link(chat_id, invite_hash, live_stream)
        {
            Ok(step) => {
                self.connection.status_note = match step_note(step, live_stream) {
                    Some(note) => note,
                    None if matches!(step, CallLinkStep::Offer { .. }) => {
                        if live_stream {
                            tr(keys::Q_OPS_OPENING_THE_LIVE_STREAM).into()
                        } else {
                            tr(keys::Q_OPS_OPENING_THE_VOICE_CHAT).into()
                        }
                    }
                    None => self.connection.status_note.clone(),
                };
            }
            Err(_) => self.connection.status_note = BAD_LINK_NOTE.into(),
        }
        cx.notify();
    }

    fn cancel_call_link_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            live.driver.cancel_call_link_leave();
        }
        cx.notify();
    }

    fn confirm_call_link_leave(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut()
            && live.driver.confirm_call_link_leave().is_err()
        {
            self.connection.status_note = tr(keys::Q_OPS_COULDNT_LEAVE_THE_CALL).into();
        }
        cx.notify();
    }

    /// tdesktop `confirmLeaveCurrent`: leave the current call to join the
    /// link's.
    pub(super) fn build_call_link_leave_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::CallLinkLeave, |this, _, cx| {
                this.cancel_call_link_leave(cx);
            });
        app.update(cx, |this, cx| {
            let Some(ask) = this
                .session()
                .and_then(|session| session.calls.call_link_confirm.clone())
            else {
                return dialog.overlay(true).on_close(on_close.clone());
            };
            let body = div()
                .id("call-link-leave-text")
                .text_sm()
                .child(ask.text())
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("call-link-leave-cancel")
                        .label(tr(keys::CALL_CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_call_link_leave(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallLinkLeave, window, cx);
                        })),
                )
                .child(
                    Button::new("call-link-leave-confirm")
                        .label(ask.button())
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_call_link_leave(cx);
                            this.close_kit_dialog_if_done(DialogKind::CallLinkLeave, window, cx);
                        })),
                )
                .into_any_element();
            finish(dialog, tr(keys::VIEW_BUTTON_CALL), body, footer, on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// A call link while another call is on: leave it and join?
    CallLinkLeave => DialogSpec::new(
        2960,
        |app| app.session().is_some_and(|s| s.calls.call_link_confirm.is_some()),
        QuillApp::build_call_link_leave_dialog,
    ),
}
