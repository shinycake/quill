//! Connect-driver tests: the gift market (recorded TDLib JSON in,
//! requests out).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::gift_market::{AttributeId, CraftGift, CraftResult, ResaleFilter, ResaleSort};
use crate::ids::ChatId;
use crate::marketplace::GiftPrice;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 7;
const FRIEND_CHAT: ChatId = ChatId(55);

struct Fixture {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    dyn_sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
    dir: std::path::PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let seq = AtomicU64::new(0);
        let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
        driver.session.my_user_id = Some(ME);
        let mut fixture = Self {
            driver,
            recorder,
            dyn_sink,
            seq,
            dir,
        };
        fixture.ingest(&format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{},"title":"Ann","type":{{"@type":"chatTypePrivate","user_id":{}}},"unread_count":0}}}}"#,
            FRIEND_CHAT.0, FRIEND_CHAT.0
        ));
        fixture
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.dyn_sink).unwrap())
            .unwrap();
    }

    fn answer(&mut self, ty: &str, mut value: Value) {
        let extra = self.last(ty)["@extra"].clone();
        value["@extra"] = extra;
        self.ingest(&value.to_string());
    }

    fn sent(&self, ty: &str) -> Vec<Value> {
        self.recorder
            .snapshot()
            .into_iter()
            .filter_map(|j| serde_json::from_str::<Value>(&j).ok())
            .filter(|v| v["@type"] == ty)
            .collect()
    }

    fn last(&self, ty: &str) -> Value {
        self.sent(ty).last().cloned().unwrap_or(Value::Null)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn collectible(number: i32, owner: i64) -> Value {
    json!({"@type": "upgradedGift", "id": (9000 + number).to_string(), "regular_gift_id": "500",
        "title": "Plush Pepe", "name": format!("PlushPepe-{number}"), "number": number,
        "model": {"@type": "upgradedGiftModel", "name": "Gold", "rarity": {"@type": "upgradedGiftAttributeRarityRare"}},
        "symbol": {"@type": "upgradedGiftSymbol", "name": "Star", "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": 20}},
        "backdrop": {"@type": "upgradedGiftBackdrop", "id": 3, "name": "Onyx", "colors": {"center_color": 1, "edge_color": 2, "symbol_color": 3, "text_color": 4},
            "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": 15}},
        "owner_id": {"@type": "messageSenderUser", "user_id": owner},
        "resale_parameters": {"@type": "giftResaleParameters", "star_count": 1500, "gram_cent_count": 0, "gram_only": false},
        "craft_probability_per_mille": 200})
}

fn resale_page(numbers: &[i32]) -> Value {
    json!({"@type": "giftsForResale", "total_count": numbers.len(),
        "gifts": numbers.iter().map(|n| json!({"@type": "giftForResale", "gift": collectible(*n, 99), "received_gift_id": ""})).collect::<Vec<_>>(),
        "models": [], "symbols": [], "backdrops": [], "next_offset": "next"})
}

#[test]
fn resale_search_sends_filters_and_pages() {
    let mut f = Fixture::new();
    f.driver.open_gift_resale(500, "Plush Pepe").unwrap();
    let first = f.last("searchGiftsForResale");
    assert_eq!(first["gift_id"], "500");
    assert_eq!(first["order"]["@type"], "giftForResaleOrderPrice");
    assert_eq!(first["for_crafting"], false);
    assert_eq!(first["offset"], "");
    f.answer("searchGiftsForResale", resale_page(&[1, 2]));
    let search = f
        .driver
        .session
        .payments
        .gift_market
        .resale
        .clone()
        .unwrap();
    assert_eq!(search.listings.len(), 2);
    assert!(search.has_more());
    assert_eq!(
        f.driver.session.gift_market_title(500).as_deref(),
        Some("Plush Pepe")
    );
    f.driver.load_more_gift_resale(false).unwrap();
    assert_eq!(f.last("searchGiftsForResale")["offset"], "next");
    f.answer("searchGiftsForResale", resale_page(&[3]));
    assert_eq!(
        f.driver
            .session
            .payments
            .gift_market
            .resale
            .as_ref()
            .unwrap()
            .listings
            .len(),
        3
    );
    let mut filter = ResaleFilter {
        sort: ResaleSort::Date,
        stars_only: true,
        ..ResaleFilter::default()
    };
    filter.toggle(AttributeId::Model(77));
    f.driver.set_gift_resale_filter(filter.clone()).unwrap();
    let filtered = f.last("searchGiftsForResale");
    assert_eq!(
        filtered["order"]["@type"],
        "giftForResaleOrderPriceChangeDate"
    );
    assert_eq!(filtered["for_stars"], true);
    assert_eq!(filtered["attributes"][0]["sticker_id"], "77");
    assert_eq!(filtered["offset"], "");
    // The same filter again sends nothing.
    assert!(f.driver.set_gift_resale_filter(filter).is_err());
    f.answer("searchGiftsForResale", resale_page(&[4]));
    assert_eq!(
        f.driver
            .session
            .payments
            .gift_market
            .resale
            .as_ref()
            .unwrap()
            .listings
            .len(),
        1
    );
}

#[test]
fn resale_buy_sends_the_confirmed_price_and_handles_a_change() {
    let mut f = Fixture::new();
    f.driver.open_gift_resale(500, "Plush Pepe").unwrap();
    f.answer("searchGiftsForResale", resale_page(&[1, 2]));
    f.driver
        .start_gift_resale_buy("PlushPepe-1", Some(FRIEND_CHAT), false)
        .unwrap();
    // A stale price is refused before anything goes out.
    assert!(
        f.driver
            .buy_gift_resale(GiftPrice::Stars(10), "", false)
            .is_err()
    );
    f.driver
        .buy_gift_resale(GiftPrice::Stars(1500), "Happy day", true)
        .unwrap();
    let sent = f.last("sendResoldGift");
    assert_eq!(sent["gift_name"], "PlushPepe-1");
    assert_eq!(sent["owner_id"]["user_id"], FRIEND_CHAT.0);
    assert_eq!(sent["price"]["star_count"], 1500);
    assert_eq!(sent["is_private"], true);
    f.answer(
        "sendResoldGift",
        json!({"@type": "giftResaleResultPriceIncreased", "price": {"@type": "giftResalePriceStar", "star_count": 1700}}),
    );
    let buy = f.driver.session.payments.gift_market.buy.clone().unwrap();
    assert_eq!(buy.price(), Some(GiftPrice::Stars(1700)));
    assert!(!buy.sending);
    f.driver
        .buy_gift_resale(GiftPrice::Stars(1700), "", false)
        .unwrap();
    f.answer(
        "sendResoldGift",
        json!({"@type": "giftResaleResultOk", "received_gift_id": "rx"}),
    );
    let market = &f.driver.session.payments.gift_market;
    assert!(market.buy.is_none());
    assert_eq!(market.resale.as_ref().unwrap().listings.len(), 1);
    let toasts = f.driver.session.take_gift_market_toasts();
    assert_eq!(
        toasts,
        vec![(
            "Gift Sent!".to_string(),
            "Ann has been notified about your gift.".to_string()
        )]
    );
}

fn auction_state(my: Value) -> Value {
    json!({"@type": "giftAuctionState",
        "gift": {"@type": "gift", "id": "800",
            "auction_info": {"@type": "giftAuction", "id": "pepe", "gifts_per_round": 3, "start_date": 1},
            "overall_limits": {"@type": "giftPurchaseLimits", "total_count": 100, "remaining_count": 40}},
        "state": {"@type": "auctionStateActive", "start_date": 1, "end_date": 2_000_000_000,
            "min_bid": 100, "bid_levels": [{"star_count": 500, "bid_date": 1, "position": 1}],
            "top_bidder_user_ids": [3], "rounds": [], "current_round_end_date": 2_000_000_000,
            "current_round_number": 1, "total_round_count": 3, "distributed_item_count": 0,
            "left_item_count": 40, "acquired_item_count": 0, "user_bid": my}})
}

#[test]
fn auction_link_follow_and_bids() {
    let mut f = Fixture::new();
    f.driver.fetch_gift_auction("pepe").unwrap();
    assert_eq!(f.last("getGiftAuctionState")["auction_id"], "pepe");
    assert!(f.driver.session.payments.gift_market.auction_loading);
    f.answer("getGiftAuctionState", auction_state(Value::Null));
    let market = &f.driver.session.payments.gift_market;
    assert_eq!(market.auction_ready, Some(800));
    assert!(!market.auction_loading);
    f.driver.follow_gift_auction(Some(800)).unwrap();
    assert_eq!(f.last("openGiftAuction")["gift_id"], "800");
    // Below the minimum bid nothing is sent.
    assert!(
        f.driver
            .bid_gift_auction(800, 50, FRIEND_CHAT.0, "", false)
            .is_err()
    );
    f.driver
        .bid_gift_auction(800, 600, FRIEND_CHAT.0, "for you", false)
        .unwrap();
    let place = f.last("placeGiftAuctionBid");
    assert_eq!(place["gift_id"], "800");
    assert_eq!(place["star_count"], 600);
    assert_eq!(place["user_id"], FRIEND_CHAT.0);
    assert_eq!(place["text"]["text"], "for you");
    // One bid at a time.
    assert!(
        f.driver
            .bid_gift_auction(800, 700, FRIEND_CHAT.0, "", false)
            .is_err()
    );
    f.answer("placeGiftAuctionBid", json!({"@type": "ok"}));
    assert_eq!(
        f.driver.session.take_gift_market_toasts()[0].0,
        "Your bid has been placed"
    );
    f.ingest(
        &json!({"@type": "updateGiftAuctionState", "state": auction_state(json!({
            "star_count": 600, "bid_date": 2, "next_bid_star_count": 601,
            "owner_id": {"@type": "messageSenderUser", "user_id": FRIEND_CHAT.0}, "was_returned": false}))})
        .to_string(),
    );
    assert_eq!(
        f.driver.session.payments.gift_market.auctions[&800].my_stars(),
        600
    );
    f.driver
        .bid_gift_auction(800, 900, FRIEND_CHAT.0, "", false)
        .unwrap();
    let raise = f.last("increaseGiftAuctionBid");
    assert_eq!(
        (raise["gift_id"].clone(), raise["star_count"].clone()),
        (json!("800"), json!(900))
    );
    f.ingest(
        &json!({"@type": "error", "code": 400, "message": "BALANCE_TOO_LOW",
            "@extra": f.last("increaseGiftAuctionBid")["@extra"]})
        .to_string(),
    );
    assert!(f.driver.session.payments.gift_market.bidding.is_none());
    // A bid for someone else is a new bid, not a raise of the friend's.
    f.driver
        .bid_gift_auction(800, 900, FRIEND_CHAT.0 + 1, "", false)
        .unwrap();
    let other = f.last("placeGiftAuctionBid");
    assert_eq!(other["user_id"], FRIEND_CHAT.0 + 1);
    assert_eq!(other["star_count"], 900);
    f.answer("placeGiftAuctionBid", json!({"@type": "ok"}));
    f.driver.follow_gift_auction(None).unwrap();
    assert_eq!(f.last("closeGiftAuction")["gift_id"], "800");
    assert!(f.driver.session.payments.gift_market.followed.is_none());
}

#[test]
fn active_auctions_feed_the_bar() {
    let mut f = Fixture::new();
    let my = json!({"star_count": 600, "bid_date": 2, "next_bid_star_count": 601, "was_returned": false});
    f.ingest(
        &json!({"@type": "updateActiveGiftAuctions", "states": [auction_state(my)]}).to_string(),
    );
    let list = f
        .driver
        .session
        .payments
        .gift_market
        .active_auctions(crate::local_time::now_unix());
    assert_eq!(list.len(), 1);
    let bar = crate::gift_market::auction_bar(&list, crate::local_time::now_unix()).unwrap();
    assert_eq!(bar.text, "You're winning (1st place).");
    f.ingest(&json!({"@type": "updateActiveGiftAuctions", "states": []}).to_string());
    assert!(f.driver.session.payments.gift_market.active.is_empty());
}

#[test]
fn acquired_and_preview_requests() {
    let mut f = Fixture::new();
    f.driver.fetch_gift_auction_acquired(800).unwrap();
    assert_eq!(f.last("getGiftAuctionAcquiredGifts")["gift_id"], "800");
    f.answer(
        "getGiftAuctionAcquiredGifts",
        json!({"@type": "giftAuctionAcquiredGifts", "gifts": [{"receiver_id": {"@type": "messageSenderUser", "user_id": 5},
            "date": 1, "star_count": 700, "auction_round_number": 1, "auction_round_position": 2,
            "unique_gift_number": 0, "text": {"text": ""}, "is_private": false}]}),
    );
    assert_eq!(
        f.driver.session.payments.gift_market.acquired[&800].len(),
        1
    );
    f.driver.fetch_gift_upgrade_preview(800).unwrap();
    assert_eq!(f.last("getGiftUpgradePreview")["regular_gift_id"], "800");
    // Not twice while in flight.
    assert!(f.driver.fetch_gift_upgrade_preview(800).is_err());
    f.answer(
        "getGiftUpgradePreview",
        json!({"@type": "giftUpgradePreview", "models": [], "symbols": [], "backdrops": [], "prices": [], "next_prices": []}),
    );
    assert!(
        f.driver
            .session
            .payments
            .gift_market
            .previews
            .contains_key(&800)
    );
}

fn received(number: i32) -> Value {
    json!({"@type": "receivedGift", "received_gift_id": format!("r{number}"),
        "gift": {"@type": "sentGiftUpgraded", "gift": collectible(number, ME)}, "craft_date": 0})
}

#[test]
fn craft_loads_both_lists_and_crafts() {
    let mut f = Fixture::new();
    let primary = CraftGift::parse_received(&received(1)).unwrap();
    f.driver.open_gift_craft(primary).unwrap();
    assert_eq!(f.last("getGiftsForCrafting")["regular_gift_id"], "500");
    let market = f.last("searchGiftsForResale");
    assert_eq!(market["for_crafting"], true);
    f.answer(
        "getGiftsForCrafting",
        json!({"@type": "giftsForCrafting", "total_count": 2, "gifts": [received(2), received(3)],
            "attribute_persistence_probabilities": [{"persistence_chance_per_mille": [1000]}],
            "next_offset": ""}),
    );
    f.answer("searchGiftsForResale", resale_page(&[8]));
    let craft = f.driver.session.payments.gift_market.craft.clone().unwrap();
    assert_eq!(craft.candidates.len(), 2);
    assert_eq!(craft.market.listings.len(), 1);
    assert_eq!(craft.probabilities, vec![vec![1000]]);
    if let Some(craft) = f.driver.session.payments.gift_market.craft.as_mut() {
        let second = craft.candidates[0].clone();
        assert!(craft.add(second));
    }
    f.driver.craft_gift().unwrap();
    assert_eq!(
        f.last("craftGift")["received_gift_ids"],
        json!(["r1", "r2"])
    );
    // Not twice.
    assert!(f.driver.craft_gift().is_err());
    f.answer("craftGift", json!({"@type": "craftGiftResultFail"}));
    let craft = f.driver.session.payments.gift_market.craft.clone().unwrap();
    assert!(!craft.crafting);
    assert_eq!(craft.result, Some(CraftResult::Fail));
}

#[test]
fn craft_buys_a_listing_into_a_slot() {
    let mut f = Fixture::new();
    f.driver
        .open_gift_craft(CraftGift::parse_received(&received(1)).unwrap())
        .unwrap();
    f.answer("searchGiftsForResale", resale_page(&[8]));
    f.driver
        .start_gift_resale_buy("PlushPepe-8", None, true)
        .unwrap();
    f.driver
        .buy_gift_resale(GiftPrice::Stars(1500), "", false)
        .unwrap();
    assert_eq!(f.last("sendResoldGift")["owner_id"]["user_id"], ME);
    f.answer(
        "sendResoldGift",
        json!({"@type": "giftResaleResultOk", "received_gift_id": "bought"}),
    );
    let craft = f.driver.session.payments.gift_market.craft.clone().unwrap();
    assert_eq!(craft.slots.len(), 2);
    assert_eq!(craft.slots[1].received_gift_id, "bought");
    assert!(craft.market.listings.is_empty());
    assert!(f.driver.session.take_gift_market_toasts().is_empty());
}
