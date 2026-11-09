#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::shell::{DialogKind, QuillShell};
use crate::ui::status_note::StatusNote;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr, tr_with};
use quill::telegram::envelope::MessageSender;

impl QuillApp {
    /// Open "Buy a collectible by name" for `chat`, the chat it buys for
    /// until it closes (`PaymentUi::marketplace_chat`).
    pub(super) fn open_marketplace_for(
        &mut self,
        chat: Option<ChatId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat) = chat else {
            self.set_note(StatusNote::silent(tr(
                keys::Q_NAVIGATION_OPEN_A_PRIVATE_CHAT_OR,
            )));
            cx.notify();
            return;
        };
        let busy = self
            .session()
            .and_then(|s| s.payments.marketplace_gift.as_ref())
            .is_some_and(|g| g.loading || g.sending);
        if !busy {
            self.payments
                .marketplace_name_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.payments
                .marketplace_comment_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            self.payments.marketplace_private = true;
            self.payments.marketplace_error = None;
            self.payments.marketplace_chat = Some(chat);
            if let Some(live) = self.live.as_mut() {
                live.driver.session.payments.marketplace_gift = None;
            } else if let Some(session) = self.demo_session.as_mut() {
                session.payments.marketplace_gift = None;
            }
        }
        self.payments.marketplace_open = true;
        self.open_box(DialogKind::Marketplace);
        cx.notify();
    }

    /// The Send Gift box's recipient, as the chat a purchase for it goes
    /// to (a user's private chat has the user's id).
    pub(super) fn send_gift_recipient_chat(&self) -> Option<ChatId> {
        let send = &self.session()?.payments.gifts_manage.send;
        let recipient = send.recipient.as_ref().filter(|_| send.open)?;
        Some(match recipient.sender {
            MessageSender::User { user_id } => ChatId(user_id),
            MessageSender::Chat { chat_id } => ChatId(chat_id),
        })
    }

    fn load_marketplace_gift(&mut self, cx: &mut Context<Self>) {
        let name = self
            .payments
            .marketplace_name_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let chat = self.payments.marketplace_chat;
        let result = chat.and_then(|chat| {
            self.live
                .as_mut()
                .map(|live| live.driver.fetch_marketplace_gift(chat, &name))
        });
        if !matches!(result, Some(Ok(_))) {
            self.payments.marketplace_error = Some(tr(keys::Q_OPS_CHOOSE_A_PRIVATE_CHAT_OR).into());
        } else {
            self.payments.marketplace_error = None;
        }
        cx.notify();
    }
    pub(super) fn build_marketplace_dialog(
        app: &Entity<Self>,
        shell: &Entity<QuillShell>,
        dialog: Dialog,
        cx: &mut App,
    ) -> Dialog {
        let close =
            QuillShell::on_close_kind(app, shell, DialogKind::Marketplace, |this, _, cx| {
                this.payments.marketplace_open = false;
                cx.notify();
            });
        app.update(cx, |this, cx| {
            let gift = this
                .session()
                .and_then(|s| s.payments.marketplace_gift.clone());
            let limit = this.session().and_then(|s| s.payments.gift_text_length_max);
            let busy = gift.as_ref().is_some_and(|g| g.loading || g.sending);
            let mut body = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .id("gift-instructions")
                        .role(Role::Label)
                        .aria_label(tr(keys::Q_OPS_LOAD_A_COLLECTIBLE_BY_ITS))
                        .child(tr(keys::Q_OPS_LOAD_A_COLLECTIBLE_BY_ITS)),
                )
                .child(
                    Textarea::new(&this.payments.marketplace_name_input)
                        .disabled(busy)
                        .aria_label(tr(keys::Q_OPS_COLLECTIBLE_GIFT_NAME)),
                )
                .child(
                    Button::new("gift-load")
                        .label(tr(keys::Q_OPS_LOAD_GIFT))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| this.load_marketplace_gift(cx))),
                );
            if let Some(error) = this.payments.marketplace_error.clone() {
                body = body.child(
                    div()
                        .id("gift-error")
                        .role(Role::Label)
                        .aria_label(error.clone())
                        .child(error),
                );
            }
            if let Some(gift) = gift {
                body = body.child(
                    div()
                        .id("gift-recipient")
                        .role(Role::Label)
                        .aria_label(tr_with(
                            keys::Q_OPS_RECIPIENT,
                            &[("recipient_name", &gift.recipient_name.to_string())],
                        ))
                        .child(tr_with(
                            keys::Q_OPS_RECIPIENT,
                            &[("recipient_name", &gift.recipient_name.to_string())],
                        )),
                );
                if gift.loading {
                    body = body.child(div().child(tr(keys::Q_OPS_LOADING_THE_GIFT)));
                }
                if let Some(note) = gift.note {
                    body = body.child(
                        div()
                            .id("gift-result")
                            .role(Role::Label)
                            .aria_label(note.clone())
                            .child(note),
                    );
                }
                if let Some(quote) = gift.quote {
                    let title = format!("{} — {}", quote.title, quote.name);
                    body = body.child(
                        div()
                            .id("gift-title")
                            .role(Role::Label)
                            .aria_label(title.clone())
                            .font_semibold()
                            .child(title),
                    );
                    for price in [quote.stars, quote.ton].into_iter().flatten() {
                        body = body.child(
                            Button::new(format!("gift-price-{}", price.label()))
                                .label(price.label())
                                .selected(gift.price == Some(price))
                                .disabled(busy || gift.completed)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut()
                                        && let Some(g) =
                                            live.driver.session.payments.marketplace_gift.as_mut()
                                        && !g.sending
                                    {
                                        g.price = Some(price);
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    body = body
                        .child(
                            Textarea::new(&this.payments.marketplace_comment_input)
                                .disabled(busy || gift.completed)
                                .aria_label(tr(keys::Q_OPS_PERSONAL_COMMENT)),
                        )
                        .child(
                            div().text_xs().child(
                                limit
                                    .map(|l| {
                                        tr_with(
                                            keys::Q_OPS_UP_TO_CHARACTERS_PAID_MESSAGE,
                                            &[("l", &l.to_string())],
                                        )
                                    })
                                    .unwrap_or(
                                        tr(keys::Q_OPS_COMMENT_LIMIT_UNAVAILABLE_RELOAD_THE).into(),
                                    ),
                            ),
                        )
                        .child(
                            Button::new("gift-private")
                                .label(if this.payments.marketplace_private {
                                    tr(keys::Q_OPS_COMMENT_AND_SENDER_RECEIVER_ONLY)
                                } else {
                                    tr(keys::Q_OPS_COMMENT_AND_SENDER_VISIBLE_TO)
                                })
                                .selected(this.payments.marketplace_private)
                                .disabled(busy || gift.completed)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.payments.marketplace_private =
                                        !this.payments.marketplace_private;
                                    cx.notify();
                                })),
                        );
                    if let Some(price) = gift.price {
                        let quoted_name = quote.name.clone();
                        body = body.child(
                            Button::new("gift-buy")
                                .label(tr_with(
                                    keys::Q_OPS_BUY_FOR_AND_SEND_TO,
                                    &[
                                        ("label", &(price.label()).to_string()),
                                        ("recipient_name", &gift.recipient_name.to_string()),
                                    ],
                                ))
                                .disabled(busy || gift.completed)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.payments.marketplace_name_input.read(cx).value().trim()
                                        != quoted_name
                                    {
                                        this.payments.marketplace_error = Some(
                                            tr(keys::Q_OPS_LOAD_THE_NEWLY_ENTERED_GIFT).into(),
                                        );
                                        cx.notify();
                                        return;
                                    }
                                    let comment = this
                                        .payments
                                        .marketplace_comment_input
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    let private = this.payments.marketplace_private;
                                    if let Some(live) = this.live.as_mut() {
                                        this.payments.marketplace_error = if live
                                            .driver
                                            .buy_marketplace_gift(price, &comment, private)
                                            .is_err()
                                        {
                                            Some(
                                                tr(keys::Q_OPS_PURCHASE_REFUSED_REVIEW_THE_QUOTE)
                                                    .into(),
                                            )
                                        } else {
                                            None
                                        };
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                }
            }
            dialog
                .overlay(true)
                .title(crate::ui::shell::dialog_title(tr(
                    keys::Q_OPS_MARKETPLACE_COLLECTIBLE_GIFT,
                )))
                .child(body)
                .on_close(close)
        })
    }
}

crate::ui::shell::register_dialogs! {
    Marketplace => DialogSpec::new(
        4100,
        |app| app.payments.marketplace_open,
        QuillApp::build_marketplace_dialog,
    )
    .close(|app, _, cx| { app.payments.marketplace_open = false; cx.notify(); }),
}
