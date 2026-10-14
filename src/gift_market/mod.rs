//! The gift market: collectibles on resale with their filters, gift
//! auctions and gift crafting, after Telegram Desktop's
//! `boxes/star_gift_resale_box.cpp`, `boxes/star_gift_auction_box.cpp`,
//! `boxes/star_gift_craft_box.cpp` and `data/components/gift_auctions.cpp`.
//!
//! Pure core: the TDLib types, the rules (bid slider bounds, the user's
//! place, craft chances) and the wording. The requests are in
//! `telegram::requests::gift_market`, the driver in `connect::gift_market`,
//! the reducer in `state::domains::payments::gift_market`, the screens in
//! `ui::gift_resale_ui`, `ui::gift_auction_ui` and `ui::gift_craft_ui`.

mod auction;
mod auction_text;
mod craft;
mod prefs;
mod resale;
mod state;
#[cfg(test)]
mod tests;

pub use auction::*;
pub use auction_text::*;
pub use craft::*;
pub use prefs::*;
pub use resale::*;
pub use state::*;

use serde_json::Value;

/// Requests of this feature (in `PaymentsPurpose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftMarketPurpose {
    /// `searchGiftsForResale`; `page` 0 replaces the list, later pages
    /// append. Answer `giftsForResale`.
    SearchResale { gift_id: i64, page: u32 },
    /// `sendResoldGift`. Answer `giftResaleResultOk` /
    /// `giftResaleResultPriceIncreased`.
    BuyResale,
    /// `getGiftAuctionState`. Answer `giftAuctionState`.
    AuctionState { gift_id: i64 },
    /// `openGiftAuction` while an auction box shows. Answer `ok`.
    OpenAuction,
    /// `closeGiftAuction` when it closes. Answer `ok`.
    CloseAuction,
    /// `placeGiftAuctionBid`. Answer `ok`, then `updateGiftAuctionState`.
    PlaceBid { gift_id: i64 },
    /// `increaseGiftAuctionBid`. Answer `ok`.
    IncreaseBid { gift_id: i64 },
    /// `getGiftAuctionAcquiredGifts`. Answer `giftAuctionAcquiredGifts`.
    AcquiredGifts { gift_id: i64 },
    /// `getGiftUpgradePreview` for the auction cover and the craftable
    /// models. Answer `giftUpgradePreview`.
    UpgradePreview { gift_id: i64 },
    /// `getGiftsForCrafting`. Answer `giftsForCrafting`.
    GiftsForCrafting { gift_id: i64, page: u32 },
    /// `craftGift`. Answer `craftGiftResult*`.
    Craft,
}

/// Updates and answers of this feature (in `PaymentsPayload`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftMarketPayload {
    /// `giftsForResale`.
    ResalePage(ResalePage),
    /// `giftAuctionState`, the answer of `getGiftAuctionState`.
    AuctionState(Box<AuctionState>),
    /// `updateGiftAuctionState`.
    AuctionUpdated(Box<AuctionState>),
    /// `updateActiveGiftAuctions`: the auctions the user has a bid in.
    ActiveAuctions(Vec<AuctionState>),
    /// `giftAuctionAcquiredGifts`.
    AcquiredGifts(Vec<AcquiredGift>),
    /// `giftUpgradePreview`.
    UpgradePreview(UpgradePreview),
    /// `giftsForCrafting`.
    CraftCandidates(CraftCandidates),
    /// `craftGiftResult*`.
    CraftResult(CraftResult),
}

/// Parses this feature's TDLib types.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<GiftMarketPayload> {
    match type_name {
        "giftsForResale" => Some(GiftMarketPayload::ResalePage(parse_resale_page(value))),
        "giftAuctionState" => {
            AuctionState::parse(value).map(|state| GiftMarketPayload::AuctionState(Box::new(state)))
        }
        "updateGiftAuctionState" => value
            .get("state")
            .and_then(AuctionState::parse)
            .map(|state| GiftMarketPayload::AuctionUpdated(Box::new(state))),
        "updateActiveGiftAuctions" => Some(GiftMarketPayload::ActiveAuctions(
            array(value, "states")
                .filter_map(AuctionState::parse)
                .collect(),
        )),
        "giftAuctionAcquiredGifts" => Some(GiftMarketPayload::AcquiredGifts(parse_acquired_gifts(
            value,
        ))),
        "giftUpgradePreview" => Some(GiftMarketPayload::UpgradePreview(UpgradePreview::parse(
            value,
        ))),
        "giftsForCrafting" => Some(GiftMarketPayload::CraftCandidates(CraftCandidates::parse(
            value,
        ))),
        "craftGiftResultSuccess"
        | "craftGiftResultTooEarly"
        | "craftGiftResultInvalidGift"
        | "craftGiftResultFail" => CraftResult::parse(value).map(GiftMarketPayload::CraftResult),
        _ => None,
    }
}

/// The toast for a refused request. Quill keeps only the error's class,
/// never TDLib's text (it can carry private data).
pub fn error_note(purpose: GiftMarketPurpose, reason: &str) -> String {
    let what = match purpose {
        GiftMarketPurpose::SearchResale { .. } => "load the gifts on sale",
        GiftMarketPurpose::BuyResale => "buy the gift",
        GiftMarketPurpose::AuctionState { .. } => "load the auction",
        GiftMarketPurpose::OpenAuction | GiftMarketPurpose::CloseAuction => "follow the auction",
        GiftMarketPurpose::PlaceBid { .. } => "place the bid",
        GiftMarketPurpose::IncreaseBid { .. } => "raise the bid",
        GiftMarketPurpose::AcquiredGifts { .. } => "load the bought gifts",
        GiftMarketPurpose::UpgradePreview { .. } => "load the gift variants",
        GiftMarketPurpose::GiftsForCrafting { .. } => "load the gifts for crafting",
        GiftMarketPurpose::Craft => "craft the gift",
    };
    format!("Couldn't {what}: {reason}.")
}

fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn int32(value: Option<&Value>) -> i32 {
    i32::try_from(int(value)).unwrap_or(0)
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn array<'a>(value: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// "1 Model" / "5 Models": tdesktop's `#one` / `#other` pair.
fn plural(count: i64, one: &str, other: &str) -> String {
    let n = crate::premium_hub::group_digits(count);
    if count == 1 {
        format!("{n} {one}")
    } else {
        format!("{n} {other}")
    }
}
