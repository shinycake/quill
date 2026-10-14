use super::*;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

const NOW: i64 = 1_000_000;

fn backdrop(id: i32, name: &str) -> Value {
    json!({"@type": "upgradedGiftBackdrop", "id": id, "name": name,
        "colors": {"center_color": 0x112233, "edge_color": 0x445566, "symbol_color": 0, "text_color": 0xFFFFFF},
        "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": 15}})
}

fn sticker(id: i64) -> Value {
    json!({"@type": "sticker", "id": id.to_string(), "set_id": "1", "width": 512, "height": 512,
        "emoji": "🎁", "format": {"@type": "stickerFormatTgs"},
        "sticker": {"@type": "file", "id": id as i32 + 100, "size": 10, "expected_size": 10,
            "local": {"path": "", "is_downloading_completed": false},
            "remote": {"id": "r", "unique_id": "u"}}})
}

fn attribute(kind: &str, name: &str, sticker_id: i64) -> Value {
    json!({"@type": kind, "name": name, "sticker": sticker(sticker_id),
        "rarity": {"@type": "upgradedGiftAttributeRarityRare"}})
}

fn collectible(number: i32, backdrop_id: i32, symbol: i64, craft: i32) -> Value {
    json!({"@type": "upgradedGift", "id": (9000 + number).to_string(), "regular_gift_id": "55",
        "title": "Plush Pepe", "name": format!("PlushPepe-{number}"), "number": number,
        "model": attribute("upgradedGiftModel", "Gold", 7),
        "symbol": attribute("upgradedGiftSymbol", &format!("Sym{symbol}"), symbol),
        "backdrop": backdrop(backdrop_id, &format!("Back{backdrop_id}")),
        "owner_id": {"@type": "messageSenderUser", "user_id": 42},
        "gift_address": "",
        "resale_parameters": {"@type": "giftResaleParameters", "star_count": 1500, "gram_cent_count": 2500, "gram_only": false},
        "craft_probability_per_mille": craft, "is_crafted": false, "is_burned": false})
}

#[test]
fn collectible_parses_attributes_rarity_and_listing() {
    let gift =
        crate::premium_hub::upgraded::parse_collectible(&collectible(12, 3, 70, 125)).unwrap();
    assert_eq!(gift.full_name(), "Plush Pepe #12");
    assert_eq!(gift.regular_gift_id, 55);
    assert_eq!(gift.backdrop.id, 3);
    assert_eq!(gift.backdrop.center, 0x112233);
    assert_eq!(gift.backdrop.rarity.label(), "1.5%");
    assert_eq!(gift.model.rarity.label(), "rare");
    assert!(gift.model.rarity.is_special());
    assert_eq!(gift.symbol.sticker_id, 70);
    assert_eq!(gift.craft_per_mille, 125);
    let resale = gift.resale.unwrap();
    assert_eq!(
        (resale.stars, resale.gram_cents, resale.gram_only),
        (1500, 2500, false)
    );
    assert_eq!(
        crate::premium_hub::upgraded::Rarity::PerMille(0).label(),
        "< 0.1%"
    );
    assert_eq!(
        crate::premium_hub::upgraded::Rarity::PerMille(20).label(),
        "2%"
    );
}

#[test]
fn resale_filter_json_tab_labels_and_toggles() {
    let mut filter = ResaleFilter::default();
    assert_eq!(AttributeKind::Model.tab_label(0), "Model");
    assert!(filter.is_checked(AttributeId::Model(7)));
    filter.toggle(AttributeId::Model(7));
    filter.toggle(AttributeId::Model(8));
    filter.toggle(AttributeId::Backdrop(3));
    assert_eq!(
        AttributeKind::Model.tab_label(filter.selected(AttributeKind::Model)),
        "2 Models"
    );
    assert_eq!(
        AttributeKind::Backdrop.tab_label(filter.selected(AttributeKind::Backdrop)),
        "1 Backdrop"
    );
    assert_eq!(AttributeKind::Symbol.tab_label(0), "Symbol");
    assert!(!filter.is_checked(AttributeId::Model(9)));
    assert!(filter.is_checked(AttributeId::Symbol(1)));
    filter.toggle(AttributeId::Model(8));
    assert_eq!(filter.selected(AttributeKind::Model), 1);
    filter.select_all(AttributeKind::Model);
    assert_eq!(filter.selected(AttributeKind::Model), 0);
    filter.sort = ResaleSort::Number;
    filter.stars_only = true;
    let json: Value = serde_json::from_str(&crate::telegram::requests::search_gifts_for_resale(
        crate::ids::RequestId(4),
        55,
        &filter,
        false,
        "",
    ))
    .unwrap();
    assert_eq!(json["@type"], "searchGiftsForResale");
    assert_eq!(json["gift_id"], "55");
    assert_eq!(json["order"]["@type"], "giftForResaleOrderNumber");
    assert_eq!(json["for_stars"], true);
    assert_eq!(
        json["attributes"][0]["@type"],
        "upgradedGiftAttributeIdBackdrop"
    );
    assert_eq!(json["attributes"][0]["backdrop_id"], 3);
    assert_eq!(
        AttributeId::Model(7).to_json()["sticker_id"],
        Value::String("7".into())
    );
    assert_eq!(
        ResaleSort::Date.to_json()["@type"],
        "giftForResaleOrderPriceChangeDate"
    );
    assert_eq!(resale_count_label(0), "No listings");
    assert_eq!(resale_count_label(1), "1 gift in resale");
    assert_eq!(resale_count_label(1234), "1,234 gifts in resale");
    assert_eq!(resale_tile_price(900, 1), "900");
    assert_eq!(resale_tile_price(1500, 3), "1,500+");
}

#[test]
fn resale_page_parses_and_sorts_counts() {
    let value = json!({"@type": "giftsForResale", "total_count": 2,
        "gifts": [{"@type": "giftForResale", "gift": collectible(1, 3, 70, 0), "received_gift_id": ""},
                  {"@type": "giftForResale", "gift": collectible(2, 4, 71, 0), "received_gift_id": "mine"}],
        "models": [{"model": attribute("upgradedGiftModel", "A", 7), "total_count": 1},
                   {"model": attribute("upgradedGiftModel", "B", 8), "total_count": 5}],
        "symbols": [], "backdrops": [{"backdrop": backdrop(3, "Onyx"), "total_count": 2}],
        "next_offset": "50"});
    let Some(GiftMarketPayload::ResalePage(page)) = parse_payload("giftsForResale", &value) else {
        panic!("not parsed");
    };
    assert_eq!(page.total_count, 2);
    assert_eq!(page.listings.len(), 2);
    assert!(page.listings[1].is_mine());
    assert_eq!(page.attributes.models[0].item.name, "B");
    assert_eq!(page.attributes.backdrops[0].item.name, "Onyx");
    let mut search = ResaleSearch::new(55, ResaleFilter::default(), true);
    search.loading = true;
    search.apply(page.clone(), Some(42), false);
    // Craft mode drops the user's own listings (owner 42 here).
    assert!(search.listings.is_empty());
    assert!(search.has_more());
    let mut search = ResaleSearch::new(55, ResaleFilter::default(), false);
    search.apply(page.clone(), Some(42), false);
    search.apply(page, Some(42), true);
    assert_eq!(search.listings.len(), 4);
}

fn auction(levels: Value, my: Value, top: Value) -> AuctionState {
    let value = json!({"@type": "giftAuctionState",
        "gift": {"@type": "gift", "id": "77", "star_count": 0,
            "auction_info": {"@type": "giftAuction", "id": "pepe", "gifts_per_round": 2, "start_date": NOW - 100},
            "overall_limits": {"@type": "giftPurchaseLimits", "total_count": 100, "remaining_count": 40}},
        "state": {"@type": "auctionStateActive", "start_date": NOW - 100, "end_date": NOW + 7200,
            "min_bid": 100, "bid_levels": levels, "top_bidder_user_ids": top,
            "rounds": [{"@type": "auctionRound", "number": 1, "duration": 600, "extend_time": 0, "top_winner_count": 0},
                       {"@type": "auctionRound", "number": 2, "duration": 300, "extend_time": 30, "top_winner_count": 3}],
            "current_round_end_date": NOW + 65, "current_round_number": 1, "total_round_count": 5,
            "distributed_item_count": 60, "left_item_count": 40, "acquired_item_count": 1,
            "user_bid": my}});
    AuctionState::parse(&value).unwrap()
}

fn levels() -> Value {
    json!([{"star_count": 5000, "bid_date": 10, "position": 1},
           {"star_count": 3000, "bid_date": 20, "position": 2},
           {"star_count": 1000, "bid_date": 30, "position": 3}])
}

#[test]
fn auction_parses_and_places_the_user() {
    let no_bid = auction(levels(), Value::Null, json!([1, 2, 3]));
    assert_eq!(no_bid.gift.auction.as_ref().unwrap().id, "pepe");
    assert_eq!(no_bid.per_round(), 2);
    assert_eq!(no_bid.total(), 100);
    assert_eq!(no_bid.min_bid_for_me(), 100);
    assert_eq!(no_bid.my_position(), 4);
    let my = json!({"star_count": 3000, "bid_date": 25, "next_bid_star_count": 3001,
        "owner_id": {"@type": "messageSenderUser", "user_id": 42}, "was_returned": false});
    let bidding = auction(levels(), my, json!([1, 42, 3]));
    // Tied at 3000 but later than the level's bid: behind it.
    assert_eq!(bidding.my_position(), 3);
    assert!(!bidding.is_winning());
    assert_eq!(bidding.min_bid_for_me(), 3001);
    let rows = bidding.bid_rows(3000, Some(42));
    assert_eq!(rows.kind, BidKind::Losing);
    assert_eq!(rows.position, 3);
    assert_eq!(
        rows.top
            .iter()
            .map(|t| (t.user_id, t.is_me))
            .collect::<Vec<_>>(),
        vec![(1, false), (42, true), (3, false)]
    );
    let raise = bidding.bid_rows(4000, Some(42));
    assert_eq!((raise.kind, raise.position), (BidKind::Setting, 2));
    assert_eq!(BidKind::Losing.title(true), "You've been outbid");
    assert_eq!(BidKind::Winning.title(false), "Your bid will be");
    assert_eq!(bidding.bid_action(3000), BidAction::Close);
    assert_eq!(bidding.bid_action(3500).label(3000), "Add ★500 to Your Bid");
    assert_eq!(no_bid.bid_action(1200).label(0), "Place a ★1,200 Bid");
    assert_eq!(BidAction::Close.label(0), "OK");
    assert_eq!(bidding.bubble_subtext(3000), "your bid");
    assert_eq!(bidding.bubble_subtext(3250), "+250");
}

#[test]
fn bid_slider_follows_tdesktop_bounds() {
    let small = auction(levels(), Value::Null, json!([]));
    assert_eq!(
        small.slider(),
        BidSlider {
            min: 100,
            mine: 0,
            max: 30_000
        }
    );
    let high = auction(
        json!([{"star_count": 40_000, "bid_date": 1, "position": 1}]),
        json!({"star_count": 30_000, "bid_date": 2, "next_bid_star_count": 30_001, "was_returned": false}),
        json!([]),
    );
    // mine × 1.2 = 36,000, below 40,000 × 1.05: the top bid × 1.2 wins.
    assert_eq!(
        high.slider(),
        BidSlider {
            min: 30_001,
            mine: 30_000,
            max: 48_000
        }
    );
    assert_eq!(high.slider().initial(), 30_000);
    assert_eq!(high.bubble_subtext(48_000), "click to bid more");
}

#[test]
fn bid_slider_steps_round_trip() {
    let slider = BidSlider {
        min: 100,
        mine: 0,
        max: 30_000,
    };
    assert_eq!(slider.amount_at(0.), 100);
    assert_eq!(slider.amount_at(BID_SLIDER_STEPS), 30_000);
    assert_eq!(slider.amount_at(-5.), 100);
    // The low end gets most of the track.
    assert!(slider.amount_at(500.) < 4_000);
    for amount in [100, 250, 1_200, 9_999, 30_000] {
        let back = slider.amount_at(slider.step_of(amount));
        assert!(
            (back - amount).abs() <= amount / 50 + 2,
            "{amount} -> {back}"
        );
    }
    let flat = BidSlider {
        min: 5,
        mine: 0,
        max: 5,
    };
    assert_eq!(flat.step_of(5), 0.);
    assert_eq!(flat.amount_at(700.), 5);
}

#[test]
fn auction_box_choice_matches_tdesktop() {
    let me = MessageSender::User { user_id: 42 };
    let other = MessageSender::User { user_id: 9 };
    let no_bid = auction(levels(), Value::Null, json!([]));
    assert_eq!(no_bid.choose_box(&me, false, NOW), AuctionBoxChoice::About);
    assert_eq!(no_bid.choose_box(&me, true, NOW), AuctionBoxChoice::Info);
    let my = json!({"star_count": 3000, "bid_date": 25, "next_bid_star_count": 3001,
        "owner_id": {"@type": "messageSenderUser", "user_id": 42}, "was_returned": false});
    let bid = auction(levels(), my, json!([]));
    assert_eq!(bid.choose_box(&me, false, NOW), AuctionBoxChoice::Bid);
    assert_eq!(
        bid.choose_box(&other, true, NOW),
        AuctionBoxChoice::ChangeRecipient
    );
    assert_eq!(
        bid.choose_box(&me, true, NOW + 10_000),
        AuctionBoxChoice::Info
    );
}

#[test]
fn auction_texts() {
    assert_eq!(nice_countdown(65), "1:05");
    assert_eq!(nice_countdown(3_725), "1:02:05");
    assert_eq!(nice_countdown(-4), "0:00");
    assert_eq!(short_stars(999_999), "999,999");
    assert_eq!(short_stars(1_500_000), "1.5M");
    assert_eq!(short_stars(12_000_000), "12M");
    let state = auction(levels(), Value::Null, json!([]));
    assert_eq!(
        join_button(&state, NOW, false),
        ("Join Auction".into(), "2 h 0 m left".into())
    );
    assert_eq!(join_button(&state, NOW + 7_000, true).1, "4 m left");
    assert_eq!(join_button(&state, NOW + 8_000, false).0, "OK");
    assert_eq!(
        join_button(&state, NOW - 200, false),
        ("Place an Early Bid".into(), "starts in 1:40".into())
    );
    let rows = info_rows(&state, NOW, |d| d.to_string());
    let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["Started", "Ends", "Current Round", "Availability"]);
    assert_eq!(rows[2].1, "1 of 5");
    assert_eq!(rows[3].1, "40 of 100 left");
    let preview = info_rows(&state, NOW - 200, |d| d.to_string());
    let labels: Vec<&str> = preview.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Starts",
            "Ends",
            "Quantity",
            "Rounds",
            "Round 1",
            "Rounds 2-5"
        ]
    );
    assert_eq!(preview[4].1, "10 minutes");
    assert_eq!(
        preview[5].1,
        "5 minutes each + 30 seconds for late bids in top 3"
    );
    assert_eq!(bid_subtitle(2), "Top 2 bidders will win");
    assert_eq!(
        send_about(2),
        "2 gifts are dropped to the top 2 bidders by bid amount."
    );
    assert_eq!(about_features(1, 3)[0].0, "Top 1 Bidder");
    assert_eq!(
        about_features(5, 3)[0].1,
        "5 gifts are dropped in 3 rounds to the top 5 bidders by bid amount."
    );
    assert_eq!(auction_tile(NOW as i32 + 5, false, NOW), ("soon", "View"));
    assert_eq!(
        auction_tile(NOW as i32 - 5, false, NOW),
        ("auction", "Join")
    );
    assert_eq!(auction_tile(NOW as i32 - 5, true, NOW), ("auction", "View"));
    assert_eq!(preview_button(0, NOW as i32 - 1, NOW), "View Results");
    assert_eq!(preview_button(NOW as i32 + 9, NOW as i32 + 99, NOW), "View");
    assert_eq!(preview_button(0, NOW as i32 + 99, NOW), "Join");
    assert_eq!(bought_link(1), "1 item bought");
    assert_eq!(bought_title(3), "3 Items Bought");
    assert_eq!(variants_link(12), "View 12 Variants");
    assert_eq!(
        change_recipient_text(Some("Ann"), None),
        "You've already placed a bid on this gift for Ann. Do you want to raise your bid and change the recipient to yourself?"
    );
}

#[test]
fn auction_bar_one_and_many() {
    let winning = auction(
        levels(),
        json!({"star_count": 6000, "bid_date": 25, "next_bid_star_count": 6001, "was_returned": false}),
        json!([]),
    );
    let outbid = auction(
        levels(),
        json!({"star_count": 500, "bid_date": 25, "next_bid_star_count": 501, "was_returned": false}),
        json!([]),
    );
    let one = auction_bar(std::slice::from_ref(&winning), NOW).unwrap();
    assert_eq!(one.title, "Active Auction");
    assert_eq!(one.text, "You're winning (1st place).");
    assert!(!one.outbid);
    assert_eq!(one.button, "1:05");
    let lost = auction_bar(std::slice::from_ref(&outbid), NOW).unwrap();
    assert_eq!(
        (lost.text.as_str(), lost.outbid),
        ("You've been outbid.", true)
    );
    let many = auction_bar(&[winning.clone(), outbid.clone()], NOW).unwrap();
    assert_eq!(many.title, "2 Active Auctions");
    assert_eq!(many.text, "You've been outbid in 1 of them.");
    assert_eq!(many.button, "View");
    let all = auction_bar(&[outbid.clone(), outbid.clone()], NOW).unwrap();
    assert_eq!(all.text, "You've been outbid in all of them.");
    let fine = auction_bar(&[winning.clone(), winning.clone()], NOW).unwrap();
    assert_eq!(fine.text, "You're winning in all of them.");
    assert!(auction_bar(&[], NOW).is_none());
    assert_eq!(
        active_row(&winning),
        (
            "Round 1 of 5".into(),
            "Your bid ★6,000 is ranked #1".into(),
            false
        )
    );
    assert!(active_row(&outbid).1.ends_with("is outbid"));
}

#[test]
fn auction_updates_and_answers_parse() {
    let state = json!({"@type": "giftAuctionState",
        "gift": {"@type": "gift", "id": "77", "auction_info": {"@type": "giftAuction", "id": "pepe", "gifts_per_round": 1, "start_date": 1}},
        "state": {"@type": "auctionStateFinished", "start_date": 1, "end_date": 2, "average_price": 900,
            "acquired_item_count": 2, "telegram_listed_item_count": 0, "fragment_listed_item_count": 0, "fragment_url": ""}});
    let update = json!({"@type": "updateGiftAuctionState", "state": state});
    let Some(GiftMarketPayload::AuctionUpdated(parsed)) =
        parse_payload("updateGiftAuctionState", &update)
    else {
        panic!("not parsed");
    };
    assert!(parsed.finished(NOW));
    assert_eq!(parsed.acquired(), 2);
    let rows = info_rows(&parsed, NOW, |d| d.to_string());
    assert_eq!(rows[0].0, "First Sale");
    assert_eq!(rows[2], ("Average Price".into(), "★900".into()));
    let active = json!({"@type": "updateActiveGiftAuctions", "states": [state]});
    let Some(GiftMarketPayload::ActiveAuctions(list)) =
        parse_payload("updateActiveGiftAuctions", &active)
    else {
        panic!("not parsed");
    };
    assert_eq!(list.len(), 1);
    let acquired = json!({"@type": "giftAuctionAcquiredGifts", "gifts": [
        {"receiver_id": {"@type": "messageSenderUser", "user_id": 5}, "date": 9, "star_count": 300,
         "auction_round_number": 2, "auction_round_position": 1, "unique_gift_number": 17,
         "text": {"text": "hi"}, "is_private": false},
        {"receiver_id": {"@type": "messageSenderUser", "user_id": 5}, "date": 8, "star_count": 200,
         "auction_round_number": 1, "auction_round_position": 2, "unique_gift_number": 0,
         "text": {"text": ""}, "is_private": false}]});
    let Some(GiftMarketPayload::AcquiredGifts(gifts)) =
        parse_payload("giftAuctionAcquiredGifts", &acquired)
    else {
        panic!("not parsed");
    };
    assert_eq!(
        gifts.iter().map(|g| g.round).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(gifts[1].text, "hi");
    let preview = json!({"@type": "giftUpgradePreview",
        "models": [attribute("upgradedGiftModel", "A", 7), attribute("upgradedGiftModel", "B", 8)],
        "symbols": [attribute("upgradedGiftSymbol", "S", 9)], "backdrops": [backdrop(1, "Onyx")],
        "prices": [], "next_prices": []});
    let Some(GiftMarketPayload::UpgradePreview(preview)) =
        parse_payload("giftUpgradePreview", &preview)
    else {
        panic!("not parsed");
    };
    assert_eq!(preview.cover(0).map(|c| c.0.name.as_str()), Some("A"));
    assert_eq!(preview.cover(1).map(|c| c.0.name.as_str()), Some("B"));
    assert!(UpgradePreview::default().cover(0).is_none());
}

fn craft_gift(number: i32, backdrop_id: i32, symbol: i64, craft: i32) -> CraftGift {
    CraftGift::parse_received(&json!({"@type": "receivedGift", "received_gift_id": format!("r{number}"),
        "gift": {"@type": "sentGiftUpgraded", "gift": collectible(number, backdrop_id, symbol, craft)},
        "craft_date": 0}))
    .unwrap()
}

#[test]
fn craft_chances_follow_the_table() {
    let slots = vec![
        craft_gift(1, 3, 70, 100),
        craft_gift(2, 3, 71, 150),
        craft_gift(3, 4, 70, 125),
    ];
    assert_eq!(success_per_mille(&slots), 375);
    assert_eq!(format_percent(375), "38%");
    assert_eq!(format_percent(374), "37%");
    let table = vec![vec![1000], vec![400, 1000], vec![250, 600, 1000]];
    let chances = attribute_chances(&slots, &table);
    let summary: Vec<(String, i32)> = chances
        .iter()
        .map(|c| (c.name().to_string(), c.per_mille))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Back3".to_string(), 600),
            ("Back4".to_string(), 250),
            ("Sym70".to_string(), 600),
            ("Sym71".to_string(), 250)
        ]
    );
    assert_eq!(
        chances[0].tooltip(),
        "60% chance the crafted gift will have Back3 background."
    );
    // A short table falls back to 1000.
    assert_eq!(attribute_chances(&slots, &[])[0].per_mille, 1000);
    assert!(shows_craftable_link(&chances));
    assert_eq!(craft_colors(4).0, 0x1C4843);
    assert_eq!(craft_colors(2).0, 0x2C4359);
    assert_eq!(
        craft_failed_text(1),
        "This crafting attempt was unsuccessful.\n1 gift was lost."
    );
    assert_eq!(suitable_on_sale(3), "3 suitable gifts on sale");
}

#[test]
fn craft_session_slots_and_gates() {
    let mut session = CraftSession::new(craft_gift(1, 3, 70, 100));
    assert!(session.add(craft_gift(2, 3, 70, 100)));
    assert!(!session.add(craft_gift(2, 3, 70, 100)));
    assert!(session.add(craft_gift(3, 3, 70, 100)));
    assert!(session.add(craft_gift(4, 3, 70, 100)));
    assert!(!session.add(craft_gift(5, 3, 70, 100)));
    assert!(!session.remove(0));
    assert!(session.remove(1));
    assert_eq!(session.slots.len(), 3);
    assert_eq!(session.success_per_mille(), 300);
    let mut later = craft_gift(6, 3, 70, 100);
    later.craft_date = NOW as i32 + 60;
    assert_eq!(later.gate(false, NOW), CraftGate::Later(NOW as i32 + 60));
    assert_eq!(
        later
            .gate(false, NOW)
            .message(|_| ("Jan 1".into(), "10:00".into())),
        Some("This gift will become available for crafting on Jan 1 at 10:00.".into())
    );
    let mut exported = craft_gift(7, 3, 70, 100);
    exported.gift.gift_address = "EQ...".into();
    assert_eq!(exported.gate(true, NOW), CraftGate::Exported);
    assert_eq!(exported.gate(false, NOW), CraftGate::Ready);
    assert_eq!(
        craft_gift(8, 3, 70, 0).gate(false, NOW),
        CraftGate::Unavailable
    );
}

#[test]
fn craft_answers_parse() {
    let value = json!({"@type": "giftsForCrafting", "total_count": 1,
        "gifts": [{"@type": "receivedGift", "received_gift_id": "r1",
            "gift": {"@type": "sentGiftUpgraded", "gift": collectible(1, 3, 70, 100)}, "craft_date": 5}],
        "attribute_persistence_probabilities": [{"persistence_chance_per_mille": [1000]},
            {"persistence_chance_per_mille": [400, 1000]}],
        "next_offset": ""});
    let Some(GiftMarketPayload::CraftCandidates(list)) = parse_payload("giftsForCrafting", &value)
    else {
        panic!("not parsed");
    };
    assert_eq!(list.gifts[0].craft_date, 5);
    assert_eq!(list.probabilities, vec![vec![1000], vec![400, 1000]]);
    let success = json!({"@type": "craftGiftResultSuccess", "gift": collectible(9, 3, 70, 0), "received_gift_id": "new"});
    assert!(matches!(
        parse_payload("craftGiftResultSuccess", &success),
        Some(GiftMarketPayload::CraftResult(CraftResult::Success { .. }))
    ));
    assert_eq!(
        parse_payload(
            "craftGiftResultTooEarly",
            &json!({"@type": "craftGiftResultTooEarly", "retry_after": 30})
        ),
        Some(GiftMarketPayload::CraftResult(CraftResult::TooEarly(30)))
    );
    assert_eq!(
        parse_payload(
            "craftGiftResultFail",
            &json!({"@type": "craftGiftResultFail"})
        ),
        Some(GiftMarketPayload::CraftResult(CraftResult::Fail))
    );
    assert!(parse_payload("ok", &json!({})).is_none());
}

#[test]
fn error_notes_name_the_action() {
    assert_eq!(
        error_note(GiftMarketPurpose::PlaceBid { gift_id: 1 }, "error 400"),
        "Couldn't place the bid: error 400."
    );
}
