//! Requests for the gift market (`crate::gift_market`): resale search,
//! auctions and crafting. `int64` gift ids go as strings.
use crate::gift_market::{CRAFT_PAGE_SIZE, RESALE_PAGE_SIZE, ResaleFilter};
use crate::ids::RequestId;
use serde_json::json;

/// `searchGiftsForResale gift_id order for_crafting for_stars attributes
/// offset limit`. Answer `giftsForResale`.
pub fn search_gifts_for_resale(
    extra: RequestId,
    gift_id: i64,
    filter: &ResaleFilter,
    for_crafting: bool,
    offset: &str,
) -> String {
    json!({
        "@type": "searchGiftsForResale",
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
        "order": filter.sort.to_json(),
        "for_crafting": for_crafting,
        "for_stars": filter.stars_only,
        "attributes": filter.attributes_json(),
        "offset": offset,
        "limit": RESALE_PAGE_SIZE,
    })
    .to_string()
}

/// `getGiftAuctionState auction_id`. Answer `giftAuctionState`.
pub fn get_gift_auction_state(extra: RequestId, auction_id: &str) -> String {
    json!({
        "@type": "getGiftAuctionState",
        "@extra": extra.as_extra(),
        "auction_id": auction_id,
    })
    .to_string()
}

/// `openGiftAuction gift_id` / `closeGiftAuction gift_id`: follow the
/// auction's updates while a box shows it. Answer `ok`.
pub fn follow_gift_auction(extra: RequestId, gift_id: i64, open: bool) -> String {
    json!({
        "@type": if open { "openGiftAuction" } else { "closeGiftAuction" },
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
    })
    .to_string()
}

/// `placeGiftAuctionBid gift_id star_count user_id text is_private`.
/// Answer `ok`.
pub fn place_gift_auction_bid(
    extra: RequestId,
    gift_id: i64,
    star_count: i64,
    user_id: i64,
    text: &str,
    is_private: bool,
) -> String {
    json!({
        "@type": "placeGiftAuctionBid",
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
        "star_count": star_count,
        "user_id": user_id,
        "text": {"@type": "formattedText", "text": text, "entities": []},
        "is_private": is_private,
    })
    .to_string()
}

/// `increaseGiftAuctionBid gift_id star_count`: `star_count` is the new
/// total. Answer `ok`.
pub fn increase_gift_auction_bid(extra: RequestId, gift_id: i64, star_count: i64) -> String {
    json!({
        "@type": "increaseGiftAuctionBid",
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
        "star_count": star_count,
    })
    .to_string()
}

/// `getGiftAuctionAcquiredGifts gift_id`. Answer `giftAuctionAcquiredGifts`.
pub fn get_gift_auction_acquired_gifts(extra: RequestId, gift_id: i64) -> String {
    json!({
        "@type": "getGiftAuctionAcquiredGifts",
        "@extra": extra.as_extra(),
        "gift_id": gift_id.to_string(),
    })
    .to_string()
}

/// `getGiftUpgradePreview regular_gift_id`. Answer `giftUpgradePreview`.
pub fn get_gift_upgrade_preview(extra: RequestId, gift_id: i64) -> String {
    json!({
        "@type": "getGiftUpgradePreview",
        "@extra": extra.as_extra(),
        "regular_gift_id": gift_id.to_string(),
    })
    .to_string()
}

/// `getGiftsForCrafting regular_gift_id offset limit`. Answer
/// `giftsForCrafting`.
pub fn get_gifts_for_crafting(extra: RequestId, gift_id: i64, offset: &str) -> String {
    json!({
        "@type": "getGiftsForCrafting",
        "@extra": extra.as_extra(),
        "regular_gift_id": gift_id.to_string(),
        "offset": offset,
        "limit": CRAFT_PAGE_SIZE,
    })
    .to_string()
}

/// `craftGift received_gift_ids`. Answer `craftGiftResult*`.
pub fn craft_gift(extra: RequestId, received_gift_ids: &[String]) -> String {
    json!({
        "@type": "craftGift",
        "@extra": extra.as_extra(),
        "received_gift_ids": received_gift_ids,
    })
    .to_string()
}
