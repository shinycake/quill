//! The gift market over the demo's chat list: the resale box and its
//! filters, buying a listing, the auction boxes, the bid slider and the
//! chat-list bar (needs the `demo-capture` feature for gpui-kit's test
//! support). Nothing reaches Telegram: the app has no connection.
use super::{AUCTION_GIFT, ME, RESALE_GIFT};
use crate::ui::app::QuillApp;
use crate::ui::gift_market_state::MarketBox;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::gift_market::{
    AttributeId, AttributeKind, BID_SLIDER_STEPS, BuyStep, ResaleSort, auction_about_shown,
    set_auction_about_shown,
};

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    // The "About shown" pref stays in memory, never in the app's files.
    quill::gift_market::set_gift_market_persistence(false);
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

#[gpui_kit::test]
fn the_resale_box_filters_and_sorts_the_listings(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert!(app.gift_market.stack.is_empty());
        app.open_gift_resale(RESALE_GIFT, "Plush Pepe", cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Resale));
        let view = app.resale_box(cx);
        assert_eq!(view.title.as_ref(), "Plush Pepe");
        let filter = |app: &QuillApp| {
            app.session()
                .and_then(|s| s.payments.gift_market.resale.as_ref())
                .map(|search| search.filter.clone())
                .unwrap_or_default()
        };
        // The demo picked two models: the pill reads "2 Models".
        let picked = filter(app).selected(AttributeKind::Model);
        assert_eq!(AttributeKind::Model.tab_label(picked), "2 Models");
        app.edit_resale_filter(|f| f.sort = ResaleSort::Date, cx);
        assert_eq!(filter(app).sort, ResaleSort::Date);
        assert_eq!(filter(app).sort.label(), "Date");
        app.edit_resale_filter(|f| f.select_all(AttributeKind::Model), cx);
        assert_eq!(filter(app).selected(AttributeKind::Model), 0);
        assert!(filter(app).is_checked(AttributeId::Model(33)));
        app.edit_resale_filter(|f| f.stars_only = true, cx);
        assert!(filter(app).stars_only);
        // Back closes the box and forgets the list.
        app.market_back(cx);
        assert!(app.gift_market.stack.is_empty());
        assert!(
            app.session()
                .is_some_and(|s| s.payments.gift_market.resale.is_none())
        );
    });
}

#[gpui_kit::test]
fn a_listing_opens_the_buy_steps(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-resale");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        // The demo opens the resale box.
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Resale));
        let name = app
            .session()
            .and_then(|s| s.payments.gift_market.resale.as_ref())
            .and_then(|search| search.listings.first())
            .map(|listing| listing.gift.name.clone())
            .unwrap_or_default();
        app.pick_resale_listing(name, cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Buy));
        let view = app.resale_buy_box(cx);
        assert_eq!(view.title.as_ref(), "Plush Pepe #1,204");
        app.set_buy_step(BuyStep::Confirm, cx);
        let view = app.resale_buy_box(cx);
        assert_eq!(view.title.as_ref(), "Buy Plush Pepe #1,204");
        // Back from the buy box returns to the listings.
        app.market_back(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Resale));
        assert!(
            app.session()
                .is_some_and(|s| s.payments.gift_market.buy.is_none())
        );
    });
}

#[gpui_kit::test]
fn the_bid_slider_sets_the_button(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    let slider = app.update_in(vcx, |app, _, cx| {
        app.gift_market.stack = vec![MarketBox::AuctionBid {
            gift_id: AUCTION_GIFT,
        }];
        let _ = app.auction_bid_box(AUCTION_GIFT, cx);
        let slider = app.gift_market.slider.as_ref().expect("the slider");
        // It starts on the user's bid: nothing to raise.
        assert_eq!(slider.chosen, 4_000);
        slider.state.clone()
    });
    // Dragging to the end of the track raises to its maximum.
    slider.update(vcx, |_, cx| {
        cx.emit(SliderEvent::Change(SliderValue::Single(BID_SLIDER_STEPS)));
    });
    app.update_in(vcx, |app, window, cx| {
        let chosen = app.gift_market.slider.as_ref().map_or(0, |ui| ui.chosen);
        assert_eq!(chosen, 30_000);
        let state = app
            .session()
            .and_then(|s| s.payments.gift_market.auctions.get(&AUCTION_GIFT))
            .cloned()
            .expect("the auction");
        assert_eq!(
            state.bid_action(chosen).label(state.my_stars()),
            "Add ★26,000 to Your Bid"
        );
        // The minimum-bid block: one step above the current bid.
        app.set_bid_amount(AUCTION_GIFT, state.min_bid_for_me(), window, cx);
        let chosen = app.gift_market.slider.as_ref().map_or(0, |ui| ui.chosen);
        assert_eq!(chosen, 4_050);
        assert_eq!(
            state.bid_action(chosen).label(state.my_stars()),
            "Add ★50 to Your Bid"
        );
        // Leaving the bid box drops the slider.
        app.market_back(cx);
        assert!(app.gift_market.slider.is_none());
    });
}

#[gpui_kit::test]
fn the_about_box_leads_to_the_info_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    set_auction_about_shown(false);
    app.update_in(vcx, |app, _, cx| {
        // No bid for this recipient yet and the about box unseen: About.
        if let Some(market) = app.gift_market_mut()
            && let Some(state) = market.auctions.get_mut(&AUCTION_GIFT)
            && let quill::gift_market::AuctionPhase::Active(active) = &mut state.phase
        {
            active.my = None;
        }
        app.show_auction_box(AUCTION_GIFT, cx);
        assert_eq!(
            app.gift_market.top(),
            Some(&MarketBox::AuctionAbout {
                gift_id: AUCTION_GIFT
            })
        );
        app.understood_auction(AUCTION_GIFT, cx);
        assert!(auction_about_shown());
        assert_eq!(
            app.gift_market.top(),
            Some(&MarketBox::AuctionInfo {
                gift_id: AUCTION_GIFT
            })
        );
        let view = app.auction_info_box(AUCTION_GIFT, cx);
        assert_eq!(view.title.as_ref(), "Toy Bear");
    });
    set_auction_about_shown(false);
}

#[gpui_kit::test]
fn the_chat_list_bar_shows_while_a_bid_runs(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(app.session().and_then(|s| s.my_user_id), Some(ME));
        assert!(app.gift_auction_bar(cx).is_some());
        assert!(app.suggestion_card(cx).is_some());
        // A tap on the bar opens the bid box of its auction.
        let list = app
            .session()
            .map(|s| {
                s.payments
                    .gift_market
                    .active_auctions(quill::local_time::now_unix())
            })
            .unwrap_or_default();
        assert_eq!(list.len(), 1);
        if let Some(market) = app.gift_market_mut() {
            market.active.clear();
        }
        assert!(app.gift_auction_bar(cx).is_none());
    });
}
