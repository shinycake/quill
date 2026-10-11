//! Request builders for managing received gifts and sending gifts (TDLib
//! 1.8.68, `schema/td_api.tl`). Several of these spend Stars
//! (`sendGift`, `upgradeGift`, `transferGift`); the connect driver only
//! builds them from an explicit confirmation in the UI.

use crate::ids::RequestId;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

fn sender(owner: MessageSender) -> Value {
    match owner {
        MessageSender::User { user_id } => json!({"@type":"messageSenderUser","user_id":user_id}),
        MessageSender::Chat { chat_id } => json!({"@type":"messageSenderChat","chat_id":chat_id}),
    }
}

/// `getReceivedGifts` restricted to a collection (`collection_id` 0 = every
/// gift). Same flags as the plain list.
pub fn get_received_gifts_in(
    extra: RequestId,
    owner: MessageSender,
    collection_id: i32,
    offset: &str,
    limit: i32,
) -> String {
    json!({
        "@type": "getReceivedGifts",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "owner_id": sender(owner),
        "collection_id": collection_id,
        "exclude_unsaved": false,
        "exclude_saved": false,
        "exclude_unlimited": false,
        "exclude_upgradable": false,
        "exclude_non_upgradable": false,
        "exclude_upgraded": false,
        "exclude_without_colors": false,
        "exclude_hosted": false,
        "sort_by_price": false,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `getOption` for one of the gift limits.
pub fn get_option(extra: RequestId, name: &str) -> String {
    json!({"@type":"getOption","@extra":extra.as_extra(),"name":name}).to_string()
}

/// `setPinnedGifts`: the complete new pin order (upgraded, saved gifts only).
pub fn set_pinned_gifts(extra: RequestId, owner: MessageSender, ids: &[String]) -> String {
    json!({
        "@type": "setPinnedGifts",
        "@extra": extra.as_extra(),
        "owner_id": sender(owner),
        "received_gift_ids": ids,
    })
    .to_string()
}

/// `upgradeGift`. `star_count` is 0 when the sender prepaid the upgrade,
/// otherwise the regular gift's `upgrade_star_count`: this spends Stars.
pub fn upgrade_gift(
    extra: RequestId,
    received_gift_id: &str,
    keep_original_details: bool,
    star_count: i64,
) -> String {
    json!({
        "@type": "upgradeGift",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "received_gift_id": received_gift_id,
        "keep_original_details": keep_original_details,
        "star_count": star_count,
    })
    .to_string()
}

/// `transferGift`. `star_count` is the gift's `transfer_star_count` (may be
/// 0): this can spend Stars and gives the gift away.
pub fn transfer_gift(
    extra: RequestId,
    received_gift_id: &str,
    new_owner: MessageSender,
    star_count: i64,
) -> String {
    json!({
        "@type": "transferGift",
        "@extra": extra.as_extra(),
        "business_connection_id": "",
        "received_gift_id": received_gift_id,
        "new_owner_id": sender(new_owner),
        "star_count": star_count,
    })
    .to_string()
}

/// `setGiftResalePrice`: `Some(stars)` lists the collectible for that
/// price, `None` takes it off sale (`price` null).
pub fn set_gift_resale_price(
    extra: RequestId,
    received_gift_id: &str,
    star_count: Option<i64>,
) -> String {
    let price = match star_count {
        Some(star_count) => json!({"@type":"giftResalePriceStar","star_count":star_count}),
        None => Value::Null,
    };
    json!({
        "@type": "setGiftResalePrice",
        "@extra": extra.as_extra(),
        "received_gift_id": received_gift_id,
        "price": price,
    })
    .to_string()
}

pub fn get_gift_collections(extra: RequestId, owner: MessageSender) -> String {
    json!({"@type":"getGiftCollections","@extra":extra.as_extra(),"owner_id":sender(owner)})
        .to_string()
}

pub fn create_gift_collection(
    extra: RequestId,
    owner: MessageSender,
    name: &str,
    ids: &[String],
) -> String {
    json!({
        "@type": "createGiftCollection",
        "@extra": extra.as_extra(),
        "owner_id": sender(owner),
        "name": name,
        "received_gift_ids": ids,
    })
    .to_string()
}

pub fn delete_gift_collection(
    extra: RequestId,
    owner: MessageSender,
    collection_id: i32,
) -> String {
    json!({
        "@type": "deleteGiftCollection",
        "@extra": extra.as_extra(),
        "owner_id": sender(owner),
        "collection_id": collection_id,
    })
    .to_string()
}

pub fn set_gift_collection_name(
    extra: RequestId,
    owner: MessageSender,
    collection_id: i32,
    name: &str,
) -> String {
    json!({
        "@type": "setGiftCollectionName",
        "@extra": extra.as_extra(),
        "owner_id": sender(owner),
        "collection_id": collection_id,
        "name": name,
    })
    .to_string()
}

/// `addGiftCollectionGifts` (`add` = true) or `removeGiftCollectionGifts`.
pub fn change_gift_collection_gifts(
    extra: RequestId,
    owner: MessageSender,
    collection_id: i32,
    ids: &[String],
    add: bool,
) -> String {
    json!({
        "@type": if add { "addGiftCollectionGifts" } else { "removeGiftCollectionGifts" },
        "@extra": extra.as_extra(),
        "owner_id": sender(owner),
        "collection_id": collection_id,
        "received_gift_ids": ids,
    })
    .to_string()
}

pub fn get_available_gifts(extra: RequestId) -> String {
    json!({"@type":"getAvailableGifts","@extra":extra.as_extra()}).to_string()
}

pub fn get_premium_gift_payment_options(extra: RequestId) -> String {
    json!({"@type":"getPremiumGiftPaymentOptions","@extra":extra.as_extra()}).to_string()
}

/// `sendGift`: pays `star_count` (+ the upgrade) from the Stars balance.
/// The text carries no entities (plain text only).
pub fn send_gift(
    extra: RequestId,
    gift_id: i64,
    owner: MessageSender,
    text: &str,
    is_private: bool,
    pay_for_upgrade: bool,
) -> String {
    json!({
        "@type": "sendGift",
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
        "owner_id": sender(owner),
        "text": {"@type": "formattedText", "text": text, "entities": []},
        "is_private": is_private,
        "pay_for_upgrade": pay_for_upgrade,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    const ME: MessageSender = MessageSender::User { user_id: 7 };

    #[test]
    fn collection_filter_sets_the_collection_id() {
        let req = v(&get_received_gifts_in(RequestId(1), ME, 12, "o", 24));
        assert_eq!(req["@type"], "getReceivedGifts");
        assert_eq!(req["collection_id"], 12);
        assert_eq!(req["offset"], "o");
        assert_eq!(req["owner_id"]["user_id"], 7);
    }

    #[test]
    fn pin_order_is_sent_whole() {
        let ids = vec!["a".to_string(), "b".to_string()];
        let req = v(&set_pinned_gifts(RequestId(2), ME, &ids));
        assert_eq!(req["@type"], "setPinnedGifts");
        assert_eq!(req["received_gift_ids"], json!(["a", "b"]));
    }

    #[test]
    fn upgrade_and_transfer_carry_their_star_price() {
        let up = v(&upgrade_gift(RequestId(3), "g", true, 25));
        assert_eq!(up["@type"], "upgradeGift");
        assert_eq!(up["keep_original_details"], true);
        assert_eq!(up["star_count"], 25);
        assert_eq!(up["business_connection_id"], "");
        let tr = v(&transfer_gift(
            RequestId(4),
            "g",
            MessageSender::Chat { chat_id: -100 },
            10,
        ));
        assert_eq!(tr["@type"], "transferGift");
        assert_eq!(tr["new_owner_id"]["chat_id"], -100);
        assert_eq!(tr["star_count"], 10);
    }

    #[test]
    fn resale_price_is_stars_or_null() {
        let on = v(&set_gift_resale_price(RequestId(5), "g", Some(500)));
        assert_eq!(on["price"]["@type"], "giftResalePriceStar");
        assert_eq!(on["price"]["star_count"], 500);
        let off = v(&set_gift_resale_price(RequestId(5), "g", None));
        assert!(off["price"].is_null());
    }

    #[test]
    fn collection_requests() {
        let ids = vec!["a".to_string()];
        let c = v(&create_gift_collection(RequestId(6), ME, "Cats", &ids));
        assert_eq!(c["@type"], "createGiftCollection");
        assert_eq!(c["name"], "Cats");
        assert_eq!(c["received_gift_ids"], json!(["a"]));
        let d = v(&delete_gift_collection(RequestId(7), ME, 3));
        assert_eq!(d["@type"], "deleteGiftCollection");
        assert_eq!(d["collection_id"], 3);
        let r = v(&set_gift_collection_name(RequestId(8), ME, 3, "Dogs"));
        assert_eq!(r["@type"], "setGiftCollectionName");
        assert_eq!(r["name"], "Dogs");
        let add = v(&change_gift_collection_gifts(
            RequestId(9),
            ME,
            3,
            &ids,
            true,
        ));
        assert_eq!(add["@type"], "addGiftCollectionGifts");
        let rm = v(&change_gift_collection_gifts(
            RequestId(9),
            ME,
            3,
            &ids,
            false,
        ));
        assert_eq!(rm["@type"], "removeGiftCollectionGifts");
        assert_eq!(
            v(&get_gift_collections(RequestId(1), ME))["@type"],
            "getGiftCollections"
        );
    }

    #[test]
    fn send_gift_shape() {
        let req = v(&send_gift(RequestId(1), 55, ME, "hi", true, false));
        assert_eq!(req["@type"], "sendGift");
        assert_eq!(req["gift_id"], "55");
        assert_eq!(req["text"]["@type"], "formattedText");
        assert_eq!(req["text"]["text"], "hi");
        assert_eq!(req["is_private"], true);
        assert_eq!(req["pay_for_upgrade"], false);
        assert_eq!(
            v(&get_available_gifts(RequestId(2)))["@type"],
            "getAvailableGifts"
        );
        assert_eq!(
            v(&get_premium_gift_payment_options(RequestId(2)))["@type"],
            "getPremiumGiftPaymentOptions"
        );
    }
}
