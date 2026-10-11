//! Parsing and gating tests for received-gift management and sending.
use crate::gift_manage::{
    GiftLimits, GiftManagePayload, PinGate, ResaleGate, SendGate, TransferGate, parse_payload,
    parse_stars_input, pinned_ids, pins_with, pins_without, valid_collection_name,
};
use crate::premium_hub::{ReceivedGift, parse_received_gift};
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

const NOW: i64 = 1_000_000;

fn regular(extra: Value) -> ReceivedGift {
    let mut v = json!({
        "@type": "receivedGift", "received_gift_id": "r1", "date": 5,
        "is_saved": true, "can_be_upgraded": true, "sell_star_count": 40,
        "prepaid_upgrade_star_count": 0,
        "gift": {"@type": "sentGiftRegular", "gift": {
            "@type": "gift", "id": "9", "star_count": 100, "upgrade_star_count": 30}},
    });
    merge(&mut v, extra);
    parse_received_gift(&v).unwrap()
}

fn unique(extra: Value) -> ReceivedGift {
    let mut v = json!({
        "@type": "receivedGift", "received_gift_id": "u1", "date": 5,
        "is_saved": true, "can_be_transferred": true, "transfer_star_count": 25,
        "next_transfer_date": 0, "next_resale_date": 0,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "number": 4}},
    });
    merge(&mut v, extra);
    parse_received_gift(&v).unwrap()
}

fn merge(base: &mut Value, extra: Value) {
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
}

#[test]
fn received_gift_keeps_the_owner_action_fields() {
    let gift = unique(json!({
        "collection_ids": [3, 5], "next_transfer_date": 77,
        "gift": {"@type": "sentGiftUpgraded", "gift": {
            "@type": "upgradedGift", "title": "Plush Pepe", "number": 4,
            "resale_parameters": {"@type": "giftResaleParameters", "star_count": 900, "gram_only": false}}}
    }));
    assert_eq!(gift.manage.collection_ids, vec![3, 5]);
    assert!(gift.in_collection(5) && !gift.in_collection(4));
    assert_eq!(gift.manage.transfer_star_count, 25);
    assert_eq!(gift.manage.next_transfer_date, 77);
    assert!(gift.manage.listed);
    assert_eq!(gift.manage.resale_star_count, 900);
    let plain = regular(json!({}));
    assert_eq!(plain.manage.upgrade_star_count, 30);
    assert!(!plain.manage.listed);
}

#[test]
fn pin_gate_needs_an_upgraded_saved_gift_and_a_free_slot() {
    assert_eq!(unique(json!({})).pin_gate(true, 0, 6), PinGate::Pin);
    assert_eq!(unique(json!({})).pin_gate(true, 6, 6), PinGate::Full);
    assert_eq!(
        unique(json!({})).pin_gate(false, 0, 6),
        PinGate::Unavailable
    );
    assert_eq!(
        unique(json!({"is_saved": false})).pin_gate(true, 0, 6),
        PinGate::Unavailable
    );
    assert_eq!(
        regular(json!({})).pin_gate(true, 0, 6),
        PinGate::Unavailable
    );
    // A pinned gift can always be unpinned, even at the limit.
    assert_eq!(
        unique(json!({"is_pinned": true})).pin_gate(true, 6, 6),
        PinGate::Unpin
    );
}

#[test]
fn pin_order_helpers() {
    let a = unique(json!({"received_gift_id": "a", "is_pinned": true}));
    let b = unique(json!({"received_gift_id": "b"}));
    let c = unique(json!({"received_gift_id": "c", "is_pinned": true}));
    let ids = pinned_ids(&[a, b, c]);
    assert_eq!(ids, vec!["a", "c"]);
    assert_eq!(pins_with(&ids, "b"), vec!["a", "c", "b"]);
    assert_eq!(pins_with(&ids, "a"), vec!["c", "a"]);
    assert_eq!(pins_without(&ids, "a"), vec!["c"]);
}

#[test]
fn upgrade_cost_is_free_when_prepaid_and_only_for_regular_own_gifts() {
    assert_eq!(regular(json!({})).upgrade_cost(true), Some(30));
    assert_eq!(
        regular(json!({"prepaid_upgrade_star_count": 30})).upgrade_cost(true),
        Some(0)
    );
    assert_eq!(regular(json!({})).upgrade_cost(false), None);
    assert_eq!(
        regular(json!({"can_be_upgraded": false})).upgrade_cost(true),
        None
    );
    assert_eq!(
        regular(json!({"was_refunded": true})).upgrade_cost(true),
        None
    );
    assert_eq!(
        unique(json!({"can_be_upgraded": true})).upgrade_cost(true),
        None
    );
}

#[test]
fn transfer_gate_follows_ownership_flag_and_cooldown() {
    assert_eq!(
        unique(json!({})).transfer_gate(true, NOW),
        TransferGate::Ready { star_count: 25 }
    );
    assert_eq!(
        unique(json!({})).transfer_gate(false, NOW),
        TransferGate::Unavailable
    );
    assert_eq!(
        unique(json!({"can_be_transferred": false})).transfer_gate(true, NOW),
        TransferGate::Unavailable
    );
    assert_eq!(
        unique(json!({"next_transfer_date": 2_000_000})).transfer_gate(true, NOW),
        TransferGate::Later(2_000_000)
    );
    // A date in the past is ready.
    assert_eq!(
        unique(json!({"next_transfer_date": 5})).transfer_gate(true, NOW),
        TransferGate::Ready { star_count: 25 }
    );
    assert_eq!(
        regular(json!({"can_be_transferred": true})).transfer_gate(true, NOW),
        TransferGate::Unavailable
    );
}

#[test]
fn resale_gate_needs_a_resale_date_or_a_listing() {
    assert_eq!(
        unique(json!({})).resale_gate(true, NOW),
        ResaleGate::Unavailable
    );
    assert_eq!(
        unique(json!({"next_resale_date": 10})).resale_gate(true, NOW),
        ResaleGate::Ready
    );
    assert_eq!(
        unique(json!({"next_resale_date": 9_999_999})).resale_gate(true, NOW),
        ResaleGate::Later(9_999_999)
    );
    assert_eq!(
        unique(json!({"next_resale_date": 10})).resale_gate(false, NOW),
        ResaleGate::Unavailable
    );
    let ton = unique(
        json!({"next_resale_date": 10, "gift": {"@type": "sentGiftUpgraded", "gift": {
        "@type": "upgradedGift", "title": "X", "number": 1,
        "resale_parameters": {"star_count": 0, "gram_only": true}}}}),
    );
    assert_eq!(ton.resale_gate(true, NOW), ResaleGate::TonOnly);
}

#[test]
fn limits_use_defaults_until_the_options_arrive() {
    let mut limits = GiftLimits::default();
    assert_eq!(limits.pinned(), 6);
    assert!(limits.resale_price_ok(125) && limits.resale_price_ok(35_000));
    assert!(!limits.resale_price_ok(124) && !limits.resale_price_ok(35_001));
    limits.set(crate::gift_manage::GiftOption::ResaleStarsMin, 200);
    limits.set(crate::gift_manage::GiftOption::PinnedMax, 3);
    assert!(!limits.resale_price_ok(199));
    assert_eq!(limits.pinned(), 3);
}

#[test]
fn input_helpers() {
    assert_eq!(parse_stars_input("1 250"), Some(1250));
    assert_eq!(parse_stars_input("1,250"), Some(1250));
    assert_eq!(parse_stars_input(" 7 "), Some(7));
    assert_eq!(parse_stars_input(""), None);
    assert_eq!(parse_stars_input("12.5"), None);
    assert_eq!(parse_stars_input("-3"), None);
    assert!(valid_collection_name("Cats"));
    assert!(valid_collection_name(" twelve chars "));
    assert!(!valid_collection_name("   "));
    assert!(!valid_collection_name("thirteen char"));
}

#[test]
fn parses_collections_and_the_upgrade_answer() {
    let collections = json!({"@type": "giftCollections", "collections": [
        {"@type": "giftCollection", "id": 4, "name": "Cats", "gift_count": 3},
        {"@type": "giftCollection", "id": 0, "name": "bad", "gift_count": 1}]});
    match parse_payload("giftCollections", &collections) {
        Some(GiftManagePayload::Collections(list)) => {
            assert_eq!(list.len(), 1);
            assert_eq!(
                (list[0].id, list[0].name.as_str(), list[0].gift_count),
                (4, "Cats", 3)
            );
        }
        other => panic!("{other:?}"),
    }
    let upgraded =
        json!({"@type": "upgradeGiftResult", "gift": {"title": "Plush Pepe", "number": 12}});
    match parse_payload("upgradeGiftResult", &upgraded) {
        Some(GiftManagePayload::Upgraded(s)) => {
            assert_eq!((s.title.as_str(), s.number), ("Plush Pepe", 12));
        }
        other => panic!("{other:?}"),
    }
    assert!(parse_payload("ok", &json!({})).is_none());
}

fn available(extra: Value) -> crate::gift_manage::AvailableGift {
    let mut gift = json!({"@type": "gift", "id": "55", "star_count": 50, "upgrade_star_count": 20});
    merge(&mut gift, extra);
    let value = json!({"@type": "availableGift", "gift": gift, "resale_count": 2});
    crate::gift_manage::parse_available_gift(&value).unwrap()
}

fn user() -> crate::gift_manage::GiftRecipient {
    crate::gift_manage::GiftRecipient {
        sender: MessageSender::User { user_id: 5 },
        name: "Ann".into(),
        is_self: false,
        is_channel: false,
    }
}

#[test]
fn send_gate_covers_sold_out_premium_auction_limits_and_channels() {
    let plain = available(json!({}));
    assert_eq!(plain.gate(&user(), false, NOW), SendGate::Ready);
    assert_eq!(plain.total_cost(true), 70);
    assert_eq!(plain.total_cost(false), 50);
    let sold = available(json!({"overall_limits": {"remaining_count": 0, "total_count": 100}}));
    assert!(sold.sold_out());
    assert_eq!(sold.gate(&user(), true, NOW), SendGate::SoldOut);
    assert_eq!(sold.supply_label().as_deref(), Some("Sold out"));
    let limited =
        available(json!({"overall_limits": {"remaining_count": 1500, "total_count": 5000}}));
    assert_eq!(limited.gate(&user(), false, NOW), SendGate::Ready);
    assert_eq!(
        limited.supply_label().as_deref(),
        Some("1,500 of 5,000 left")
    );
    let mut channel = user();
    channel.is_channel = true;
    assert_eq!(limited.gate(&channel, false, NOW), SendGate::NotForChannels);
    assert_eq!(plain.gate(&channel, false, NOW), SendGate::Ready);
    let premium = available(json!({"is_premium": true}));
    assert_eq!(premium.gate(&user(), false, NOW), SendGate::PremiumOnly);
    assert_eq!(premium.gate(&user(), true, NOW), SendGate::Ready);
    let auction = available(json!({"auction_info": {"@type": "giftAuction", "id": "a"}}));
    assert_eq!(auction.gate(&user(), true, NOW), SendGate::Auction);
    let none_left = available(json!({"user_limits": {"remaining_count": 0, "total_count": 2}}));
    assert_eq!(none_left.gate(&user(), true, NOW), SendGate::UserLimit);
    let soon = available(json!({"next_send_date": 5_000_000}));
    assert_eq!(soon.gate(&user(), true, NOW), SendGate::Later(5_000_000));
    assert_eq!(soon.gate(&user(), true, 6_000_000), SendGate::Ready);
}

#[test]
fn premium_options_parse_and_label() {
    let value = json!({"@type": "premiumGiftPaymentOptions", "options": [
        {"currency": "USD", "amount": 999, "star_count": 1000, "discount_percentage": 0, "month_count": 3},
        {"currency": "USD", "amount": 2999, "star_count": 3000, "discount_percentage": 25, "month_count": 12}]});
    let Some(GiftManagePayload::PremiumOptions(options)) =
        parse_payload("premiumGiftPaymentOptions", &value)
    else {
        panic!("not parsed");
    };
    assert_eq!(options[0].title(), "3 months");
    assert_eq!(options[1].title(), "1 year");
    assert_eq!(options[1].discount_percentage, 25);
    assert!(options[0].price_label().contains("9.99"));
}
