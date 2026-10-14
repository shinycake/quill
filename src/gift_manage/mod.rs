//! Managing received gifts (pin, collections, upgrade, transfer, resale)
//! after Telegram Desktop's `info/peer_gifts/*` and the gift view box in
//! `boxes/star_gift_box.cpp`.
//!
//! Pure core: the extra `receivedGift` fields, the gating rules for each
//! owner action, the collection and limit types, and the parser for the
//! answers. The requests are in `telegram::requests_gifts`, the driver in
//! `connect::gift_manage`, the reducer in `state::domains::payments`.
#![deny(clippy::indexing_slicing)]

mod send;
mod state;
#[cfg(test)]
mod tests;

use crate::premium_hub::ReceivedGift;
use crate::telegram::envelope::{ParsedFile, StickerContent};
use serde_json::Value;

pub use send::*;
pub use state::*;

fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
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

/// The `receivedGift` fields the owner actions need.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GiftManageInfo {
    pub can_be_transferred: bool,
    pub collection_ids: Vec<i32>,
    /// Stars the sender prepaid for the upgrade (the receiver upgrades free).
    pub prepaid_upgrade_star_count: i64,
    pub transfer_star_count: i64,
    pub next_transfer_date: i32,
    pub next_resale_date: i32,
    /// `gift.upgrade_star_count` of a regular gift.
    pub upgrade_star_count: i64,
    /// A collectible listed for resale: its Star price (`resale_parameters`).
    pub resale_star_count: i64,
    /// Listed for TON only (`resale_parameters.gram_only`).
    pub resale_gram_only: bool,
    pub listed: bool,
    /// Not usable for crafting before this Unix time (`craft_date`).
    pub craft_date: i32,
}

impl GiftManageInfo {
    /// Read the fields off one `receivedGift` object.
    pub fn parse(received: &Value) -> Self {
        let inner = received.get("gift").and_then(|g| g.get("gift"));
        let resale = inner.and_then(|g| g.get("resale_parameters"));
        Self {
            can_be_transferred: flag(received, "can_be_transferred"),
            collection_ids: received
                .get("collection_ids")
                .and_then(Value::as_array)
                .map(|ids| ids.iter().map(|id| int(Some(id)) as i32).collect())
                .unwrap_or_default(),
            prepaid_upgrade_star_count: int(received.get("prepaid_upgrade_star_count")),
            transfer_star_count: int(received.get("transfer_star_count")),
            next_transfer_date: int(received.get("next_transfer_date")) as i32,
            next_resale_date: int(received.get("next_resale_date")) as i32,
            upgrade_star_count: int(inner.and_then(|g| g.get("upgrade_star_count"))),
            resale_star_count: int(resale.and_then(|r| r.get("star_count"))),
            resale_gram_only: resale.is_some_and(|r| flag(r, "gram_only")),
            listed: resale.is_some_and(|r| !r.is_null()),
            craft_date: int(received.get("craft_date")) as i32,
        }
    }
}

/// Whether the Pin / Unpin button is offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinGate {
    Pin,
    Unpin,
    /// Every pin slot is taken: unpin another gift first.
    Full,
    Unavailable,
}

/// Transfer availability of a collectible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferGate {
    Unavailable,
    /// Not before this Unix time.
    Later(i32),
    Ready {
        star_count: i64,
    },
}

/// Resale availability of a collectible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResaleGate {
    Unavailable,
    Later(i32),
    /// The listing is TON-only; Quill sets Star prices only.
    TonOnly,
    Ready,
}

impl ReceivedGift {
    /// Pin or unpin on the profile. Only upgraded gifts that are shown on the
    /// profile can be pinned (`setPinnedGifts`), and only in the unfiltered
    /// list (the caller passes `pinned` = pinned gifts in that list).
    pub fn pin_gate(&self, mine: bool, pinned: usize, limit: i64) -> PinGate {
        if !mine || !self.gift.upgraded || self.was_refunded {
            return PinGate::Unavailable;
        }
        if self.is_pinned {
            return PinGate::Unpin;
        }
        if !self.is_saved {
            return PinGate::Unavailable;
        }
        if (pinned as i64) >= limit {
            PinGate::Full
        } else {
            PinGate::Pin
        }
    }

    /// Stars the upgrade costs the owner: `Some(0)` when the sender prepaid
    /// it, `None` when the gift cannot be upgraded.
    pub fn upgrade_cost(&self, mine: bool) -> Option<i64> {
        if !mine || self.gift.upgraded || !self.can_be_upgraded || self.was_refunded {
            return None;
        }
        if self.manage.prepaid_upgrade_star_count > 0 {
            Some(0)
        } else if self.manage.upgrade_star_count > 0 {
            Some(self.manage.upgrade_star_count)
        } else {
            None
        }
    }

    pub fn transfer_gate(&self, mine: bool, now: i64) -> TransferGate {
        if !mine || !self.gift.upgraded || !self.manage.can_be_transferred || self.was_refunded {
            return TransferGate::Unavailable;
        }
        let date = self.manage.next_transfer_date;
        if date > 0 && i64::from(date) > now {
            TransferGate::Later(date)
        } else {
            TransferGate::Ready {
                star_count: self.manage.transfer_star_count.max(0),
            }
        }
    }

    pub fn resale_gate(&self, mine: bool, now: i64) -> ResaleGate {
        if !mine
            || !self.gift.upgraded
            || self.was_refunded
            || (self.manage.next_resale_date == 0 && !self.manage.listed)
        {
            return ResaleGate::Unavailable;
        }
        if self.manage.listed && self.manage.resale_gram_only {
            return ResaleGate::TonOnly;
        }
        let date = self.manage.next_resale_date;
        if date > 0 && i64::from(date) > now && !self.manage.listed {
            ResaleGate::Later(date)
        } else {
            ResaleGate::Ready
        }
    }

    pub fn in_collection(&self, collection_id: i32) -> bool {
        self.manage.collection_ids.contains(&collection_id)
    }
}

/// Ids of the pinned gifts of an unfiltered list, in list order.
pub fn pinned_ids(gifts: &[ReceivedGift]) -> Vec<String> {
    gifts
        .iter()
        .filter(|g| g.is_pinned)
        .map(|g| g.id.clone())
        .collect()
}

/// The new pin order after pinning `id`: it joins the end of the pins.
pub fn pins_with(current: &[String], id: &str) -> Vec<String> {
    let mut out: Vec<String> = current.iter().filter(|x| *x != id).cloned().collect();
    out.push(id.to_string());
    out
}

/// The new pin order after unpinning `id`.
pub fn pins_without(current: &[String], id: &str) -> Vec<String> {
    current.iter().filter(|x| *x != id).cloned().collect()
}

/// tdesktop's defaults (`AppConfig`) while TDLib's options are unknown.
pub const DEFAULT_PINNED_MAX: i64 = 6;
pub const DEFAULT_COLLECTIONS_MAX: i64 = 10;
pub const DEFAULT_COLLECTION_SIZE_MAX: i64 = 500;
pub const DEFAULT_RESALE_STARS_MIN: i64 = 125;
pub const DEFAULT_RESALE_STARS_MAX: i64 = 35_000;
/// `createGiftCollection` / `setGiftCollectionName`: 1-12 characters.
pub const COLLECTION_NAME_MAX: usize = 12;

/// `getOption` names for the gift limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftOption {
    PinnedMax,
    CollectionsMax,
    CollectionSizeMax,
    ResaleStarsMin,
    ResaleStarsMax,
}

impl GiftOption {
    pub const ALL: [GiftOption; 5] = [
        Self::PinnedMax,
        Self::CollectionsMax,
        Self::CollectionSizeMax,
        Self::ResaleStarsMin,
        Self::ResaleStarsMax,
    ];

    /// The TDLib option name.
    pub fn name(self) -> &'static str {
        match self {
            Self::PinnedMax => "pinned_gift_count_max",
            Self::CollectionsMax => "gift_collection_count_max",
            Self::CollectionSizeMax => "gift_collection_size_max",
            Self::ResaleStarsMin => "gift_resale_star_count_min",
            Self::ResaleStarsMax => "gift_resale_star_count_max",
        }
    }
}

/// The gift limits TDLib reports, `None` until the option arrives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GiftLimits {
    pub pinned_max: Option<i64>,
    pub collections_max: Option<i64>,
    pub collection_size_max: Option<i64>,
    pub resale_stars_min: Option<i64>,
    pub resale_stars_max: Option<i64>,
}

impl GiftLimits {
    pub fn set(&mut self, option: GiftOption, value: i64) {
        let slot = match option {
            GiftOption::PinnedMax => &mut self.pinned_max,
            GiftOption::CollectionsMax => &mut self.collections_max,
            GiftOption::CollectionSizeMax => &mut self.collection_size_max,
            GiftOption::ResaleStarsMin => &mut self.resale_stars_min,
            GiftOption::ResaleStarsMax => &mut self.resale_stars_max,
        };
        *slot = Some(value);
    }

    pub fn pinned(&self) -> i64 {
        self.pinned_max.unwrap_or(DEFAULT_PINNED_MAX)
    }

    pub fn collections(&self) -> i64 {
        self.collections_max.unwrap_or(DEFAULT_COLLECTIONS_MAX)
    }

    /// The inclusive resale price range in Stars.
    pub fn resale_range(&self) -> (i64, i64) {
        (
            self.resale_stars_min.unwrap_or(DEFAULT_RESALE_STARS_MIN),
            self.resale_stars_max.unwrap_or(DEFAULT_RESALE_STARS_MAX),
        )
    }

    /// Whether `stars` is an allowed resale price.
    pub fn resale_price_ok(&self, stars: i64) -> bool {
        let (min, max) = self.resale_range();
        (min..=max).contains(&stars)
    }
}

/// Parse a resale price typed by the user ("1 250", "1,250") into Stars.
pub fn parse_stars_input(input: &str) -> Option<i64> {
    let digits: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_')
        .collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A collection name is 1-12 characters after trimming.
pub fn valid_collection_name(name: &str) -> bool {
    let n = name.trim().chars().count();
    (1..=COLLECTION_NAME_MAX).contains(&n)
}

/// One `giftCollection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftCollection {
    pub id: i32,
    pub name: String,
    pub icon: Option<StickerContent>,
    pub files: Vec<ParsedFile>,
    pub gift_count: i32,
}

pub fn parse_gift_collection(value: &Value) -> Option<GiftCollection> {
    let id = int(value.get("id")) as i32;
    if id <= 0 {
        return None;
    }
    let (icon, files) = crate::premium_hub::parse_gift_sticker(value.get("icon"));
    Some(GiftCollection {
        id,
        name: text(value, "name"),
        icon,
        files,
        gift_count: int(value.get("gift_count")) as i32,
    })
}

pub fn parse_gift_collections(value: &Value) -> Vec<GiftCollection> {
    value
        .get("collections")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().filter_map(parse_gift_collection).collect())
        .unwrap_or_default()
}

/// What `upgradeGift` returned: the collectible's name for the toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeSummary {
    pub title: String,
    pub number: i32,
}

/// Answers of this feature, wrapped as `PaymentsPayload::GiftManage`.
#[derive(Debug, Clone, PartialEq)]
pub enum GiftManagePayload {
    Collections(Vec<GiftCollection>),
    Collection(Option<GiftCollection>),
    Upgraded(UpgradeSummary),
    Available(Vec<AvailableGift>),
    PremiumOptions(Vec<PremiumGiftOption>),
}

/// Parse one of this feature's answer types; `None` = not ours.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<GiftManagePayload> {
    Some(match type_name {
        "giftCollections" => GiftManagePayload::Collections(parse_gift_collections(value)),
        "giftCollection" => GiftManagePayload::Collection(parse_gift_collection(value)),
        "upgradeGiftResult" => {
            let gift = value.get("gift");
            GiftManagePayload::Upgraded(UpgradeSummary {
                title: gift.map(|g| text(g, "title")).unwrap_or_default(),
                number: int(gift.and_then(|g| g.get("number"))) as i32,
            })
        }
        "availableGifts" => GiftManagePayload::Available(parse_available_gifts(value)),
        "premiumGiftPaymentOptions" => {
            GiftManagePayload::PremiumOptions(parse_premium_gift_options(value))
        }
        _ => return None,
    })
}
