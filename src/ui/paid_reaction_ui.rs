//! Star (paid) reactions: the quick tap with its toast and Undo, and the
//! box with the amount slider and Top Senders (tdesktop
//! `payments_reaction_process.cpp`, `payments_reaction_box.cpp`,
//! `history_view_paid_reaction_toast.cpp`).
//!
//! A tap on the Star chip adds one Star and shows "Star sent" with a
//! countdown Undo. The Stars are committed when the countdown ends. The
//! box adds the chosen amount the same way, so it gets the same Undo.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::{accent, bg_canvas, bg_subtle, text_muted, warning};
use super::paid_reaction_state::PaidBox;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr, tr_with};
use quill::paid_reaction::{
    DEFAULT_AMOUNT, PaidSender, QUICK_AMOUNT, SLIDER_STEPS, amount_for_step, step_for_amount,
};
use quill::premium_hub::group_digits;
use quill::state::unix_ms_now;
use std::cell::RefCell;
use std::rc::Rc;

impl QuillApp {
    /// Adds `stars` to the message's pending Star reaction. A refusal ends
    /// up in the status line.
    fn add_paid(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        stars: i64,
        sender: Option<PaidSender>,
        cx: &mut Context<Self>,
    ) {
        let now = unix_ms_now();
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .add_paid_reaction(chat_id, message_id, stars, sender, now);
            if let Some(note) = live.driver.session.payments.paid.note.clone() {
                self.connection.set_note(StatusNote::info(note));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            let balance = session.payments.hub.balance.map(|b| b.stars);
            let paid = &mut session.payments.paid;
            match paid.refusal(chat_id.0, message_id.0, stars, balance) {
                None => {
                    paid.add(chat_id.0, message_id.0, stars, sender, now);
                }
                Some(refusal) => self
                    .connection
                    .set_note(StatusNote::error(refusal.message().to_string())),
            }
        }
        cx.notify();
    }

    /// A click on the Star chip: one Star, Undo for a few seconds.
    pub(super) fn tap_paid_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        self.add_paid(chat_id, message_id, QUICK_AMOUNT, None, cx);
    }

    /// The Star cell of the reaction picker, when the message allows Star
    /// reactions (tdesktop opens the amount box from it).
    pub(super) fn paid_picker_cell(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self
            .session()?
            .payments
            .paid
            .offered_for(chat_id.0, message_id.0)
        {
            return None;
        }
        Some(
            div()
                .id("reaction-strip-paid")
                .size(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|style| style.bg(bg_subtle()))
                .role(gpui_kit::Role::Button)
                .aria_label(tr(keys::PAID_REACTION_TITLE))
                .child(div().text_size(px(22.)).child("\u{2B50}"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.message_ui.menu = None;
                    this.open_paid_box(chat_id, message_id, cx);
                }))
                .into_any_element(),
        )
    }

    /// The toast's Undo.
    fn undo_paid_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.undo_paid_reaction(chat_id, message_id);
        } else if let Some(session) = self.demo_session.as_mut() {
            session.payments.paid.undo(chat_id.0, message_id.0);
        }
        cx.notify();
    }

    /// Opens the amount box for a message (the chip's menu, or the Star
    /// cell of the reaction picker).
    pub(super) fn open_paid_box(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session() else {
            return;
        };
        let max = session.payments.paid.max();
        let amount = DEFAULT_AMOUNT.clamp(1, max);
        // Start on how the account already shows on this message.
        let sender = session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .and_then(|message| message.paid_reactors().iter().find(|r| r.is_me))
            .map_or(PaidSender::Regular, |mine| match mine.sender {
                _ if mine.is_anonymous => PaidSender::Anonymous,
                Some(quill::telegram::envelope::MessageSender::Chat { chat_id }) => {
                    PaidSender::Chat(chat_id)
                }
                _ => PaidSender::Regular,
            });
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(SLIDER_STEPS as f32)
                .step(1.)
                .default_value(step_for_amount(amount, max) as f32)
        });
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            let max = this.session().map_or(1, |s| s.payments.paid.max());
            if let Some(boxed) = this.paid_reaction.boxed.as_mut() {
                boxed.amount = amount_for_step(value.end().round() as i64, max);
            }
            cx.notify();
        })
        .detach();
        self.paid_reaction.boxed = Some(PaidBox {
            chat_id,
            message_id,
            amount,
            sender,
            slider,
        });
        if let Some(live) = self.live.as_mut() {
            // The balance for the "Send" check.
            let _ = live.driver.maybe_fetch_star_transactions();
            // Channels the Stars may be sent as.
            let _ = live.driver.maybe_fetch_paid_reaction_senders(chat_id);
        }
        cx.notify();
    }

    fn close_paid_box(&mut self, cx: &mut Context<Self>) {
        self.paid_reaction.boxed = None;
        cx.notify();
    }

    fn send_paid_box(&mut self, cx: &mut Context<Self>) {
        let Some(boxed) = self.paid_reaction.boxed.as_ref() else {
            return;
        };
        let (chat_id, message_id, amount) = (boxed.chat_id, boxed.message_id, boxed.amount);
        let sender = boxed.sender;
        self.paid_reaction.boxed = None;
        self.add_paid(chat_id, message_id, amount, Some(sender), cx);
    }

    /// The "Star sent" toast under the chat's top bars, with Undo and the
    /// seconds left.
    pub(super) fn paid_toast_view(
        &self,
        chat_id: ChatId,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.session()?.payments.paid.toast()?.chat_id != chat_id.0 {
            return None;
        }
        self.paid_toast_view_body(chat_id, cx)
    }

    /// `paid_toast_view` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn paid_toast_view_body(&self, chat_id: ChatId, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.session()?.payments.paid.toast()?.clone();
        if pending.chat_id != chat_id.0 {
            return None;
        }
        let message_id = MessageId(pending.message_id);
        let anonymous = pending.sender == Some(PaidSender::Anonymous);
        let title = match (pending.stars, anonymous) {
            (1, false) => tr(keys::Q_MONEY_STAR_SENT),
            (_, false) => tr(keys::Q_MONEY_STARS_SENT),
            (1, true) => tr(keys::Q_MONEY_STAR_SENT_ANONYMOUSLY),
            (_, true) => tr(keys::Q_MONEY_STARS_SENT_ANONYMOUSLY),
        };
        let body = if pending.stars == 1 {
            tr(keys::Q_MONEY_YOU_REACTED_WITH_1_STAR).to_string()
        } else {
            tr_with(
                keys::Q_MONEY_YOU_REACTED_WITH_STARS,
                &[("stars", &(group_digits(pending.stars)).to_string())],
            )
        };
        let seconds = pending.seconds_left(unix_ms_now());
        Some(
            div()
                .id("paid-reaction-toast")
                .flex()
                .flex_none()
                .items_center()
                .gap_3()
                .px_3()
                .py_1()
                .min_h(px(48.))
                .border_b_1()
                .border_color(cx.theme().border)
                .bg(bg_canvas())
                .child(
                    div()
                        .text_size(px(22.))
                        .text_color(warning())
                        .child("\u{2B50}"),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_sm().font_semibold().child(title))
                        .child(div().text_xs().text_color(text_muted()).child(body)),
                )
                .child(
                    Button::new("paid-reaction-undo")
                        .label(tr_with(
                            keys::Q_MONEY_UNDO,
                            &[("seconds", &seconds.to_string())],
                        ))
                        .ghost()
                        .small()
                        .text_color(accent())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.undo_paid_reaction(chat_id, message_id, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn build_paid_reaction_dialog(
        app: &Entity<QuillApp>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let on_close =
            QuillShell::on_close_kind(app, shell, DialogKind::PaidReaction, |this, _, cx| {
                this.close_paid_box(cx);
            });
        app.update(cx, |this, cx| {
            let dialog = dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(keys::PAID_REACT_TITLE)));
            let (Some(session), Some(boxed)) = (this.session(), this.paid_reaction.boxed.as_ref())
            else {
                return dialog.on_close(on_close);
            };
            let muted = cx.theme().muted_foreground;
            let channel = session.chats.get(&boxed.chat_id.0).map_or_else(
                || tr(keys::Q_MONEY_THIS_CHANNEL).to_string(),
                |chat| chat.title.clone(),
            );
            let message = session
                .histories
                .get(&boxed.chat_id.0)
                .and_then(|history| history.messages.get(&boxed.message_id.0));
            let already = message
                .map(|m| {
                    m.paid_reactors()
                        .iter()
                        .filter(|r| r.is_me)
                        .map(|r| r.star_count)
                        .sum::<i64>()
                })
                .unwrap_or(0);
            let balance = session.payments.hub.balance;
            let amount = boxed.amount;
            let paid = &session.payments.paid;
            let refusal = paid.refusal(
                boxed.chat_id.0,
                boxed.message_id.0,
                amount,
                balance.map(|b| b.stars),
            );
            let mut body = div().flex().flex_col().gap_3();
            body = body.child(div().text_sm().child(tr_with(
                keys::Q_MONEY_CHOOSE_HOW_MANY_STARS_YOU,
                &[("channel", &channel.to_string())],
            )));
            body = body.child(
                div().flex().flex_col().items_center().py_1().child(
                    div()
                        .text_size(px(30.))
                        .font_semibold()
                        .child(format!("\u{2B50} {}", group_digits(amount))),
                ),
            );
            body = body.child(Slider::new(&boxed.slider).bg(cx.theme().primary));
            if already > 0 {
                body = body.child(div().text_xs().text_color(muted).child(tr_with(
                    keys::Q_LEFTOVERS_YOU_SENT_TO_SUPPORT,
                    &[(
                        "amount",
                        &if already == 1 {
                            tr(keys::Q_MONEY_1_STAR).to_string()
                        } else {
                            tr_with(
                                keys::Q_MONEY_STARS_4,
                                &[("group_digits", &(group_digits(already)).to_string())],
                            )
                        },
                    )],
                )));
            }
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Switch::new("paid-reaction-top")
                            .checked(boxed.sender == PaidSender::Regular)
                            .accessibility_label(tr(keys::PAID_REACT_SHOW_IN_TOP))
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                if let Some(boxed) = this.paid_reaction.boxed.as_mut() {
                                    boxed.sender = if *on {
                                        PaidSender::Regular
                                    } else {
                                        PaidSender::Anonymous
                                    };
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child(tr(keys::PAID_REACT_SHOW_IN_TOP))),
            );
            // Channels the account manages: send the Stars as one of them.
            for (ix, channel_id) in session
                .payments
                .paid
                .channels_for(boxed.chat_id.0)
                .iter()
                .enumerate()
            {
                let chosen = boxed.sender == PaidSender::Chat(*channel_id);
                let channel_id = *channel_id;
                let title = session.sender_label(quill::telegram::envelope::MessageSender::Chat {
                    chat_id: channel_id,
                });
                body = body.child(
                    Button::new(("paid-reaction-as", ix as u64))
                        .label(tr_with(
                            keys::Q_MONEY_SEND_AS,
                            &[("title", &title.to_string())],
                        ))
                        .small()
                        .when(chosen, |b| b.primary())
                        .when(!chosen, |b| b.ghost())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(boxed) = this.paid_reaction.boxed.as_mut() {
                                boxed.sender = if boxed.sender == PaidSender::Chat(channel_id) {
                                    PaidSender::Regular
                                } else {
                                    PaidSender::Chat(channel_id)
                                };
                            }
                            cx.notify();
                        })),
                );
            }
            let reactors = message.map(|m| m.paid_reactors()).unwrap_or(&[]);
            if !reactors.is_empty() {
                body = body.child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(muted)
                        .child(tr(keys::PAID_REACT_TOP_TITLE)),
                );
                for (ix, reactor) in reactors.iter().take(10).enumerate() {
                    let name = match reactor.sender {
                        _ if reactor.is_anonymous => tr(keys::PAID_REACT_ANONYMOUS).to_string(),
                        Some(sender) => session.sender_label(sender),
                        None => tr(keys::PAID_REACT_ANONYMOUS).to_string(),
                    };
                    let label = if reactor.is_me {
                        tr_with(keys::Q_MONEY_YOU, &[("name", &name.to_string())])
                    } else {
                        name.clone()
                    };
                    body =
                        body.child(
                            div()
                                .id(("paid-reactor", ix as u64))
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(super::message_text::kit_avatar_element(
                                    &name,
                                    None,
                                    px(28.),
                                ))
                                .child(div().flex_1().min_w_0().truncate().text_sm().child(label))
                                .child(div().text_sm().text_color(warning()).child(format!(
                                    "\u{2B50} {}",
                                    group_digits(reactor.star_count)
                                ))),
                        );
                }
            }
            if let Some(refusal) = refusal {
                body = body.child(
                    div()
                        .text_xs()
                        .text_color(warning())
                        .child(refusal.message()),
                );
            }
            body = body.child(
                Button::new("paid-reaction-send")
                    .label(tr_with(
                        keys::Q_MONEY_SEND,
                        &[("group_digits", &(group_digits(amount)).to_string())],
                    ))
                    .primary()
                    .disabled(refusal.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.send_paid_box(cx))),
            );
            dialog
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
                .on_close(on_close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    /// The Star reaction box: amount slider, Top Senders, Send.
    PaidReaction => DialogSpec::new(
        3850,
        |app| app.paid_reaction.boxed.is_some(),
        QuillApp::build_paid_reaction_dialog,
    )
    .closes(|app, _, cx| app.close_paid_box(cx)),
}
