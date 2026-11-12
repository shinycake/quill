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
use gpui_kit::test::TestWindowExt;
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
    app.update_in(vcx, |app, _window, cx| {
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

/// Round 17 C finding 8: Escape on the Buy box while the purchase is in
/// flight keeps the purchase, so its answer still lands (toast, listing
/// out, craft slot); a box closed with nothing sent drops it.
#[gpui_kit::test]
fn closing_the_buy_box_mid_purchase_keeps_the_purchase(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-resale");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _window, cx| {
        let name = app
            .session()
            .and_then(|s| s.payments.gift_market.resale.as_ref())
            .and_then(|search| search.listings.first())
            .map(|listing| listing.gift.name.clone())
            .unwrap_or_default();
        app.pick_resale_listing(name, cx);
        if let Some(buy) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.payments.gift_market.buy.as_mut())
        {
            buy.begin_send(41);
        }
        app.market_back(cx);
        assert_eq!(app.gift_market.top(), Some(&MarketBox::Resale));
        assert!(
            app.session()
                .and_then(|s| s.payments.gift_market.buy.as_ref())
                .is_some_and(|buy| buy.waits_for(quill::ids::RequestId(41))),
            "the purchase in flight stays for its answer"
        );
    });
}

/// Round 17 C finding 16: "Hide My Name" is cleared once the market's
/// boxes are all closed, so the next auction does not bid anonymously.
#[gpui_kit::test]
fn hide_my_name_does_not_carry_to_the_next_auction(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.gift_market.stack = vec![MarketBox::bid(AUCTION_GIFT)];
        app.gift_market.flow.set_hide_name(true);
        app.market_back(cx);
        assert!(app.gift_market.stack.is_empty());
        assert!(!app.gift_market.flow.hide_name());
    });
}

/// Another user's bid moves the slider's bounds while "Custom Amount" is
/// open: the rebuilt slider keeps the field, its text and its error.
#[gpui_kit::test]
fn the_custom_amount_survives_a_slider_rebuild(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        app.gift_market.stack = vec![MarketBox::bid(AUCTION_GIFT)];
        let _ = app.auction_bid_box(AUCTION_GIFT, cx);
        app.open_custom_bid(AUCTION_GIFT, window, cx);
        let before = app
            .gift_market
            .bid_mut(AUCTION_GIFT)
            .and_then(|ui| {
                ui.bounds = (0, 0);
                ui.custom.as_mut()
            })
            .map(|custom| {
                custom.error = Some("too low".into());
                custom.input.clone()
            })
            .expect("the field is open");
        let _ = app.auction_bid_box(AUCTION_GIFT, cx);
        let custom = app
            .gift_market
            .bid(AUCTION_GIFT)
            .and_then(|ui| ui.custom.as_ref())
            .expect("the field stayed");
        assert_eq!(custom.input, before);
        assert_eq!(custom.error.as_deref(), Some("too low"));
    });
}

#[gpui_kit::test]
fn the_bid_slider_sets_the_button(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    let slider = app.update_in(vcx, |app, _, cx| {
        app.gift_market.stack = vec![MarketBox::bid(AUCTION_GIFT)];
        let _ = app.auction_bid_box(AUCTION_GIFT, cx);
        let slider = app.gift_market.bid(AUCTION_GIFT).expect("the slider");
        // It starts on the user's bid: nothing to raise.
        assert_eq!(slider.chosen, 4_000);
        slider.state.clone()
    });
    // Dragging to the end of the track raises to its maximum.
    slider.update(vcx, |_, cx| {
        cx.emit(SliderEvent::Change(SliderValue::Single(BID_SLIDER_STEPS)));
    });
    app.update_in(vcx, |app, window, cx| {
        let chosen = app.gift_market.bid(AUCTION_GIFT).map_or(0, |ui| ui.chosen);
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
        let chosen = app.gift_market.bid(AUCTION_GIFT).map_or(0, |ui| ui.chosen);
        assert_eq!(chosen, 4_050);
        assert_eq!(
            state.bid_action(chosen).label(state.my_stars()),
            "Add ★50 to Your Bid"
        );
        // Leaving the bid box drops the slider.
        app.market_back(cx);
        assert!(app.gift_market.bid(AUCTION_GIFT).is_none());
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
    });
    // One tap on the bar's button opens the bid box once: the button
    // keeps the click from reaching the bar under it.
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.click("gift-auction-bar-button", cx);
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(app.gift_market.bar_opens, 1);
        assert!(matches!(
            app.gift_market.top(),
            Some(MarketBox::AuctionBid { gift_id, .. }) if *gift_id == AUCTION_GIFT
        ));
        if let Some(market) = app.gift_market_mut() {
            market.active.clear();
        }
        assert!(app.gift_auction_bar(cx).is_none());
    });
}

/// Sweep s4-payments-2, 10-013: two failures in one frame both toast (the
/// second waits in the queue behind the first).
#[gpui_kit::test]
fn two_failed_answers_in_one_frame_both_toast(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-resale");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        if let Some(market) = app.gift_market_mut() {
            market.events = vec![
                quill::gift_market::MarketEvent::Note("Purchase failed".into()),
                quill::gift_market::MarketEvent::Note("Craft failed".into()),
            ];
        }
        app.gift_market_frame_upkeep(window, cx);
        let toasts = &app.connection.toasts;
        assert_eq!(toasts.visible().map(|n| n.text()), Some("Purchase failed"));
        assert_eq!(toasts.waiting(), 1, "the second one is queued, not lost");
    });
}

/// Sweep s4-payments-2, 10-006: the bid slider, the custom field and "Hide
/// My Name" belong to the boxes that show them: replacing, popping or
/// clearing the last of those boxes drops them, so the next auction starts
/// clean.
#[gpui_kit::test]
fn a_replaced_or_closed_bid_box_takes_its_choices_with_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| {
        let market = &mut app.gift_market;
        market.stack.clear();
        market.flow.set_hide_name(true);
        market.push(MarketBox::bid(1));
        market.push(MarketBox::AuctionSend { gift_id: 1 });
        assert_eq!(market.pop(), Some(MarketBox::AuctionSend { gift_id: 1 }));
        assert!(market.flow.hide_name(), "the bid box below still uses it");
        // Replace the bid box by another box: nothing of it stays.
        assert_eq!(market.pop(), Some(MarketBox::bid(1)));
        assert!(!market.flow.hide_name(), "no box is left to own the choice");
        market.flow.set_hide_name(true);
        market.push(MarketBox::AuctionSend { gift_id: 2 });
        market.clear();
        assert!(!market.flow.hide_name());
    });
}

/// Sweep r2-27 (10-005, 10-007): clearing the boxes ends an auction that
/// was waiting to open and forgets the recipient, whichever way they
/// closed.
#[gpui_kit::test]
fn clearing_the_boxes_ends_a_pending_open_and_forgets_the_recipient(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, _| {
        let market = &mut app.gift_market;
        market.stack = vec![MarketBox::ActiveAuctions];
        market
            .flow
            .set_recipient(Some(quill::gift_manage::GiftRecipient {
                sender: quill::telegram::envelope::MessageSender::User { user_id: 5 },
                name: "Maya".into(),
                is_self: false,
                is_channel: false,
            }));
        market.flow.begin_opening(quill::ids::RequestId(9));
        assert_eq!(
            market.flow.opening(),
            Some((quill::ids::RequestId(9), true))
        );
        market.clear();
        assert_eq!(market.flow.opening(), None);
        assert!(market.flow.recipient().is_none());
    });
}

/// Sweep r2-27 (10-005): an auction state that answers another ask does
/// not open this ask's box, and is left for its own asker.
#[gpui_kit::test]
fn another_asks_auction_answer_does_not_open_this_asks_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        app.gift_market.stack = vec![MarketBox::ActiveAuctions];
        app.gift_market.flow.begin_opening(quill::ids::RequestId(7));
        let other = quill::gift_market::AuctionFetch::Ready {
            gift_id: AUCTION_GIFT,
            request: quill::ids::RequestId(8),
        };
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.gift_market.auction = other;
        }
        app.gift_market_frame_upkeep(window, cx);
        assert_eq!(app.gift_market.flow.opening(), None, "the ask is over");
        assert_eq!(app.gift_market.stack, vec![MarketBox::ActiveAuctions]);
        let left = app
            .demo_session
            .as_ref()
            .map(|session| session.payments.gift_market.auction);
        assert_eq!(left, Some(other), "not this ask's to take");
    });
}

/// Sweep r2-27 (10-008): a bid box reads its own slider, not the topmost
/// bid box's.
#[gpui_kit::test]
fn a_bid_box_reads_its_own_slider(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-gift-auction");
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.gift_market.stack = vec![MarketBox::bid(AUCTION_GIFT)];
        let _ = app.auction_bid_box(AUCTION_GIFT, cx);
        assert!(app.gift_market.bid(AUCTION_GIFT).is_some());
        assert!(app.gift_market.bid(AUCTION_GIFT + 1).is_none());
        assert!(app.gift_market.bid_mut(AUCTION_GIFT + 1).is_none());
    });
}
