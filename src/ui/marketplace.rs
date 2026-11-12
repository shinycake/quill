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
    /// until it closes (`PaymentUi::marketplace`).
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
        let running = self
            .session()
            .and_then(|s| s.payments.marketplace_gift.as_ref())
            .filter(|g| g.is_loading() || g.is_sending())
            .map(|g| g.chat_id);
        // A purchase on its way is for the chat it started in: another
        // chat's box waits for it instead of taking it over.
        if running.is_some_and(|started_in| started_in != chat) {
            self.set_note(StatusNote::silent(tr(
                keys::Q_PEOPLE_REQUEST_ALREADY_IN_FLIGHT,
            )));
            cx.notify();
            return;
        }
        // The kept draft resumes when the session's purchase is the one it
        // asked for, whatever stage that is in and however the box closed
        // (the window, the chat left or blocked): a failure that landed
        // meanwhile shows.
        let resume = self.payments.marketplace.resumes(
            self.session()
                .and_then(|s| s.payments.marketplace_gift.as_ref()),
        );
        if self.payments.marketplace.open_for(chat, resume) {
            self.reset_marketplace_inputs(window, cx);
        }
        self.open_box(DialogKind::Marketplace);
        cx.notify();
    }

    /// A fresh draft starts with empty inputs: the one reset, tied to
    /// `MarketplaceBox::open_for`'s fresh branch.
    fn reset_marketplace_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [
            &self.payments.marketplace_name_input,
            &self.payments.marketplace_comment_input,
        ] {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    /// The Send Gift box's recipient, as the chat a purchase for it goes
    /// to (a user's private chat has the user's id).
    pub(super) fn send_gift_recipient_chat(&self) -> Option<ChatId> {
        let send = &self.session()?.payments.gifts_manage.send;
        let recipient = send.recipient.as_ref().filter(|_| send.is_open())?;
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
        // Each cause says its own reason: no chat to buy for, no connection
        // (a demo, or signed out), a request that could not be sent.
        let asked = match (self.payments.marketplace.chat(), self.live.as_mut()) {
            (None, _) => Err(tr(keys::Q_OPS_CHOOSE_A_PRIVATE_CHAT_OR)),
            (Some(_), None) => Err(tr(keys::Q_MONEY_BUYING_GIFTS_NEEDS_A_SIGNED)),
            (Some(chat), Some(live)) => live
                .driver
                .fetch_marketplace_gift(chat, &name)
                .map_err(|_| tr(keys::Q_MONEY_COULD_NOT_LOAD_GIFTS)),
        };
        match asked {
            Ok(request) => self.payments.marketplace.asked(request),
            Err(text) => self.payments.marketplace.set_error(Some(text.to_string())),
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
                this.payments.marketplace.close();
                cx.notify();
            });
        app.update(cx, |this, cx| {
            // The quote is the one this box's draft asked for, for its chat:
            // a purchase of another chat or an earlier box never shows here.
            let draft = this.payments.marketplace.draft().cloned();
            let gift = this
                .session()
                .and_then(|s| s.payments.marketplace_gift.clone())
                .filter(|gift| draft.as_ref().is_some_and(|d| d.shows(gift)));
            let limit = this.session().and_then(|s| s.payments.gift_text_length_max);
            let marketplace_private = this.payments.marketplace.draft().is_none_or(|d| d.private);
            let busy = gift
                .as_ref()
                .is_some_and(|g| g.is_loading() || g.is_sending());
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
            // This box's own refusal (nothing was sent), apart from the
            // note a purchase's own try leaves.
            if let Some(error) = draft.as_ref().and_then(|d| d.error.clone()) {
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
                if gift.is_loading() {
                    body = body.child(div().child(tr(keys::Q_OPS_LOADING_THE_GIFT)));
                }
                if let Some(note) = gift.note() {
                    body = body.child(
                        div()
                            .id("gift-result")
                            .role(Role::Label)
                            .aria_label(note.clone())
                            .child(note),
                    );
                }
                if let Some(quote) = gift.quote().cloned() {
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
                                .selected(gift.price() == Some(price))
                                .disabled(busy || gift.is_completed())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(live) = this.live.as_mut()
                                        && let Some(g) =
                                            live.driver.session.payments.marketplace_gift.as_mut()
                                    {
                                        g.select_price(price);
                                    }
                                    cx.notify();
                                })),
                        );
                    }
                    body = body
                        .child(
                            Textarea::new(&this.payments.marketplace_comment_input)
                                .disabled(busy || gift.is_completed())
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
                                .label(if marketplace_private {
                                    tr(keys::Q_OPS_COMMENT_AND_SENDER_RECEIVER_ONLY)
                                } else {
                                    tr(keys::Q_OPS_COMMENT_AND_SENDER_VISIBLE_TO)
                                })
                                .selected(marketplace_private)
                                .disabled(busy || gift.is_completed())
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(draft) = this.payments.marketplace.open_draft_mut()
                                    {
                                        draft.private = !draft.private;
                                    }
                                    cx.notify();
                                })),
                        );
                    if let Some(price) = gift.price() {
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
                                .disabled(busy || gift.is_completed())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if this.payments.marketplace_name_input.read(cx).value().trim()
                                        != quoted_name
                                    {
                                        this.payments.marketplace.set_error(Some(
                                            tr(keys::Q_OPS_LOAD_THE_NEWLY_ENTERED_GIFT).into(),
                                        ));
                                        cx.notify();
                                        return;
                                    }
                                    let comment = this
                                        .payments
                                        .marketplace_comment_input
                                        .read(cx)
                                        .value()
                                        .to_string();
                                    let private =
                                        this.payments.marketplace.draft().is_none_or(|d| d.private);
                                    // A refusal says why in the purchase's note.
                                    this.payments.marketplace.set_error(None);
                                    if let Some(live) = this.live.as_mut() {
                                        let _ = live
                                            .driver
                                            .buy_marketplace_gift(price, &comment, private);
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
                // A quote or a purchase on its way: the box stays until the
                // answer, whatever way closes it.
                .close_button(!busy)
                .overlay_closable(!busy)
                .keyboard(!busy)
        })
    }
}

crate::ui::shell::register_dialogs! {
    Marketplace => DialogSpec::new(
        4100,
        |app| app.payments.marketplace.is_open(),
        QuillApp::build_marketplace_dialog,
    )
    .close(|app, _, cx| { app.payments.marketplace.close(); cx.notify(); }),
}
