//! "Select Gifts" for a craft slot (tdesktop `ShowSelectGiftBox`): the
//! user's other collectibles of the collection, then the suitable listings
//! on sale, which are bought into the slot.

use super::app::QuillApp;
use super::gift_craft_ui::gate_notice;
use super::gift_market_state::MarketBox;
use super::gift_market_ui::{BoxView, collectible_tile, price_line};
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_manage::GiftRecipient;
use quill::gift_market::{CraftGate, ResaleBuy, format_percent, listing_price, suitable_on_sale};
use quill::local_time::now_unix;
use quill::telegram::envelope::MessageSender;

/// A tile's side in the select box.
const TILE: f32 = 96.;

/// A small badge in a tile's top right corner.
fn corner_badge(text: String) -> Div {
    div()
        .absolute()
        .top_1()
        .right_1()
        .px_1p5()
        .rounded_full()
        .bg(gpui_kit::black().opacity(0.35))
        .text_xs()
        .font_semibold()
        .text_color(gpui_kit::white())
        .child(text)
}

impl QuillApp {
    /// A gift of the user's own: into the first free slot, unless a gate
    /// says it can't go there.
    pub(super) fn pick_craft_gift(&mut self, received_gift_id: &str, cx: &mut Context<Self>) {
        let Some(craft) = self.craft_session() else {
            return;
        };
        let Some(gift) = craft
            .candidates
            .iter()
            .find(|gift| gift.received_gift_id == received_gift_id)
            .cloned()
        else {
            return;
        };
        let gate = gift.gate(craft.slots.is_empty(), now_unix());
        if let Some(notice) = gate_notice(gate) {
            self.market_push(notice, cx);
            return;
        }
        if gate != CraftGate::Ready {
            return;
        }
        if let Some(craft) = self.craft_mut() {
            craft.add(gift);
        }
        self.market_replace(MarketBox::Craft, cx);
    }

    /// A listing on sale: bought for the user, then it fills a slot.
    fn pick_craft_listing(&mut self, name: String, cx: &mut Context<Self>) {
        let started = match self.live.as_mut() {
            Some(live) => live.driver.start_gift_resale_buy(&name, None, true).is_ok(),
            None => {
                let me = self.session().and_then(|s| s.my_user_id).unwrap_or(0);
                self.gift_market_mut().is_some_and(|market| {
                    let listing = market
                        .craft
                        .as_ref()
                        .and_then(|craft| {
                            craft.market.listings.iter().find(|l| l.gift.name == name)
                        })
                        .cloned();
                    listing.is_some_and(|listing| {
                        let recipient = GiftRecipient {
                            sender: MessageSender::User { user_id: me },
                            name: String::new(),
                            is_self: true,
                            is_channel: false,
                        };
                        market.buy = Some(ResaleBuy::new(listing, recipient, true));
                        true
                    })
                })
            }
        };
        if started {
            self.payments.gift_hide_name = false;
            // The purchase returns to the craft box, not to this list.
            self.gift_market.stack.pop();
            self.gift_market.push(MarketBox::Craft);
            self.gift_market.push(MarketBox::Buy);
        }
        cx.notify();
    }

    fn more_craft_candidates(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_craft_candidates();
        }
        cx.notify();
    }

    fn more_craft_listings(&mut self, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.load_more_gift_resale(true);
        }
        cx.notify();
    }

    pub(super) fn craft_select_box(&mut self, cx: &mut Context<Self>) -> BoxView {
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Select Gifts", div());
        };
        let Some(craft) = session.payments.gift_market.craft.as_ref() else {
            return BoxView::new("Select Gifts", div());
        };
        let muted = cx.theme().muted_foreground;
        let heading = |text: String| div().text_sm().font_semibold().child(text);
        let mut body = div().flex().flex_col().gap_3();
        if !craft.slots.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("You need to add up to 3 gifts from the same collection for crafting."),
            );
        }
        body = body.child(heading("Your gifts".into()));
        let mut own = div().flex().flex_wrap().gap_2();
        let mut own_count = 0;
        for gift in craft.free_candidates() {
            own_count += 1;
            let id = gift.received_gift_id.clone();
            let tile = collectible_tile(
                ("craft-own", gift.gift.id as u64),
                &gift.gift,
                session,
                &roots,
                TILE,
            )
            .child(corner_badge(format_percent(gift.gift.craft_per_mille)));
            own = own.child(
                div()
                    .id(SharedString::from(format!("craft-own-{id}")))
                    .rounded_lg()
                    .cursor_pointer()
                    .tab_index(0)
                    .role(Role::Button)
                    .aria_label(gift.gift.full_name())
                    .on_click(cx.listener(move |this, _, _, cx| this.pick_craft_gift(&id, cx)))
                    .child(tile),
            );
        }
        if craft.candidates_loading && own_count == 0 {
            body = body.child(div().text_sm().text_color(muted).child("Loading\u{2026}"));
        } else if own_count == 0 {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("You don't have other gifts from this collection."),
            );
        } else {
            body = body.child(own);
        }
        if !craft.candidates_loading && !craft.candidates_offset.is_empty() {
            body = body.child(
                Button::new("craft-own-more")
                    .label("Show more")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_craft_candidates(cx))),
            );
        }
        let market = &craft.market;
        body = body.child(heading(suitable_on_sale(market.total_count)));
        let mut sale = div().flex().flex_wrap().gap_2();
        for listing in &market.listings {
            let gift = &listing.gift;
            let name = gift.name.clone();
            let price = listing_price(gift, false);
            let label = format!(
                "{}, {}",
                gift.full_name(),
                price.map(|price| price.label()).unwrap_or_default()
            );
            let tile =
                collectible_tile(("craft-sale", gift.id as u64), gift, session, &roots, TILE)
                    .children(price.map(|price| {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .bg(gpui_kit::black().opacity(0.25))
                            .child(price_line(price, gpui_kit::white()))
                    }));
            sale = sale.child(
                div()
                    .id(SharedString::from(format!("craft-sale-{name}")))
                    .rounded_lg()
                    .cursor_pointer()
                    .tab_index(0)
                    .role(Role::Button)
                    .aria_label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pick_craft_listing(name.clone(), cx);
                    }))
                    .child(tile),
            );
        }
        if market.loading && market.listings.is_empty() {
            body = body.child(div().text_sm().text_color(muted).child("Loading\u{2026}"));
        } else if market.listings.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Could not find other gifts from this collection."),
            );
        } else {
            body = body.child(sale);
        }
        if !market.loading && market.has_more() {
            body = body.child(
                Button::new("craft-sale-more")
                    .label("Show more")
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.more_craft_listings(cx))),
            );
        }
        BoxView::new("Select Gifts", body)
            .footer(div().flex().justify_end().child(self.market_back_button(
                "craft-select-back",
                "Back",
                cx,
            )))
            .wide()
    }
}
