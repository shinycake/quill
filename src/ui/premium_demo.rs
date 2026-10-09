//! Screenshot fixtures for the Stars, gifts, Premium and gift-card screens.
//! English text and fake names only; everything is injected, no live
//! Telegram and no purchase of any kind.

use super::demo::{demo_file_json, demo_thumb_png_path};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::premium_hub::{
    parse_premium_features, parse_premium_state, parse_received_gifts, parse_star_transactions,
};
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const ME: i64 = 9;
const CHAT: i64 = 71;

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(1_700_000_000)
}

fn sticker_json(file_id: i32) -> serde_json::Value {
    let file: serde_json::Value =
        serde_json::from_str(&demo_file_json(file_id, &demo_thumb_png_path(), true))
            .unwrap_or_default();
    json!({"@type": "sticker", "id": file_id, "emoji": "\u{1F381}", "width": 512, "height": 512,
           "format": {"@type": "stickerFormatWebp"}, "sticker": file})
}

/// Fake people the fixtures name.
fn seed_users(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    for (id, first, last) in [
        (ME, "Idan", "Birman"),
        (1, "Dana", "Cole"),
        (2, "Omar", "Haddad"),
        (3, "Lea", "Stern"),
        (51, "Demo shop bot", ""),
    ] {
        let json = format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":{id},"first_name":"{first}","last_name":"{last}","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}

pub(super) fn apply_ready_stars(session: &mut Session, sink: &Arc<MemorySink>, seq: &AtomicU64) {
    seed_users(session, sink, seq);
    session.my_user_id = Some(ME);
    let t = now();
    let tx = |id: &str, stars: i64, ago: i64, ty: serde_json::Value| {
        json!({"id": id, "star_amount": {"star_count": stars, "nanostar_count": 0},
               "is_refund": false, "date": t - ago, "type": ty})
    };
    let page = parse_star_transactions(&json!({
        "star_amount": {"star_count": 1240, "nanostar_count": 0},
        "next_offset": "",
        "transactions": [
            tx("tx-a1f3", 1000, 3_600, json!({"@type": "starTransactionTypePremiumBotDeposit"})),
            tx("tx-b27c", -100, 86_400, json!({"@type": "starTransactionTypeGiftPurchase",
                "owner_id": {"@type": "messageSenderUser", "user_id": 1},
                "gift": {"@type": "gift", "star_count": 100}})),
            tx("tx-c91d", -250, 172_800, json!({"@type": "starTransactionTypeBotInvoicePurchase",
                "user_id": 51, "product_info": {"title": "Pro plan"}})),
            tx("tx-d004", 85, 259_200, json!({"@type": "starTransactionTypeGiftSale",
                "user_id": 2, "gift": {"@type": "gift", "star_count": 100}})),
            tx("tx-e55a", 25, 345_600, json!({"@type": "starTransactionTypeChannelPaidMediaSale",
                "user_id": 3, "message_id": 1, "media": []})),
            tx("tx-f6b8", -50, 432_000, json!({"@type": "starTransactionTypeChannelSubscriptionPurchase",
                "chat_id": -1002002, "subscription_period": 2592000})),
        ]
    }));
    session.hub.apply_transactions(page, false);
    session.hub.stars_open = true;
}

pub(super) fn apply_ready_gifts(
    session: &mut Session,
    select_first: bool,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_users(session, sink, seq);
    session.my_user_id = Some(ME);
    let t = now();
    let gift = |id: &str,
                sender: i64,
                stars: i64,
                saved: bool,
                ago: i64,
                text: &str,
                sell: i64,
                file: i32| {
        json!({"received_gift_id": id,
               "sender_id": {"@type": "messageSenderUser", "user_id": sender},
               "text": {"text": text, "entities": []},
               "is_private": false, "is_saved": saved, "is_pinned": id == "g1",
               "can_be_upgraded": true, "date": t - ago, "sell_star_count": sell,
               "gift": {"@type": "sentGiftRegular",
                        "gift": {"@type": "gift", "star_count": stars, "sticker": sticker_json(file)}}})
    };
    let collectible = json!({"received_gift_id": "g5", "is_saved": true, "date": t - 900_000,
        "sender_id": {"@type": "messageSenderUser", "user_id": 3},
        "gift": {"@type": "sentGiftUpgraded",
                 "gift": {"@type": "upgradedGift", "title": "Plush Pepe", "number": 4242,
                          "model": {"name": "Classic", "sticker": sticker_json(206)},
                          "backdrop": {"name": "Ocean", "colors": {"center_color": 0x2e86de, "edge_color": 0x1b4f8f}}}}});
    let page = parse_received_gifts(&json!({
        "total_count": 5, "next_offset": "",
        "gifts": [
            gift("g1", 1, 100, true, 86_400, "Happy birthday!", 85, 201),
            gift("g2", 2, 50, true, 172_800, "", 42, 202),
            gift("g3", 3, 250, false, 345_600, "For all the help this year.", 212, 203),
            gift("g4", 1, 25, true, 604_800, "", 21, 204),
            collectible,
        ]
    }));
    for gift in &page.gifts {
        for file in &gift.gift.files {
            session.files.insert(file.id.0, file.clone());
        }
    }
    let first = page.gifts.first().map(|g| g.id.clone());
    session.hub.gifts_owner = Some(quill::telegram::envelope::MessageSender::User { user_id: ME });
    session.hub.apply_gifts(page, false);
    session.hub.gifts_open = true;
    if select_first {
        session.hub.gift_selected = first;
    }
}

pub(super) fn apply_ready_premium(session: &mut Session) {
    session.hub.premium = Some(parse_premium_features(&json!({
        "features": [
            {"@type": "premiumFeatureIncreasedLimits"},
            {"@type": "premiumFeatureIncreasedUploadFileSize"},
            {"@type": "premiumFeatureImprovedDownloadSpeed"},
            {"@type": "premiumFeatureVoiceRecognition"},
            {"@type": "premiumFeatureDisabledAds"},
            {"@type": "premiumFeatureUniqueReactions"},
            {"@type": "premiumFeatureCustomEmoji"},
            {"@type": "premiumFeatureEmojiStatus"},
        ],
        "limits": [
            {"type": {"@type": "premiumLimitTypeSupergroupCount"}, "default_value": 500, "premium_value": 1000},
            {"type": {"@type": "premiumLimitTypePinnedChatCount"}, "default_value": 5, "premium_value": 10},
            {"type": {"@type": "premiumLimitTypeChatFolderCount"}, "default_value": 10, "premium_value": 20},
        ]
    })));
    session.hub.premium_state = Some(parse_premium_state(&json!({
        "state": {"text": "Unlock exclusive features, doubled limits and more.", "entities": []},
        "payment_options": []
    })));
    session.hub.premium_open = true;
}

pub(super) fn apply_ready_gift_cards(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    seed_users(session, sink, seq);
    session.my_user_id = Some(ME);
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let t = now();
    let sticker = |id: i32| sticker_json(id).to_string();
    let text = |s: &str| {
        format!(
            r#"{{"@type":"formattedText","text":{},"entities":[]}}"#,
            serde_json::to_string(s).unwrap_or_default()
        )
    };
    let mut jsons = vec![
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{CHAT},"title":"Dana Cole","type":{{"@type":"chatTypePrivate","user_id":1}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{CHAT},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"4998","is_pinned":false}}}}"#
        ),
        format!(
            r#"{{"@type":"updateUser","user":{{"@type":"user","id":1,"first_name":"Dana","last_name":"Cole","usernames":null,"phone_number":"","status":{{"@type":"userStatusRecently"}},"profile_photo":null,"is_contact":true,"type":{{"@type":"userTypeRegular"}}}}}}"#
        ),
    ];
    let contents = vec![
        (
            1,
            false,
            format!(
                r#"{{"@type":"messageGift","gift":{{"@type":"gift","id":"1","star_count":100,"sticker":{}}},"sender_id":{{"@type":"messageSenderUser","user_id":1}},"receiver_id":{{"@type":"messageSenderUser","user_id":{ME}}},"received_gift_id":"rg1","text":{},"sell_star_count":85,"prepaid_upgrade_star_count":0}}"#,
                sticker(301),
                text("Happy birthday! Hope this makes you smile.")
            ),
        ),
        (
            1,
            false,
            format!(
                r#"{{"@type":"messageUpgradedGift","gift":{{"@type":"upgradedGift","title":"Plush Pepe","number":4242,"model":{{"name":"Classic","sticker":{}}},"symbol":{{"name":"Star"}},"backdrop":{{"name":"Ocean","colors":{{"center_color":3048158,"edge_color":1789839}}}}}},"sender_id":{{"@type":"messageSenderUser","user_id":1}},"receiver_id":{{"@type":"messageSenderUser","user_id":{ME}}},"origin":{{"@type":"upgradedGiftOriginUpgrade"}},"received_gift_id":"rg2"}}"#,
                sticker(302)
            ),
        ),
        (
            1,
            false,
            format!(
                r#"{{"@type":"messageGiftedPremium","gifter_user_id":1,"receiver_user_id":{ME},"text":{},"currency":"USD","amount":999,"month_count":3,"day_count":0,"sticker":{}}}"#,
                text("Enjoy Premium!"),
                sticker(303)
            ),
        ),
        (
            ME,
            true,
            format!(
                r#"{{"@type":"messageGiftedStars","gifter_user_id":{ME},"receiver_user_id":1,"currency":"USD","amount":499,"star_count":250,"transaction_id":"t","sticker":{}}}"#,
                sticker(304)
            ),
        ),
        (
            1,
            false,
            format!(
                r#"{{"@type":"messageGiveaway","parameters":{{"@type":"giveawayParameters","boosted_chat_id":-1002002,"prize_description":"Plus a limited edition mug"}},"winner_count":5,"prize":{{"@type":"giveawayPrizeStars","star_count":500}},"sticker":{}}}"#,
                sticker(305)
            ),
        ),
    ];
    for (index, (sender, outgoing, content)) in contents.into_iter().enumerate() {
        let id = 3001 + index as i64;
        let date = t - 3_000 + index as i64 * 60;
        jsons.push(format!(
            r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{CHAT},"sender_id":{{"@type":"messageSenderUser","user_id":{sender}}},"is_outgoing":{outgoing},"date":{date},"content":{content}}}}}"#
        ));
    }
    for json in jsons {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
    session.open_chat(ChatId(CHAT));
}
