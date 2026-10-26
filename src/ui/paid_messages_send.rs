//! Writing to someone who charges (tdesktop `ShowSendPaidConfirm`,
//! `PaysStatus`, `EmptyChatLockedBox`): the line over the composer with the
//! price, the confirmation before the first paid send with "Don't ask me
//! again", and the Remove Fee bar and box for a user who pays the account.
//!
//! Nothing here sends a paid message by itself. A send carries a price only
//! after the confirmation (or a trusted price); without one TDLib refuses
//! the message and no Stars move.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{danger, text_muted, warning};
use super::paid_messages_state::{ComposerPaidConfirm, RemoveFeeBox};
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::paid_messages::{
    PaidConfirm, SendGate, needs_confirmation, pays_bar_text, refund_label, remove_fee_text,
    stars_text,
};
use quill::state::Session;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    fn paid_session_mut(&mut self) -> Option<&mut Session> {
        match self.live.as_mut() {
            Some(live) => Some(&mut live.driver.session),
            None => self.demo_session.as_deref_mut(),
        }
    }

    /// Before a send in `chat_id`: opens the confirmation when it costs
    /// Stars the user has not agreed to yet. True when the send waits.
    pub(super) fn paid_send_waits(
        &mut self,
        chat_id: ChatId,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session() else {
            return false;
        };
        let stars = session.send_gate(chat_id).stars();
        if !needs_confirmation(&session.payments.paid_messages.trust, chat_id.0, stars) {
            return false;
        }
        let name = session.chats.get(&chat_id.0).map_or_else(
            || tr(keys::Q_MONEY_THIS_USER).to_string(),
            |chat| chat.title.clone(),
        );
        let Some(slot) = self.send_slot() else {
            return false;
        };
        let messages = self.pending_attachments().len().max(1) as i64;
        self.paid_messages.confirm = Some(ComposerPaidConfirm {
            ask: PaidConfirm {
                chat_id: chat_id.0,
                name,
                per_message: stars,
                messages,
                text: text.to_string(),
            },
            slot,
        });
        self.paid_messages.confirm_trust = false;
        if let Some(live) = self.live.as_mut() {
            // The balance for the Pay check.
            let _ = live.driver.maybe_fetch_star_transactions();
        }
        cx.notify();
        true
    }

    /// The confirmation on screen: the one opened for a composer send, or
    /// a send the guard holds (a sticker, GIF, voice note, forward...).
    /// The flag is true for a held send.
    pub(super) fn paid_confirm_view(&self) -> Option<(PaidConfirm, bool)> {
        if let Some(pending) = self.paid_messages.confirm.as_ref() {
            return Some((pending.ask.clone(), false));
        }
        let held = self.live.as_ref()?.driver.held_paid_send()?;
        let name = self.session()?.chats.get(&held.chat_id).map_or_else(
            || tr(keys::Q_MONEY_THIS_CHAT_2).to_string(),
            |chat| chat.title.clone(),
        );
        Some((
            PaidConfirm {
                chat_id: held.chat_id,
                name,
                per_message: held.per_message,
                messages: held.messages,
                text: String::new(),
            },
            true,
        ))
    }

    /// Cancel: a composer send keeps its text; a held send is dropped.
    fn cancel_paid_confirm(&mut self, cx: &mut Context<Self>) {
        if self.paid_messages.confirm.take().is_none()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.discard_paid_send();
        }
        cx.notify();
    }

    /// Pay: remember the price when asked to, then send. A composer send
    /// goes out again and is released from the guard; a held send is
    /// released as it is.
    pub(super) fn confirm_paid_send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let remember = self.paid_messages.confirm_trust;
        let Some(ComposerPaidConfirm { ask: confirm, slot }) = self.paid_messages.confirm.take()
        else {
            if let Some(live) = self.live.as_mut()
                && live.driver.held_paid_send().is_some()
                && live.driver.release_paid_send(remember).is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULDNT_SEND_THE_PAID_MESSAGE,
                )));
            }
            cx.notify();
            return;
        };
        // Another chat or account is on screen now: the composer would send
        // there, at a price nobody agreed to. Nothing goes, as on Cancel;
        // the text waits in its own chat's composer.
        if self.send_slot().as_ref() != Some(&slot) {
            cx.notify();
            return;
        }
        let chat = ChatId(confirm.chat_id);
        if remember {
            if let Some(live) = self.live.as_mut() {
                live.driver.remember_paid_trust(chat, confirm.per_message);
            } else if let Some(session) = self.paid_session_mut() {
                session
                    .payments
                    .paid_messages
                    .trust
                    .trust(chat.0, confirm.per_message);
            }
        }
        let total = confirm.total();
        self.submit_composer(confirm.text, window, cx);
        // Not trusted: what the send left held is released up to the total
        // the user just agreed to, never more.
        let mut budget = total;
        while let Some(live) = self.live.as_mut()
            && let Some(next) = live.driver.held_paid_send()
            && next.chat_id == chat.0
            && next.total() <= budget
        {
            budget -= next.total();
            let _ = live.driver.release_paid_send(false);
        }
        cx.notify();
    }

    /// The line over the composer: what the other side charges, or that
    /// they pay the account with the Remove Fee button.
    pub(super) fn paid_messages_notice(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let chat_id = session.open_chat?;
        let title = session.chats.get(&chat_id.0)?.title.clone();
        if let SendGate::Charges { stars } = session.send_gate(chat_id) {
            let line = SendGate::Charges { stars }.sentence(&title)?;
            return Some(
                div()
                    .id("paid-messages-charges")
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .text_sm()
                    .text_color(text_muted())
                    .child(div().text_color(warning()).child("\u{2B50}"))
                    .child(line)
                    .into_any_element(),
            );
        }
        let pays = session.user_pays_me(chat_id);
        let user_id = session.private_chat_user_id(chat_id)?;
        if pays == 0 {
            return None;
        }
        let error = session.payments.paid_messages.error.clone();
        Some(
            div()
                .id("paid-messages-pays-me")
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(text_muted())
                        .child(pays_bar_text(&title, pays)),
                )
                .child(
                    Button::new("paid-messages-remove-fee")
                        .label(tr(keys::MESSAGES_PRIVACY_REMOVE_FEE))
                        .ghost()
                        .small()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_remove_fee(user_id, cx);
                        })),
                )
                .when_some(error, |this, line| {
                    this.child(div().text_xs().text_color(danger()).child(line))
                })
                .into_any_element(),
        )
    }

    /// Opens the Remove Fee box and asks what the user paid so far.
    pub(super) fn open_remove_fee(&mut self, user_id: i64, cx: &mut Context<Self>) {
        let name = self.session().and_then(|s| s.user(user_id)).map_or_else(
            || tr(keys::Q_MONEY_THIS_USER_2).to_string(),
            |u| u.display_name(),
        );
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.fetch_paid_message_revenue(user_id);
        }
        if let Some(session) = self.paid_session_mut() {
            session.payments.paid_messages.error = None;
        }
        self.paid_messages.remove_fee = Some(RemoveFeeBox {
            user_id,
            name,
            refund: false,
        });
        cx.notify();
    }

    fn close_remove_fee(&mut self, cx: &mut Context<Self>) {
        self.paid_messages.remove_fee = None;
        cx.notify();
    }

    pub(super) fn confirm_remove_fee(&mut self, cx: &mut Context<Self>) {
        let Some(boxed) = self.paid_messages.remove_fee.take() else {
            return;
        };
        let paid = self
            .session()
            .and_then(|s| {
                s.payments
                    .paid_messages
                    .revenue
                    .get(&boxed.user_id)
                    .copied()
            })
            .unwrap_or(0);
        let refund = boxed.refund && paid > 0;
        if let Some(live) = self.live.as_mut() {
            if live
                .driver
                .allow_unpaid_messages(boxed.user_id, refund)
                .is_err()
            {
                self.connection.set_note(StatusNote::error(tr(
                    keys::Q_MONEY_COULDNT_REMOVE_THE_FEE_TRY,
                )));
            }
        } else if let Some(demo) = self.demo_session.as_mut() {
            demo.finish_unpaid_exception(boxed.user_id);
        }
        cx.notify();
    }

    pub(super) fn build_paid_confirm_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PaidSendConfirm, |this, _, cx| {
                this.cancel_paid_confirm(cx);
            });
        app.update(cx, |this, cx| {
            let Some((confirm, _)) = this.paid_confirm_view() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let balance = this
                .session()
                .and_then(|s| s.payments.hub.balance)
                .map(|b| b.stars);
            let short = balance.filter(|stars| *stars < confirm.total());
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(confirm.body()));
            if let Some(stars) = short {
                body = body.child(div().text_xs().text_color(warning()).child(tr_with(
                    keys::Q_MONEY_YOUR_BALANCE_IS_BUY_MORE,
                    &[("stars_text", &(stars_text(stars)).to_string())],
                )));
            }
            let body = body
                .child(
                    Checkbox::new("paid-send-trust")
                        .label(tr(keys::LAUNCH_EXE_DONT_ASK))
                        .checked(this.paid_messages.confirm_trust)
                        .on_click(cx.listener(|this, &on: &bool, _, cx| {
                            this.paid_messages.confirm_trust = on;
                            cx.notify();
                        })),
                )
                .into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("paid-send-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.cancel_paid_confirm(cx);
                            this.close_kit_dialog_if_done(DialogKind::PaidSendConfirm, window, cx);
                        })),
                )
                .child(
                    Button::new("paid-send-pay")
                        .label(confirm.button())
                        .primary()
                        .disabled(short.is_some())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_paid_send(window, cx);
                            this.close_kit_dialog_if_done(DialogKind::PaidSendConfirm, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(confirm.title()))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
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

    pub(super) fn build_remove_fee_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::RemoveFee, |this, _, cx| {
                this.close_remove_fee(cx);
            });
        app.update(cx, |this, cx| {
            let Some(boxed) = this.paid_messages.remove_fee.as_ref() else {
                return dialog.overlay(true).on_close(on_close);
            };
            let paid = this
                .session()
                .and_then(|s| {
                    s.payments
                        .paid_messages
                        .revenue
                        .get(&boxed.user_id)
                        .copied()
                })
                .unwrap_or(0);
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_sm().child(remove_fee_text(&boxed.name)));
            if let Some(label) = refund_label(paid) {
                body = body.child(
                    Checkbox::new("remove-fee-refund")
                        .label(label)
                        .checked(boxed.refund)
                        .on_click(cx.listener(|this, &on: &bool, _, cx| {
                            if let Some(boxed) = this.paid_messages.remove_fee.as_mut() {
                                boxed.refund = on;
                            }
                            cx.notify();
                        })),
                );
            }
            let body = body.into_any_element();
            let footer = div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("remove-fee-cancel")
                        .label(tr(keys::CANCEL))
                        .ghost()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_remove_fee(cx);
                            this.close_kit_dialog_if_done(DialogKind::RemoveFee, window, cx);
                        })),
                )
                .child(
                    Button::new("remove-fee-confirm")
                        .label(tr(keys::Q_MONEY_CONFIRM))
                        .primary()
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.confirm_remove_fee(cx);
                            this.close_kit_dialog_if_done(DialogKind::RemoveFee, window, cx);
                        })),
                );
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::MESSAGES_PRIVACY_REMOVE_FEE,
                )))
                .content(crate::ui::shell::scrollable_dialog_content({
                    let body = Rc::new(RefCell::new(Some(body)));
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
}

crate::ui::shell::register_dialogs! {
    /// Confirm payment before a message that costs Stars.
    PaidSendConfirm => DialogSpec::new(
        3855,
        |app| app.paid_confirm_view().is_some(),
        QuillApp::build_paid_confirm_dialog,
    )
    .closes(|app, _, cx| app.cancel_paid_confirm(cx)),
    /// Allow a user to write for free, optionally refunding what they paid.
    RemoveFee => DialogSpec::new(
        3857,
        |app| app.paid_messages.remove_fee.is_some(),
        QuillApp::build_remove_fee_dialog,
    )
    .closes(|app, _, cx| app.close_remove_fee(cx)),
}
