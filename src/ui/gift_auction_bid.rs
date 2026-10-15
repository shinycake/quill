//! "Place a Bid" (tdesktop `star_gift_auction_box.cpp`, `AuctionBidBox`):
//! a slider with the amount bubble, a custom amount, the minimum bid, the
//! round countdown and the gifts left, the user's place and the top three
//! bidders. The button names the Stars it spends.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use super::chat_theme::success;
use super::gift_market_state::{BidSliderUi, CustomBidUi, MarketBox};
use super::gift_market_ui::BoxView;
use super::shell::DialogKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::gift_market::{
    AuctionState, BID_SLIDER_STEPS, BidAction, BidKind, BidSlider, MAX_CUSTOM_BID, TopBid,
    bid_subtitle, bid_title, nice_countdown, seconds_left, short_stars,
};
use quill::lang::{keys, tr, tr_with};
use quill::local_time::now_unix;
use quill::premium_hub::group_digits;
use quill::state::Session;
use quill::telegram::envelope::MessageSender;
use std::path::PathBuf;

/// Keeps `amount` if the slider may stop there (at least the minimum,
/// or the user's own bid), else the slider's start.
fn allowed(range: BidSlider, amount: i64) -> i64 {
    if amount >= range.min || (range.mine > 0 && amount == range.mine) {
        amount
    } else {
        range.initial()
    }
}

fn stars_text(n: i64) -> String {
    format!("\u{2605}{}", group_digits(n))
}

/// One of the three blocks under the slider: a value and its caption.
fn info_block(icon: Option<AnyElement>, value: String, caption: &'static str, cx: &App) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .gap_0p5()
        .py_2()
        .rounded_lg()
        .bg(cx.theme().secondary)
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .text_sm()
                .font_semibold()
                .children(icon)
                .child(value),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
}

/// A bidder row: place, photo, name and Stars.
fn bidder_row(
    place: String,
    sender: MessageSender,
    stars: i64,
    session: &Session,
    roots: &[PathBuf],
    cx: &App,
) -> Div {
    let (name, photo) = super::history::reactor_avatar(&sender, Some(session), roots);
    let name = if name.is_empty() {
        session.sender_label(sender)
    } else {
        name
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .text_sm()
        .child(
            div()
                .w(px(28.))
                .flex_none()
                .text_center()
                .font_semibold()
                .child(place),
        )
        .child(super::chat_row::chat_avatar(&name, photo.as_deref(), 28.))
        .child(div().flex_1().min_w_0().truncate().child(name))
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .font_semibold()
                .child(super::earn::star_icon(12.))
                .child(group_digits(stars)),
        )
        .text_color(cx.theme().foreground)
}

impl QuillApp {
    /// Builds (or rebuilds, when a new bid moved the range) the slider of
    /// the bid box for `gift_id`.
    fn ensure_bid_slider(&mut self, gift_id: i64, cx: &mut Context<Self>) {
        let Some(state) = self
            .session()
            .and_then(|s| s.payments.gift_market.auctions.get(&gift_id))
        else {
            return;
        };
        let range = state.slider();
        let bounds = (range.min, range.max);
        let previous = self
            .gift_market
            .slider
            .as_ref()
            .filter(|ui| ui.gift_id == gift_id);
        if previous.is_some_and(|ui| ui.bounds == bounds) {
            return;
        }
        let chosen = allowed(range, previous.map_or(range.initial(), |ui| ui.chosen));
        let slider = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(BID_SLIDER_STEPS)
                .step(1.)
                .default_value(range.step_of(chosen))
        });
        cx.subscribe(&slider, move |this, _, event: &SliderEvent, cx| {
            let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
            let Some(state) = this
                .session()
                .and_then(|s| s.payments.gift_market.auctions.get(&gift_id))
            else {
                return;
            };
            let amount = state.slider().amount_at(value.end());
            if let Some(ui) = this.gift_market.slider.as_mut() {
                ui.chosen = amount;
            }
            cx.notify();
        })
        .detach();
        self.gift_market.slider = Some(BidSliderUi {
            gift_id,
            state: slider,
            chosen,
            bounds,
        });
    }

    /// "+" and the bubble: the "Custom Amount" field.
    fn open_custom_bid(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self.gift_market.slider.as_ref().map_or(0, |ui| ui.chosen);
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(tr(keys::Q_MONEY_STARS));
            input.set_value(chosen.to_string(), window, cx);
            input
        });
        self.gift_market.custom = Some(CustomBidUi { input, error: None });
        cx.notify();
    }

    fn save_custom_bid(&mut self, gift_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(custom) = self.gift_market.custom.as_ref() else {
            return;
        };
        let typed: String = custom
            .input
            .read(cx)
            .value()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let Some(state) = self.auction_state_cloned(gift_id) else {
            return;
        };
        let least = state.min_bid_for_me();
        let amount = typed.parse::<i64>().unwrap_or(0);
        let error = if amount < least && amount != state.my_stars() {
            Some(tr_with(
                keys::Q_MONEY_THE_MINIMUM_BID_IS,
                &[("stars_text", &(stars_text(least)).to_string())],
            ))
        } else if amount > MAX_CUSTOM_BID {
            Some(tr_with(
                keys::Q_MONEY_THE_MAXIMUM_BID_IS,
                &[("stars_text", &(stars_text(MAX_CUSTOM_BID)).to_string())],
            ))
        } else {
            None
        };
        if let Some(error) = error {
            if let Some(custom) = self.gift_market.custom.as_mut() {
                custom.error = Some(error);
            }
            cx.notify();
            return;
        }
        self.gift_market.custom = None;
        let range = state.slider();
        if let Some(ui) = self.gift_market.slider.as_mut() {
            ui.chosen = amount;
            let step = range.step_of(amount);
            ui.state
                .update(cx, |slider, cx| slider.set_value(step, window, cx));
        }
        cx.notify();
    }

    fn auction_state_cloned(&self, gift_id: i64) -> Option<AuctionState> {
        self.session()?
            .payments
            .gift_market
            .auctions
            .get(&gift_id)
            .cloned()
    }

    /// The button: places, raises or (nothing to raise) closes.
    fn submit_auction_bid(&mut self, gift_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.auction_state_cloned(gift_id) else {
            return;
        };
        let chosen = self.gift_market.slider.as_ref().map_or(0, |ui| ui.chosen);
        let stars = match state.bid_action(chosen) {
            BidAction::Close => {
                self.market_back(cx);
                self.close_kit_dialog_if_done(DialogKind::GiftMarket, window, cx);
                return;
            }
            BidAction::Place(n) | BidAction::Increase(n) => n,
        };
        let Some(MessageSender::User { user_id }) = self.auction_recipient() else {
            self.connection.status_note = tr(keys::Q_MONEY_AUCTION_GIFTS_CAN_ONLY_BE).into();
            cx.notify();
            return;
        };
        let add = stars.saturating_sub(state.my_stars());
        let poor = self
            .session()
            .and_then(|s| s.payments.hub.balance)
            .is_some_and(|balance| balance.stars < add);
        if poor {
            self.connection.status_note = tr(keys::Q_MONEY_YOUR_STARS_BALANCE_IS_TOO).into();
            self.open_stars(cx);
            cx.notify();
            return;
        }
        let text = self
            .payments
            .gift_message_input
            .read(cx)
            .value()
            .to_string();
        let hide = self.gift_market.hide_name;
        let sent = match self.live.as_mut() {
            Some(live) => live
                .driver
                .bid_gift_auction(gift_id, stars, user_id, &text, hide)
                .is_ok(),
            None => {
                self.connection.status_note = tr(keys::Q_MONEY_BIDS_NEED_A_SIGNED_IN).into();
                false
            }
        };
        if sent {
            // The send and change boxes are done; the bid box stays and
            // shows the new place when the update arrives.
            self.gift_market.stack.retain(|entry| {
                !matches!(
                    entry,
                    MarketBox::AuctionSend { .. } | MarketBox::ChangeRecipient { .. }
                )
            });
        } else if self.live.is_some() {
            self.connection.status_note = tr(keys::Q_MONEY_COULDNT_PLACE_THE_BID).into();
        }
        cx.notify();
    }

    pub(super) fn auction_bid_box(&mut self, gift_id: i64, cx: &mut Context<Self>) -> BoxView {
        self.ensure_bid_slider(gift_id, cx);
        let roots = self.media_display_roots();
        let Some(session) = self.session() else {
            return BoxView::new("Place a Bid", div());
        };
        let Some(state) = session.payments.gift_market.auctions.get(&gift_id) else {
            return BoxView::new("Place a Bid", div());
        };
        let Some(slider) = self.gift_market.slider.as_ref() else {
            return BoxView::new("Place a Bid", div());
        };
        self.request_animation_tick(1, cx);
        let now = now_unix();
        let started = state.started(now);
        let chosen = slider.chosen;
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_3().child(
            div()
                .text_sm()
                .text_center()
                .text_color(muted)
                .child(bid_subtitle(state.per_round())),
        );
        // The amount bubble rides above the thumb (tdesktop's
        // `PaidReactionSlider` bubble); a click opens the custom amount.
        let subtext = state.bubble_subtext(chosen);
        let ratio = (state.slider().step_of(chosen) / BID_SLIDER_STEPS).clamp(0.12, 0.88);
        let bubble = div()
            .id("auction-bid-bubble")
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .px_4()
            .py_1()
            .rounded_full()
            .bg(cx.theme().primary)
            .text_color(cx.theme().primary_foreground)
            .cursor_pointer()
            .role(Role::Button)
            .aria_label(tr_with(
                keys::Q_MONEY_BID,
                &[
                    ("stars_text", &(stars_text(chosen)).to_string()),
                    ("subtext", &subtext.to_string()),
                ],
            ))
            .on_click(cx.listener(|this, _, window, cx| {
                this.open_custom_bid(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_lg()
                    .font_semibold()
                    .whitespace_nowrap()
                    .child(super::earn::star_icon(16.))
                    .child(group_digits(chosen)),
            )
            .children(
                (!subtext.is_empty()).then(|| div().text_xs().whitespace_nowrap().child(subtext)),
            );
        body = body.child(
            div()
                .flex()
                .items_end()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div().relative().w_full().h(px(56.)).child(
                                // A zero-width anchor at the thumb's ratio
                                // centres the bubble over it.
                                div()
                                    .absolute()
                                    .bottom_0()
                                    .left(relative(ratio))
                                    .w(px(0.))
                                    .flex()
                                    .justify_center()
                                    .child(bubble),
                            ),
                        )
                        .child(Slider::new(&slider.state).bg(cx.theme().primary)),
                )
                .child(
                    Button::new("auction-bid-custom")
                        .icon(IconName::Plus)
                        .ghost()
                        .small()
                        .tooltip(tr(keys::AUCTION_BID_CUSTOM_TITLE))
                        .accessibility_label(tr(keys::AUCTION_BID_CUSTOM_TITLE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_custom_bid(window, cx);
                        })),
                ),
        );
        if let Some(custom) = self.gift_market.custom.as_ref() {
            body =
                body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .rounded_lg()
                        .bg(cx.theme().secondary)
                        .child(
                            div()
                                .text_sm()
                                .font_semibold()
                                .child(tr(keys::AUCTION_BID_CUSTOM_TITLE)),
                        )
                        .child(Input::new(&custom.input))
                        .children(custom.error.clone().map(|error| {
                            div().text_xs().text_color(cx.theme().danger).child(error)
                        }))
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("auction-custom-cancel")
                                        .label(tr(keys::CANCEL))
                                        .ghost()
                                        .small()
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.gift_market.custom = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("auction-custom-save")
                                        .label(tr(keys::SETTINGS_SAVE))
                                        .primary()
                                        .small()
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.save_custom_bid(gift_id, window, cx);
                                        })),
                                ),
                        ),
                );
        }
        // Minimum bid, the countdown and the gifts left.
        let least = state.min_bid_for_me();
        let (countdown, caption) = if started {
            (
                nice_countdown(seconds_left(state.next_round_at(), now)),
                tr(keys::AUCTION_BID_UNTIL),
            )
        } else {
            (
                nice_countdown(seconds_left(state.start_date(), now)),
                tr(keys::AUCTION_BID_BEFORE_START),
            )
        };
        let left = state.active().map_or(0, |active| active.left);
        body = body.child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id("auction-min-bid")
                        .flex_1()
                        .flex()
                        .cursor_pointer()
                        .role(Role::Button)
                        .aria_label(tr_with(
                            keys::Q_MONEY_MINIMUM_BID_2,
                            &[("stars_text", &(stars_text(least)).to_string())],
                        ))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.set_bid_amount(gift_id, least, window, cx);
                        }))
                        .child(info_block(
                            Some(super::earn::star_icon(14.)),
                            short_stars(least),
                            tr(keys::Q_MONEY_MINIMUM_BID),
                            cx,
                        )),
                )
                .child(info_block(None, countdown, caption, cx))
                .child(info_block(
                    None,
                    format!("\u{1F381} {}", short_stars(i64::from(left))),
                    "left",
                    cx,
                )),
        );
        // The user's place and the top three.
        let rows = state.bid_rows(chosen, session.my_user_id);
        let kind_color = match rows.kind {
            BidKind::Setting => cx.theme().foreground,
            BidKind::Winning => success().into(),
            BidKind::Losing => cx.theme().danger,
        };
        let me = session
            .my_user_id
            .map(|user_id| MessageSender::User { user_id });
        if let Some(me) = me {
            body = body
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(kind_color)
                        .child(rows.kind.title(started)),
                )
                .child(bidder_row(
                    format!("#{}", rows.position),
                    me,
                    chosen,
                    session,
                    &roots,
                    cx,
                ));
        }
        if !rows.top.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .font_semibold()
                    .child(tr(keys::AUCTION_BID_WINNERS_TITLE)),
            );
            let medals = ["\u{1F947}", "\u{1F948}", "\u{1F949}"];
            for (top, medal) in rows.top.iter().zip(medals) {
                let TopBid { user_id, stars, .. } = *top;
                body = body.child(bidder_row(
                    medal.to_string(),
                    MessageSender::User { user_id },
                    stars,
                    session,
                    &roots,
                    cx,
                ));
            }
        }
        if let Some(balance) = session.payments.hub.balance {
            body = body.child(div().text_xs().text_color(muted).child(tr_with(
                keys::Q_MONEY_YOUR_BALANCE_IS,
                &[("stars", &(stars_text(balance.stars)).to_string())],
            )));
        }
        let bidding = session.payments.gift_market.bidding == Some(gift_id);
        let action = state.bid_action(chosen);
        let button = Button::new("auction-bid-submit")
            .label(action.label(state.my_stars()))
            .primary()
            .loading(bidding)
            .disabled(bidding)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.submit_auction_bid(gift_id, window, cx);
            }));
        let view = BoxView::new(bid_title(started), body)
            .wide()
            .footer(div().flex().justify_end().child(button));
        match self.auction_menu(state, cx) {
            Some(menu) => view.title_action(menu),
            None => view,
        }
    }

    /// Moves the slider to `amount` (the minimum-bid block).
    pub(super) fn set_bid_amount(
        &mut self,
        gift_id: i64,
        amount: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.auction_state_cloned(gift_id) else {
            return;
        };
        let step = state.slider().step_of(amount);
        if let Some(ui) = self.gift_market.slider.as_mut() {
            ui.chosen = amount;
            ui.state
                .update(cx, |slider, cx| slider.set_value(step, window, cx));
        }
        cx.notify();
    }
}
