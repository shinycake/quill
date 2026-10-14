//! `ready-gift-resale` and `ready-gift-auction`: the gift market over the
//! ready chat list (injected, no live Telegram).
//!
//! `ready-gift-resale` opens the resale box of "Plush Pepe" with two models
//! picked; `QUILL_DEMO_MARKET=buy` or `confirm` opens the buy box above it.
//! `ready-gift-auction` shows the chat-list bar of a running auction the
//! user bid in; `QUILL_DEMO_MARKET` opens a box over it: `bid`, `info`,
//! `about`, `send`, `active` or `bought`.

use super::app::QuillApp;
use super::demo::{demo_file_json, demo_media_allowlist, seed_ready_chats_session};
use super::gift_market_state::MarketBox;
use super::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::gift_manage::GiftRecipient;
use quill::gift_market::{
    AttributeId, AuctionState, BuyStep, ResaleBuy, ResaleFilter, ResaleSearch, ResaleSort,
    parse_acquired_gifts, parse_resale_page,
};
use quill::local_time::now_unix;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::MessageSender;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

register_demos![
    DemoSpec::ready(
        "ready-gift-resale",
        seed_gift_market_session,
        "screenshot demo — gift resale: listings with sort and attribute filters (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_resale),
    DemoSpec::ready(
        "ready-gift-auction",
        seed_gift_market_session,
        "screenshot demo — gift auction: chat-list bar, bid, info and about boxes (injected, no live Telegram)"
    )
    .setup(QuillApp::demo_gift_auction),
];

/// The signed-in user.
pub(super) const ME: i64 = 501;
/// The regular gift whose collectibles are on resale.
pub(super) const RESALE_GIFT: i64 = 7001;
/// The gift sold by auction.
pub(super) const AUCTION_GIFT: i64 = 7100;

/// (id, first name, last name, avatar) of the people in the fixtures.
const PEOPLE: [(i64, &str, &str, &str); 5] = [
    (ME, "Robin", "Lee", "av07.png"),
    (502, "Maya", "Chen", "av01.png"),
    (503, "Ben", "Ito", "av05.png"),
    (504, "Dana", "Weiss", "av02.png"),
    (505, "Leo", "Park", "av10.png"),
];

/// (model sticker id, name) of the resale models.
const MODELS: [(i64, &str); 4] = [
    (31, "Cozy Frog"),
    (32, "Gold Rush"),
    (33, "Midnight"),
    (34, "Spring Pepe"),
];

/// (id, name, center, edge) of the backdrops.
const BACKDROPS: [(i32, &str, u32, u32); 4] = [
    (1, "Emerald", 0x5BA36F, 0x2E6B45),
    (2, "Sapphire", 0x4C7FD0, 0x2B4B8C),
    (3, "Rose Gold", 0xD7957F, 0x9E5C4C),
    (4, "Onyx Black", 0x5A5F66, 0x2C3036),
];

fn attribute(kind: &str, name: &str, sticker_id: i64, per_mille: i32) -> Value {
    json!({"@type": kind, "name": name,
        "sticker": {"@type": "sticker", "id": sticker_id.to_string(), "set_id": "1",
            "width": 512, "height": 512, "emoji": "\u{1F381}",
            "format": {"@type": "stickerFormatTgs"},
            "sticker": {"@type": "file", "id": 0, "size": 0, "expected_size": 0,
                "local": {"path": "", "is_downloading_completed": false},
                "remote": {"id": "", "unique_id": ""}}},
        "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": per_mille}})
}

fn backdrop(index: usize) -> Value {
    let (id, name, center, edge) = BACKDROPS
        .get(index % BACKDROPS.len())
        .copied()
        .unwrap_or((1, "Emerald", 0x5BA36F, 0x2E6B45));
    json!({"@type": "upgradedGiftBackdrop", "id": id, "name": name,
        "colors": {"center_color": center, "edge_color": edge, "symbol_color": 0x1D3B2A, "text_color": 0xFFFFFF},
        "rarity": {"@type": "upgradedGiftAttributeRarityPerMille", "per_mille": 15 + 5 * id}})
}

pub(super) fn listing(number: i32, model: usize, backdrop_ix: usize, stars: i64) -> Value {
    let (sticker_id, name) = MODELS
        .get(model % MODELS.len())
        .copied()
        .unwrap_or((31, "Cozy Frog"));
    json!({"@type": "giftForResale", "received_gift_id": "",
        "gift": {"@type": "upgradedGift", "id": (90_000 + number).to_string(),
            "regular_gift_id": RESALE_GIFT.to_string(), "title": "Plush Pepe",
            "name": format!("PlushPepe-{number}"), "number": number,
            "model": attribute("upgradedGiftModel", name, sticker_id, 12),
            "symbol": attribute("upgradedGiftSymbol", "Clover", 41, 8),
            "backdrop": backdrop(backdrop_ix),
            "owner_id": {"@type": "messageSenderUser", "user_id": 503},
            "gift_address": "",
            "resale_parameters": {"@type": "giftResaleParameters", "star_count": stars,
                "gram_cent_count": stars * 2, "gram_only": false},
            "craft_probability_per_mille": 0, "is_crafted": false, "is_burned": false}})
}

pub(super) fn resale_page() -> Value {
    let listings: Vec<Value> = [
        (1204, 0, 0, 1_250),
        (88, 1, 1, 1_400),
        (3051, 0, 2, 1_480),
        (412, 1, 3, 1_650),
        (2210, 0, 1, 1_900),
        (77, 1, 0, 2_400),
        (5120, 0, 3, 2_750),
        (19, 1, 2, 4_100),
        (640, 0, 0, 5_500),
    ]
    .into_iter()
    .map(|(number, model, back, stars)| listing(number, model, back, stars))
    .collect();
    let models: Vec<Value> = MODELS
        .iter()
        .zip([128, 96, 41, 12])
        .map(|((id, name), count)| {
            json!({"@type": "upgradedGiftModelCount",
                "model": attribute("upgradedGiftModel", name, *id, 12), "total_count": count})
        })
        .collect();
    let backdrops: Vec<Value> = (0..BACKDROPS.len())
        .zip([90, 75, 60, 52])
        .map(|(ix, count)| {
            json!({"@type": "upgradedGiftBackdropCount", "backdrop": backdrop(ix), "total_count": count})
        })
        .collect();
    json!({"@type": "giftsForResale", "total_count": 224, "gifts": listings,
        "models": models, "backdrops": backdrops,
        "symbols": [
            {"@type": "upgradedGiftSymbolCount", "symbol": attribute("upgradedGiftSymbol", "Clover", 41, 8), "total_count": 140},
            {"@type": "upgradedGiftSymbolCount", "symbol": attribute("upgradedGiftSymbol", "Star", 42, 8), "total_count": 84}
        ],
        "next_offset": "9"})
}

/// The running auction: round 3 of 10, the user in fourth place.
pub(super) fn auction_state(now: i64, my_bid: Option<i64>) -> Option<AuctionState> {
    let user_bid = my_bid.map_or(Value::Null, |stars| {
        json!({"@type": "userAuctionBid", "star_count": stars, "bid_date": now - 300,
            "next_bid_star_count": stars + 50,
            "owner_id": {"@type": "messageSenderUser", "user_id": 502}, "was_returned": false})
    });
    let value = json!({"@type": "giftAuctionState",
        "gift": {"@type": "gift", "id": AUCTION_GIFT.to_string(), "star_count": 0,
            "auction_info": {"@type": "giftAuction", "id": "toybear", "gifts_per_round": 3,
                "start_date": now - 3_600},
            "overall_limits": {"@type": "giftPurchaseLimits", "total_count": 30, "remaining_count": 21}},
        "state": {"@type": "auctionStateActive", "start_date": now - 3_600, "end_date": now + 7_200,
            "min_bid": 500,
            "bid_levels": [
                {"@type": "auctionBid", "star_count": 6_200, "bid_date": now - 900, "position": 1},
                {"@type": "auctionBid", "star_count": 4_800, "bid_date": now - 800, "position": 2},
                {"@type": "auctionBid", "star_count": 4_100, "bid_date": now - 700, "position": 3}
            ],
            "top_bidder_user_ids": [503, 504, 505],
            "rounds": [
                {"@type": "auctionRound", "number": 1, "duration": 1_200, "extend_time": 0, "top_winner_count": 0},
                {"@type": "auctionRound", "number": 2, "duration": 600, "extend_time": 30, "top_winner_count": 3}
            ],
            "current_round_end_date": now + 274, "current_round_number": 3,
            "total_round_count": 10, "distributed_item_count": 9, "left_item_count": 21,
            "acquired_item_count": 1, "user_bid": user_bid}});
    AuctionState::parse(&value)
}

pub(super) fn seed_gift_market_session(sink: Arc<MemorySink>) -> Session {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let mut session = seed_ready_chats_session(sink);
    session.my_user_id = Some(ME);
    let seq = AtomicU64::new(session.last_seq);
    let mut apply = |json: &str| {
        if let Some(owned) = copy_and_parse(json, &seq, &dyn_sink) {
            session.apply(owned);
        }
    };
    let showcase = demo_media_allowlist().join("showcase");
    for (ix, (id, first, last, avatar)) in PEOPLE.iter().enumerate() {
        let path = showcase.join(avatar).to_string_lossy().to_string();
        let small = demo_file_json(6_100 + ix as i32, &path, true);
        apply(&format!(r#"{{"@type":"updateFile","file":{small}}}"#));
        apply(&format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":{{"@type":"profilePhoto","id":{pid},"small":{small},"big":null,"minithumbnail":null,"has_animation":false,"is_personal":false}},"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#,
            pid = 9_100 + ix
        ));
    }
    apply(
        r#"{"@type":"updateNewChat","chat":{"id":502,"title":"Maya Chen","type":{"@type":"chatTypePrivate","user_id":502},"unread_count":0}}"#,
    );
    apply(
        r#"{"@type":"updateOwnedStarCount","star_amount":{"@type":"starAmount","star_count":24500,"nanostar_count":0}}"#,
    );
    let now = now_unix();
    let market = &mut session.payments.gift_market;
    market.remember_title(RESALE_GIFT, "Plush Pepe");
    market.remember_title(AUCTION_GIFT, "Toy Bear");
    let mut filter = ResaleFilter {
        sort: ResaleSort::Price,
        ..ResaleFilter::default()
    };
    filter.toggle(AttributeId::Model(31));
    filter.toggle(AttributeId::Model(32));
    let mut search = ResaleSearch::new(RESALE_GIFT, filter, false);
    search.apply(parse_resale_page(&resale_page()), Some(ME), false);
    market.resale = Some(search);
    if let Some(state) = auction_state(now, Some(4_000)) {
        market.active = vec![state.clone()];
        market.auctions.insert(AUCTION_GIFT, state);
    }
    market.acquired.insert(
        AUCTION_GIFT,
        parse_acquired_gifts(&json!({"@type": "giftAuctionAcquiredGifts", "gifts": [
            {"@type": "giftAuctionAcquiredGift",
                "receiver_id": {"@type": "messageSenderUser", "user_id": 502},
                "date": now - 5_000, "star_count": 3_900, "bid_date": now - 5_400,
                "auction_round_number": 1, "auction_round_position": 2,
                "unique_gift_number": 14, "text": {"@type": "formattedText", "text": "For your desk!", "entities": []},
                "is_private": false}
        ]})),
    );
    session
}

impl QuillApp {
    fn demo_gift_resale(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.gift_market.stack = vec![MarketBox::Resale];
        let step = match std::env::var("QUILL_DEMO_MARKET")
            .unwrap_or_default()
            .as_str()
        {
            "buy" => Some(BuyStep::View),
            "confirm" => Some(BuyStep::Confirm),
            _ => None,
        };
        if let Some(step) = step
            && let Some(market) = self.gift_market_mut()
            && let Some(listing) = market
                .resale
                .as_ref()
                .and_then(|search| search.listings.first().cloned())
        {
            let mut buy = ResaleBuy::new(
                listing,
                GiftRecipient {
                    sender: MessageSender::User { user_id: 502 },
                    name: "Maya Chen".into(),
                    is_self: false,
                    is_channel: false,
                },
                false,
            );
            buy.step = step;
            market.buy = Some(buy);
            self.gift_market.stack.push(MarketBox::Buy);
        }
        cx.notify();
    }

    fn demo_gift_auction(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let gift_id = AUCTION_GIFT;
        let next = match std::env::var("QUILL_DEMO_MARKET")
            .unwrap_or_default()
            .as_str()
        {
            "bid" => Some(MarketBox::AuctionBid { gift_id }),
            "info" => Some(MarketBox::AuctionInfo { gift_id }),
            "about" => Some(MarketBox::AuctionAbout { gift_id }),
            "send" => Some(MarketBox::AuctionSend { gift_id }),
            "active" => Some(MarketBox::ActiveAuctions),
            "bought" => Some(MarketBox::AuctionBought { gift_id }),
            _ => None,
        };
        if let Some(next) = next {
            self.gift_market.recipient = Some(GiftRecipient {
                sender: MessageSender::User { user_id: 502 },
                name: "Maya Chen".into(),
                is_self: false,
                is_channel: false,
            });
            self.gift_market.stack = vec![next];
        }
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "gift_market_tests.rs"]
mod tests;
