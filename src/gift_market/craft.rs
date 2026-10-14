//! Gift crafting: up to four collectibles of one collection are burnt for
//! a chance at a rarer one (tdesktop `boxes/star_gift_craft_box.cpp`).

use super::{array, int32, plural, text};
use crate::premium_hub::upgraded::{Collectible, GiftAttribute, GiftBackdrop, parse_collectible};
use serde_json::Value;

/// Gifts one craft takes.
pub const CRAFT_SLOTS: usize = 4;
/// Attribute chips under the slots.
pub const CRAFT_CHIPS: usize = 8;
/// Gifts per `getGiftsForCrafting` page.
pub const CRAFT_PAGE_SIZE: i32 = 50;

/// A collectible the user can put in a craft slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CraftGift {
    pub gift: Collectible,
    pub received_gift_id: String,
    /// Not usable before this Unix time; 0 = now.
    pub craft_date: i32,
}

impl CraftGift {
    /// A `receivedGift` holding a collectible.
    pub fn parse_received(value: &Value) -> Option<Self> {
        let gift = parse_collectible(value.get("gift")?.get("gift")?)?;
        let received_gift_id = text(value, "received_gift_id");
        if received_gift_id.is_empty() {
            return None;
        }
        Some(Self {
            gift,
            received_gift_id,
            craft_date: int32(value.get("craft_date")),
        })
    }

    /// Why this gift can't go in a slot (tdesktop `ShowCraftLaterError`,
    /// `ShowCraftAddressError` for the first slot).
    pub fn gate(&self, first_slot: bool, now: i64) -> CraftGate {
        if self.gift.craft_per_mille <= 0 {
            CraftGate::Unavailable
        } else if i64::from(self.craft_date) > now {
            CraftGate::Later(self.craft_date)
        } else if first_slot && !self.gift.gift_address.is_empty() {
            CraftGate::Exported
        } else {
            CraftGate::Ready
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraftGate {
    Ready,
    /// The collectible can't be crafted at all.
    Unavailable,
    /// "Crafting unavailable": not before this Unix time.
    Later(i32),
    /// Exported to the blockchain; can't be the primary gift.
    Exported,
}

impl CraftGate {
    /// The error box text; `when` formats a Unix time as (date, time).
    pub fn message(self, when: impl Fn(i32) -> (String, String)) -> Option<String> {
        match self {
            Self::Ready | Self::Unavailable => None,
            Self::Later(date) => {
                let (day, time) = when(date);
                Some(format!(
                    "This gift will become available for crafting on {day} at {time}."
                ))
            }
            Self::Exported => Some(
                "This gift has been exported to the blockchain and can't be used as the primary gift for crafting."
                    .into(),
            ),
        }
    }
}

/// `giftsForCrafting`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CraftCandidates {
    pub total_count: i32,
    pub gifts: Vec<CraftGift>,
    /// `attribute_persistence_probabilities`: row `n - 1` holds the
    /// per-mille chances for an attribute carried by 1..=n of `n` gifts.
    pub probabilities: Vec<Vec<i32>>,
    pub next_offset: String,
}

impl CraftCandidates {
    pub fn parse(value: &Value) -> Self {
        Self {
            total_count: int32(value.get("total_count")).max(0),
            gifts: array(value, "gifts")
                .filter_map(CraftGift::parse_received)
                .collect(),
            probabilities: array(value, "attribute_persistence_probabilities")
                .map(|row| {
                    array(row, "persistence_chance_per_mille")
                        .map(|n| int32(Some(n)).clamp(0, 1000))
                        .collect()
                })
                .collect(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// `craftGiftResult*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CraftResult {
    Success {
        gift: Box<Collectible>,
        received_gift_id: String,
    },
    /// Try again in this many seconds.
    TooEarly(i32),
    InvalidGift,
    Fail,
}

impl CraftResult {
    pub fn parse(value: &Value) -> Option<Self> {
        Some(match value.get("@type").and_then(Value::as_str)? {
            "craftGiftResultSuccess" => Self::Success {
                gift: Box::new(parse_collectible(value.get("gift")?)?),
                received_gift_id: text(value, "received_gift_id"),
            },
            "craftGiftResultTooEarly" => Self::TooEarly(int32(value.get("retry_after")).max(0)),
            "craftGiftResultInvalidGift" => Self::InvalidGift,
            "craftGiftResultFail" => Self::Fail,
            _ => return None,
        })
    }
}

/// tdesktop `FormatPercent`: per mille rounded to a whole percent.
pub fn format_percent(per_mille: i32) -> String {
    format!("{}%", (per_mille.max(0) + 5) / 10)
}

/// The chance the craft succeeds: the sum of the gifts' chances.
pub fn success_per_mille(slots: &[CraftGift]) -> i32 {
    slots
        .iter()
        .map(|slot| slot.gift.craft_per_mille.max(0))
        .fold(0, i32::saturating_add)
}

/// What a chip under the slots stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChanceAttribute {
    Backdrop(GiftBackdrop),
    Symbol(GiftAttribute),
}

/// One attribute chip: the chance the crafted gift keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributeChance {
    pub attribute: ChanceAttribute,
    pub per_mille: i32,
}

impl AttributeChance {
    pub fn name(&self) -> &str {
        match &self.attribute {
            ChanceAttribute::Backdrop(backdrop) => &backdrop.name,
            ChanceAttribute::Symbol(symbol) => &symbol.name,
        }
    }

    /// "40% chance the crafted gift will have Onyx background."
    pub fn tooltip(&self) -> String {
        let kind = match self.attribute {
            ChanceAttribute::Backdrop(_) => "background",
            ChanceAttribute::Symbol(_) => "symbol",
        };
        format!(
            "{} chance the crafted gift will have {} {kind}.",
            format_percent(self.per_mille),
            self.name()
        )
    }
}

/// tdesktop `MakeRarityExpectancyPreview`: backdrops then symbols, each
/// most-shared first, at most eight. `table[total - 1][count - 1]` is the
/// chance; 1000 when the table is short.
pub fn attribute_chances(slots: &[CraftGift], table: &[Vec<i32>]) -> Vec<AttributeChance> {
    let total = slots.len();
    if total == 0 {
        return Vec::new();
    }
    let per_mille = |count: usize| -> i32 {
        table
            .get(total - 1)
            .and_then(|row| row.get(count.saturating_sub(1)))
            .copied()
            .unwrap_or(1000)
    };
    let mut backdrops: Vec<(GiftBackdrop, usize)> = Vec::new();
    let mut symbols: Vec<(GiftAttribute, usize)> = Vec::new();
    for slot in slots {
        let backdrop = &slot.gift.backdrop;
        match backdrops.iter_mut().find(|(b, _)| b.id == backdrop.id) {
            Some(entry) => entry.1 += 1,
            None => backdrops.push((backdrop.clone(), 1)),
        }
        let symbol = &slot.gift.symbol;
        match symbols
            .iter_mut()
            .find(|(s, _)| s.sticker_id == symbol.sticker_id)
        {
            Some(entry) => entry.1 += 1,
            None => symbols.push((symbol.clone(), 1)),
        }
    }
    backdrops.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    symbols.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    backdrops
        .into_iter()
        .map(|(b, count)| AttributeChance {
            attribute: ChanceAttribute::Backdrop(b),
            per_mille: per_mille(count),
        })
        .chain(symbols.into_iter().map(|(s, count)| AttributeChance {
            attribute: ChanceAttribute::Symbol(s),
            per_mille: per_mille(count),
        }))
        .take(CRAFT_CHIPS)
        .collect()
}

/// tdesktop shows "View all craftable models" while the chips are few.
pub fn shows_craftable_link(chances: &[AttributeChance]) -> bool {
    chances.len() <= 5
}

/// The craft box colors tdesktop hardcodes by slot count: background
/// center, edge, and the button's gradient (0xRRGGBB).
pub fn craft_colors(count: usize) -> (u32, u32, u32, u32) {
    if count >= CRAFT_SLOTS {
        (0x1C4843, 0x1A2E37, 0x3ACA49, 0x007D9E)
    } else {
        (0x2C4359, 0x232E3F, 0x10A5DF, 0x2091E9)
    }
}

/// "This crafting attempt was unsuccessful.\n2 gifts were lost."
pub fn craft_failed_text(count: usize) -> String {
    format!(
        "This crafting attempt was unsuccessful.\n{} lost.",
        plural(count as i64, "gift was", "gifts were")
    )
}

/// "{n} suitable gifts on sale" (`lng_gift_craft_select_market`).
pub fn suitable_on_sale(count: i32) -> String {
    plural(
        i64::from(count),
        "suitable gift on sale",
        "suitable gifts on sale",
    )
}
