//! The collectible side of an `upgradedGift`: its model, symbol and
//! backdrop with their rarity, the craft chance and the resale listing
//! (tdesktop `Data::UniqueGift`, `data/data_star_gift.h`). Shared by the
//! received-gifts grid, the resale market and crafting (`crate::gift_market`).

use super::{flag, group_digits, int, sender_of, sticker_content, text_of};
use crate::telegram::envelope::{MessageSender, ParsedFile, StickerContent};
use serde_json::Value;

/// `UpgradedGiftAttributeRarity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rarity {
    /// How many of every 1000 upgraded gifts get the attribute; 0 reads
    /// "< 0.1%".
    PerMille(i32),
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub fn parse(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::PerMille(0);
        };
        match value.get("@type").and_then(Value::as_str) {
            Some("upgradedGiftAttributeRarityUncommon") => Self::Uncommon,
            Some("upgradedGiftAttributeRarityRare") => Self::Rare,
            Some("upgradedGiftAttributeRarityEpic") => Self::Epic,
            Some("upgradedGiftAttributeRarityLegendary") => Self::Legendary,
            _ => Self::PerMille(int(value.get("per_mille")).clamp(0, 1000) as i32),
        }
    }

    /// tdesktop `UniqueGiftAttributeHasSpecialRarity`: a named rarity
    /// instead of a share.
    pub fn is_special(self) -> bool {
        !matches!(self, Self::PerMille(_))
    }

    /// tdesktop `UniqueGiftAttributeText`: "1.5%", "< 0.1%" or the tag
    /// ("uncommon", "rare", "epic", "legendary").
    pub fn label(self) -> String {
        match self {
            Self::PerMille(0) => "< 0.1%".into(),
            Self::PerMille(n) if n % 10 == 0 => format!("{}%", n / 10),
            Self::PerMille(n) => format!("{}.{}%", n / 10, n % 10),
            Self::Uncommon => "uncommon".into(),
            Self::Rare => "rare".into(),
            Self::Epic => "epic".into(),
            Self::Legendary => "legendary".into(),
        }
    }
}

/// A model or a symbol: its name, sticker and rarity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftAttribute {
    pub name: String,
    /// `sticker.id`, the id `upgradedGiftAttributeIdModel` /
    /// `upgradedGiftAttributeIdSymbol` filter by.
    pub sticker_id: i64,
    pub sticker: Option<StickerContent>,
    pub files: Vec<ParsedFile>,
    pub rarity: Rarity,
    /// `upgradedGiftModel.is_crafted`: only obtainable by crafting.
    pub is_crafted: bool,
}

impl GiftAttribute {
    pub fn parse(value: &Value) -> Self {
        let sticker = value.get("sticker");
        let (content, files) = sticker_content(sticker);
        Self {
            name: text_of(value, "name"),
            sticker_id: int(sticker.and_then(|s| s.get("id"))),
            sticker: content,
            files,
            rarity: Rarity::parse(value.get("rarity")),
            is_crafted: flag(value, "is_crafted"),
        }
    }
}

/// `upgradedGiftBackdrop`: colors are 0xRRGGBB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftBackdrop {
    pub id: i32,
    pub name: String,
    pub center: u32,
    pub edge: u32,
    pub symbol: u32,
    pub text: u32,
    pub rarity: Rarity,
}

impl GiftBackdrop {
    pub fn parse(value: &Value) -> Self {
        let colors = value.get("colors");
        let color = |key: &str| (int(colors.and_then(|c| c.get(key))) as u32) & 0x00FF_FFFF;
        Self {
            id: int(value.get("id")) as i32,
            name: text_of(value, "name"),
            center: color("center_color"),
            edge: color("edge_color"),
            symbol: color("symbol_color"),
            text: color("text_color"),
            rarity: Rarity::parse(value.get("rarity")),
        }
    }
}

/// What a collectible is made of, beyond `GiftInfo`'s title and sticker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collectible {
    pub id: i64,
    /// The regular gift it was upgraded from (the resale and craft lists
    /// are per regular gift).
    pub regular_gift_id: i64,
    /// The slug in `t.me/nft/<name>`.
    pub name: String,
    /// "Plush Pepe" without the number.
    pub title: String,
    pub number: i32,
    pub model: GiftAttribute,
    pub symbol: GiftAttribute,
    pub backdrop: GiftBackdrop,
    pub owner: Option<MessageSender>,
    /// Non-empty once exported to the blockchain.
    pub gift_address: String,
    /// The chance this gift adds to a craft, per mille; 0 = not craftable.
    pub craft_per_mille: i32,
    pub is_crafted: bool,
    pub is_burned: bool,
    /// The resale listing: Stars and 1/100 Gram prices, and whether the
    /// seller takes Grams only. `None` when not on sale.
    pub resale: Option<ResaleListingPrice>,
    /// The estimated value (`value_currency`, `value_amount` in the
    /// currency's smallest units); the unique gift box shows its "Value"
    /// row only when both are set (`crate::premium_hub::gift_value`).
    pub value_currency: String,
    pub value_amount: i64,
}

/// `giftResaleParameters`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResaleListingPrice {
    pub stars: i64,
    pub gram_cents: i64,
    pub gram_only: bool,
}

impl Collectible {
    /// "Plush Pepe #1,234" (tdesktop `UniqueGiftName`).
    pub fn full_name(&self) -> String {
        format!("{} #{}", self.title, group_digits(i64::from(self.number)))
    }

    /// Every file of the model and symbol stickers.
    pub fn files(&self) -> impl Iterator<Item = &ParsedFile> {
        self.model.files.iter().chain(self.symbol.files.iter())
    }
}

/// Parse an `upgradedGift`; `None` for any other object.
pub fn parse_collectible(gift: &Value) -> Option<Collectible> {
    if gift.get("@type").and_then(Value::as_str) != Some("upgradedGift") {
        return None;
    }
    let resale = gift
        .get("resale_parameters")
        .filter(|r| !r.is_null())
        .map(|r| ResaleListingPrice {
            stars: int(r.get("star_count")).max(0),
            gram_cents: int(r.get("gram_cent_count")).max(0),
            gram_only: flag(r, "gram_only"),
        });
    Some(Collectible {
        id: int(gift.get("id")),
        regular_gift_id: int(gift.get("regular_gift_id")),
        name: text_of(gift, "name"),
        title: text_of(gift, "title"),
        number: int(gift.get("number")) as i32,
        model: gift
            .get("model")
            .map(GiftAttribute::parse)
            .unwrap_or_else(|| GiftAttribute::parse(&Value::Null)),
        symbol: gift
            .get("symbol")
            .map(GiftAttribute::parse)
            .unwrap_or_else(|| GiftAttribute::parse(&Value::Null)),
        backdrop: GiftBackdrop::parse(gift.get("backdrop").unwrap_or(&Value::Null)),
        owner: sender_of(gift.get("owner_id")),
        gift_address: text_of(gift, "gift_address"),
        craft_per_mille: int(gift.get("craft_probability_per_mille")).clamp(0, 1000) as i32,
        is_crafted: flag(gift, "is_crafted"),
        is_burned: flag(gift, "is_burned"),
        resale,
        value_currency: text_of(gift, "value_currency"),
        value_amount: int(gift.get("value_amount")).max(0),
    })
}
