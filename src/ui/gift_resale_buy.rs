//! Buying a resale listing (tdesktop `star_gift_box.cpp`,
//! `ShowBuyResaleGiftBox`): the collectible and its attributes, "Send a
//! Gift" when it goes to someone else, then the price question with
//! "Pay in Stars" / "Pay in Grams". Only the last button spends anything.

use super::app::QuillApp;
use super::gift_manage_ui::hint;
use super::gift_market_ui::{BoxView, collectible_tile};
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_market::{BuyStep, ResaleBuy, listing_price};
use quill::marketplace::GiftPrice;
use quill::premium_hub::upgraded::Rarity;

/// "★1,250" / "12.50 Grams" in sentences and buttons.
pub(super) fn price_text(price: GiftPrice) -> String {
    match price {
        GiftPrice::Stars(n) => format!("\u{2605}{}", quill::premium_hub::group_digits(n)),
        GiftPrice::TonCents(n) => format!(
            "{}.{:02} Grams",
            quill::premium_hub::group_digits(n / 100),
            n % 100
        ),
    }
}

/// One attribute row: label, name and the rarity tag.
pub(super) fn attribute_row(label: &'static str, name: &str, rarity: Rarity, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_3()
        .text_sm()
        .child(div().text_color(cx.theme().muted_foreground).child(label))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().font_medium().child(name.to_string()))
                .child(
                    div()
                        .px_1p5()
                        .rounded_full()
                        .bg(cx.theme().accent)
                        .text_xs()
                        .text_color(cx.theme().accent_foreground)
                        .child(rarity.label()),
                ),
        )
}

impl QuillApp {
    fn resale_buy_mut(&mut self) -> Option<&mut ResaleBuy> {
        self.gift_market_mut()
            .and_then(|market| market.buy.as_mut())
    }

    pub(super) fn set_buy_step(&mut self, step: BuyStep, cx: &mut Context<Self>) {
        if let Some(buy) = self.resale_buy_mut() {
            buy.step = step;
        }
        cx.notify();
    }

    fn set_buy_grams(&mut self, grams: bool, cx: &mut Context<Self>) {
        if let Some(buy) = self.resale_buy_mut() {
            let gram_only = buy.listing.gift.resale.is_some_and(|r| r.gram_only);
            buy.pay_in_grams = grams || gram_only;
            buy.price_changed = None;
        }
        cx.notify();
    }

    /// "Buy for …": `sendResoldGift` at the price on the button.
    fn confirm_resale_buy(&mut self, price: GiftPrice, cx: &mut Context<Self>) {
        let text = self
            .payments
            .gift_message_input
            .read(cx)
            .value()
            .to_string();
        let hide = self.payments.gift_hide_name;
        match self.live.as_mut() {
            Some(live) => {
                if live.driver.buy_gift_resale(price, &text, hide).is_err() {
                    self.connection.status_note = "Couldn't buy the gift.".into();
                }
            }
            None => {
                self.connection.status_note = "Buying gifts needs a signed-in account.".into();
            }
        }
        cx.notify();
    }

    pub(super) fn resale_buy_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Buy", div());
        };
        let Some(buy) = session.payments.gift_market.buy.as_ref() else {
            return BoxView::new("Buy", div());
        };
        let gift = &buy.listing.gift;
        let name = gift.full_name();
        let price = buy.price();
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_3();
        match buy.step {
            BuyStep::View => {
                body = body
                    .child(div().flex().justify_center().child(collectible_tile(
                        ("resale-buy-cover", 0),
                        gift,
                        session,
                        &roots,
                        160.,
                    )))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(attribute_row(
                                "Model",
                                &gift.model.name,
                                gift.model.rarity,
                                cx,
                            ))
                            .child(attribute_row(
                                "Backdrop",
                                &gift.backdrop.name,
                                gift.backdrop.rarity,
                                cx,
                            ))
                            .child(attribute_row(
                                "Symbol",
                                &gift.symbol.name,
                                gift.symbol.rarity,
                                cx,
                            )),
                    );
                if gift.resale.is_some_and(|r| r.gram_only) {
                    body = body.child(hint("The seller only accepts Grams as payment.", cx));
                }
            }
            BuyStep::Send => {
                let about = format!(
                    "You can hide your name and message from visitors to {}'s profile. {} will still see your name and message.",
                    buy.recipient.name, buy.recipient.name
                );
                body = body
                    .child(
                        Textarea::new(&self.payments.gift_message_input)
                            .aria_label("Enter Message"),
                    )
                    .child(
                        Checkbox::new("resale-buy-hide-name")
                            .label("Hide My Name")
                            .checked(self.payments.gift_hide_name)
                            .on_click(cx.listener(|this, &on, _, cx| {
                                this.payments.gift_hide_name = on;
                                cx.notify();
                            })),
                    )
                    .child(hint(about, cx));
            }
            BuyStep::Confirm => {
                let resale = gift.resale;
                let both = resale.is_some_and(|r| !r.gram_only && r.stars > 0 && r.gram_cents > 0);
                if both {
                    let owner = cx.entity().downgrade();
                    body = body.child(
                        TabBar::new("resale-buy-currency")
                            .pill()
                            .child(Tab::new().label("Pay in Stars"))
                            .child(Tab::new().label("Pay in Grams"))
                            .selected_index(usize::from(buy.pay_in_grams))
                            .on_click(move |ix, _, cx| {
                                let grams = *ix == 1;
                                let _ = owner.update(cx, |this, cx| this.set_buy_grams(grams, cx));
                            }),
                    );
                }
                let shown = price.map(price_text).unwrap_or_default();
                if let Some(changed) = buy.price_changed {
                    body = body
                        .child(div().text_sm().font_semibold().child("Price change!"))
                        .child(div().text_sm().child(format!(
                            "This gift price was changed and now is {}. Do you still want to buy?",
                            price_text(changed)
                        )));
                } else if buy.recipient.is_self {
                    body = body.child(
                        div()
                            .text_sm()
                            .child(format!("Do you want to buy {name} for {shown}?")),
                    );
                } else {
                    body = body.child(div().text_sm().child(format!(
                        "Do you want to buy {name} for {shown} and gift it to {}?",
                        buy.recipient.name
                    )));
                }
                // The other currency's amount (tdesktop "Equals to").
                let other = listing_price(gift, !buy.pay_in_grams).filter(|p| Some(*p) != price);
                if both && let Some(other) = other {
                    body = body.child(hint(format!("Equals to {}", price_text(other)), cx));
                }
                if let Some(balance) = session.payments.hub.balance {
                    body = body.child(div().text_xs().text_color(muted).child(format!(
                        "Your balance is \u{2605}{}.",
                        quill::premium_hub::group_digits(balance.stars)
                    )));
                }
            }
        }
        let step = buy.step;
        let is_self = buy.recipient.is_self;
        let sending = buy.sending;
        let action = match (step, price) {
            (_, None) => Button::new("resale-buy-next")
                .label("Not for sale")
                .disabled(true),
            (BuyStep::View, Some(price)) => Button::new("resale-buy-next")
                .label(format!("Buy for {}", price_text(price)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    let next = if is_self {
                        BuyStep::Confirm
                    } else {
                        BuyStep::Send
                    };
                    this.set_buy_step(next, cx);
                })),
            (BuyStep::Send, Some(price)) => Button::new("resale-buy-next")
                .label(format!("Buy for {}", price_text(price)))
                .on_click(cx.listener(|this, _, _, cx| this.set_buy_step(BuyStep::Confirm, cx))),
            (BuyStep::Confirm, Some(price)) => Button::new("resale-buy-confirm")
                .label(if sending {
                    "Buying\u{2026}".to_string()
                } else {
                    format!("Buy for {}", price_text(price))
                })
                .loading(sending)
                .disabled(sending)
                .on_click(cx.listener(move |this, _, _, cx| this.confirm_resale_buy(price, cx))),
        };
        let back = Button::new("resale-buy-back")
            .label(if step == BuyStep::View {
                "Back"
            } else {
                "Cancel"
            })
            .ghost()
            .on_click(cx.listener(move |this, _, window, cx| match step {
                BuyStep::View => {
                    this.market_back(cx);
                    this.close_kit_dialog_if_done(super::shell::DialogKind::GiftMarket, window, cx);
                }
                BuyStep::Send => this.set_buy_step(BuyStep::View, cx),
                BuyStep::Confirm if is_self => this.set_buy_step(BuyStep::View, cx),
                BuyStep::Confirm => this.set_buy_step(BuyStep::Send, cx),
            }));
        let title = match step {
            BuyStep::View => name,
            BuyStep::Send => "Send a Gift".to_string(),
            BuyStep::Confirm => format!("Buy {name}"),
        };
        BoxView::new(title, body).footer(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(back)
                .child(action.primary()),
        )
    }
}
