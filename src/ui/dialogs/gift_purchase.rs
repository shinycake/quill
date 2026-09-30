use super::super::app::QuillApp;
use super::super::shell::{DialogKind, QuillShell};
use gpui_kit::base::Selectable;
use gpui_kit::component::button::*;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::telegram::requests::GiftResalePrice;
use std::cell::RefCell;
use std::rc::Rc;

/// Slice parity:gifts-signed-comment — the "Buy collectible gift" dialog.
/// Fields map 1:1 to `sendResoldGift` (TDLib 1.8.67, `schema/td_api.tl:15404`):
/// gift name, owner, price, and the personal `text` comment ("signed"
/// message) attached to the purchase offer. The gift name / owner are
/// entered manually — gift browsing/discovery is out of this slice.
pub struct GiftPurchaseDialog {
    pub(crate) gift_name_input: Entity<TextareaState>,
    pub(crate) owner_id_input: Entity<TextareaState>,
    pub(crate) price_input: Entity<TextareaState>,
    pub(crate) price_currency: GiftPriceCurrency,
    pub(crate) comment_input: Entity<TextareaState>,
    pub(crate) is_private: bool,
}

/// Slice parity:gifts-signed-comment — which `GiftResalePrice` variant the
/// price amount is interpreted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GiftPriceCurrency {
    Stars,
    TonGrams,
}

impl GiftPurchaseDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        // Create the comment input first: the `input` closure below holds a
        // mutable borrow of `window` for its lifetime, so it must come last.
        let comment_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Personal comment (optional)")
                .auto_grow(2, 4)
                .submit_on_enter(false)
        });
        let mut input = |cx: &mut Context<QuillApp>, placeholder: &str| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            })
        };
        Self {
            gift_name_input: input(cx, "Gift name (e.g. \"Durov's Cap\")"),
            owner_id_input: input(cx, "Owner user ID"),
            price_input: input(cx, "Price amount"),
            price_currency: GiftPriceCurrency::Stars,
            comment_input,
            is_private: false,
        }
    }

    /// Reads the dialog fields into `sendResoldGift` arguments. Returns
    /// `None` when a required field is missing or malformed (the Buy button
    /// validates on submit and shows a status note).
    pub(crate) fn to_request_args(&self, cx: &App) -> Option<GiftPurchaseArgs> {
        let text = |entity: &Entity<TextareaState>| entity.read(cx).value().to_string();
        let gift_name = text(&self.gift_name_input).trim().to_string();
        if gift_name.is_empty() {
            return None;
        }
        let owner_id: i64 = text(&self.owner_id_input).trim().parse().ok()?;
        let amount: i64 = text(&self.price_input).trim().parse().ok()?;
        if amount <= 0 {
            return None;
        }
        let price = match self.price_currency {
            GiftPriceCurrency::Stars => GiftResalePrice::Star(amount),
            GiftPriceCurrency::TonGrams => GiftResalePrice::Gram(amount),
        };
        let comment = text(&self.comment_input).trim().to_string();
        Some(GiftPurchaseArgs {
            gift_name,
            owner_user_id: quill::ids::UserId(owner_id),
            price,
            comment,
            is_private: self.is_private,
        })
    }
}

/// Slice parity:gifts-signed-comment — validated `sendResoldGift` arguments
/// read from the dialog.
pub(crate) struct GiftPurchaseArgs {
    pub gift_name: String,
    pub owner_user_id: quill::ids::UserId,
    pub price: GiftResalePrice,
    pub comment: String,
    pub is_private: bool,
}

/// Slice parity:gifts-signed-comment — kit dialog builder for the "Buy
/// collectible gift" dialog.
pub fn build_gift_purchase_dialog(
    app: &Entity<QuillApp>,
    shell: &Entity<QuillShell>,
    dialog: Dialog,
    cx: &mut App,
) -> Dialog {
    let on_close =
        QuillShell::on_close_kind(app, shell, DialogKind::GiftPurchase, |this, _, cx| {
            this.close_gift_purchase_dialog(cx);
        });
    app.update(cx, |this, cx| {
        let dialog = dialog.overlay(true).title("Buy Collectible Gift");
        let Some(dialog_state) = this.gift_purchase_dialog.as_ref() else {
            return dialog.on_close(on_close);
        };
        let is_private = dialog_state.is_private;
        let price_currency = dialog_state.price_currency;
        let body = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Offer to purchase an upgraded gift from its current owner via the Marketplace."),
            )
            .child(Textarea::new(&dialog_state.gift_name_input).h(px(40.)))
            .child(Textarea::new(&dialog_state.owner_id_input).h(px(40.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(
                        Textarea::new(&dialog_state.price_input).h(px(40.)),
                    ))
                    .child(
                        Button::new("gift-purchase-currency-stars")
                            .label("Stars")
                            .selected(price_currency == GiftPriceCurrency::Stars)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.gift_purchase_dialog.as_mut() {
                                    d.price_currency = GiftPriceCurrency::Stars;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        Button::new("gift-purchase-currency-ton")
                            .label("TON")
                            .selected(price_currency == GiftPriceCurrency::TonGrams)
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.gift_purchase_dialog.as_mut() {
                                    d.price_currency = GiftPriceCurrency::TonGrams;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(Textarea::new(&dialog_state.comment_input))
            .child(
                div().flex().items_center().gap_2().child(
                    Switch::new("gift-purchase-private")
                        .checked(is_private)
                        .accessibility_label("Private purchase")
                        .on_click(cx.listener(move |this, &on, _, cx| {
                            if let Some(d) = this.gift_purchase_dialog.as_mut() {
                                d.is_private = on;
                                cx.notify();
                            }
                        })),
                ),
            );
        let footer = div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("gift-purchase-cancel")
                    .label("Cancel")
                    .ghost()
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_gift_purchase_dialog(cx);
                        this.close_kit_dialog_if_done(DialogKind::GiftPurchase, window, cx);
                    })),
            )
            .child(Button::new("gift-purchase-buy").label("Buy Gift").on_click(
                cx.listener(|this, _, window, cx| {
                    this.submit_gift_purchase_dialog(cx);
                    this.close_kit_dialog_if_done(DialogKind::GiftPurchase, window, cx);
                }),
            ));
        let body = body.into_any_element();
        dialog
            .content({
                let body = Rc::new(RefCell::new(Some(body)));
                move |content, _, _| {
                    let body = body
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| div().into_any_element());
                    content.child(body)
                }
            })
            .footer(footer)
            .on_close(on_close)
    })
}
