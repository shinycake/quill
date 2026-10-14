//! Collectibles on resale and the filters of the resale box (tdesktop
//! `boxes/star_gift_resale_box.cpp`, `ResaleGiftsFilter`).

use super::{array, int32, plural, text};
use crate::premium_hub::upgraded::{Collectible, GiftAttribute, GiftBackdrop, parse_collectible};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Listings per `searchGiftsForResale` page.
pub const RESALE_PAGE_SIZE: i32 = 50;

/// The order of the listings (`GiftForResaleOrder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ResaleSort {
    #[default]
    Price,
    Date,
    Number,
}

impl ResaleSort {
    pub const ALL: [Self; 3] = [Self::Price, Self::Date, Self::Number];

    /// The pill text (`lng_gift_resale_price` / `_date` / `_number`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Price => "Price",
            Self::Date => "Date",
            Self::Number => "Number",
        }
    }

    /// The menu item (`lng_gift_resale_sort_*`).
    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Price => "Sort by Price",
            Self::Date => "Sort by Date",
            Self::Number => "Sort by Number",
        }
    }

    pub fn to_json(self) -> Value {
        let ty = match self {
            Self::Price => "giftForResaleOrderPrice",
            Self::Date => "giftForResaleOrderPriceChangeDate",
            Self::Number => "giftForResaleOrderNumber",
        };
        json!({ "@type": ty })
    }
}

/// Which attribute list a filter tab shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttributeKind {
    Model,
    Backdrop,
    Symbol,
}

impl AttributeKind {
    /// tdesktop's tab order after the sort pill.
    pub const ALL: [Self; 3] = [Self::Model, Self::Backdrop, Self::Symbol];

    /// "Model" with nothing picked, "{n} Models" otherwise
    /// (`lng_gift_resale_model`, `lng_gift_resale_models`).
    pub fn tab_label(self, selected: usize) -> String {
        let (single, one, other) = match self {
            Self::Model => ("Model", "Model", "Models"),
            Self::Backdrop => ("Backdrop", "Backdrop", "Backdrops"),
            Self::Symbol => ("Symbol", "Symbol", "Symbols"),
        };
        if selected == 0 {
            single.to_string()
        } else {
            plural(selected as i64, one, other)
        }
    }
}

/// `UpgradedGiftAttributeId`: what a filter picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttributeId {
    /// A model, by its sticker id.
    Model(i64),
    /// A backdrop, by its id.
    Backdrop(i32),
    /// A symbol, by its sticker id.
    Symbol(i64),
}

impl AttributeId {
    pub fn kind(self) -> AttributeKind {
        match self {
            Self::Model(_) => AttributeKind::Model,
            Self::Backdrop(_) => AttributeKind::Backdrop,
            Self::Symbol(_) => AttributeKind::Symbol,
        }
    }

    pub fn to_json(self) -> Value {
        match self {
            Self::Model(id) => json!({
                "@type": "upgradedGiftAttributeIdModel",
                "sticker_id": id.to_string(),
            }),
            Self::Backdrop(id) => json!({
                "@type": "upgradedGiftAttributeIdBackdrop",
                "backdrop_id": id,
            }),
            Self::Symbol(id) => json!({
                "@type": "upgradedGiftAttributeIdSymbol",
                "sticker_id": id.to_string(),
            }),
        }
    }
}

/// The sort, the payment filter and the picked attributes.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct ResaleFilter {
    pub sort: ResaleSort,
    /// Only listings payable in Stars.
    pub stars_only: bool,
    pub attributes: BTreeSet<AttributeId>,
}

impl ResaleFilter {
    pub fn selected(&self, kind: AttributeKind) -> usize {
        self.attributes
            .iter()
            .filter(|id| id.kind() == kind)
            .count()
    }

    /// tdesktop shows every item checked while none of its kind is picked.
    pub fn is_checked(&self, id: AttributeId) -> bool {
        self.selected(id.kind()) == 0 || self.attributes.contains(&id)
    }

    /// Picks or unpicks one attribute. With none of its kind picked, the
    /// first tap keeps only that one.
    pub fn toggle(&mut self, id: AttributeId) {
        if !self.attributes.remove(&id) {
            self.attributes.insert(id);
        }
    }

    /// "Select All": drops every pick of `kind`.
    pub fn select_all(&mut self, kind: AttributeKind) {
        self.attributes.retain(|id| id.kind() != kind);
    }

    pub fn attributes_json(&self) -> Vec<Value> {
        self.attributes.iter().map(|id| id.to_json()).collect()
    }
}

/// One `giftForResale`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResaleListing {
    pub gift: Collectible,
    /// Set when the listing is the current user's own gift.
    pub received_gift_id: String,
}

impl ResaleListing {
    pub fn is_mine(&self) -> bool {
        !self.received_gift_id.is_empty()
    }
}

/// An attribute and how many listings carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted<T> {
    pub item: T,
    pub count: i32,
}

/// The attribute lists of a collection, most listed first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResaleAttributes {
    pub models: Vec<Counted<GiftAttribute>>,
    pub backdrops: Vec<Counted<GiftBackdrop>>,
    pub symbols: Vec<Counted<GiftAttribute>>,
}

impl ResaleAttributes {
    pub fn is_empty(&self) -> bool {
        self.models.is_empty() && self.backdrops.is_empty() && self.symbols.is_empty()
    }

    /// Every sticker file of the models and symbols, for the downloader.
    pub fn files(&self) -> impl Iterator<Item = &crate::telegram::envelope::ParsedFile> {
        self.models
            .iter()
            .chain(self.symbols.iter())
            .flat_map(|entry| entry.item.files.iter())
    }
}

/// One `giftsForResale` page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResalePage {
    pub total_count: i32,
    pub listings: Vec<ResaleListing>,
    /// Only on the first page of a search without attributes.
    pub attributes: ResaleAttributes,
    pub next_offset: String,
}

pub fn parse_resale_page(value: &Value) -> ResalePage {
    let listings = array(value, "gifts")
        .filter_map(|row| {
            Some(ResaleListing {
                gift: parse_collectible(row.get("gift")?)?,
                received_gift_id: text(row, "received_gift_id"),
            })
        })
        .collect();
    let counted = |key: &str, inner: &str| -> Vec<(Value, i32)> {
        array(value, key)
            .filter_map(|row| Some((row.get(inner)?.clone(), int32(row.get("total_count")))))
            .collect()
    };
    let mut attributes = ResaleAttributes {
        models: counted("models", "model")
            .into_iter()
            .map(|(item, count)| Counted {
                item: GiftAttribute::parse(&item),
                count,
            })
            .collect(),
        backdrops: counted("backdrops", "backdrop")
            .into_iter()
            .map(|(item, count)| Counted {
                item: GiftBackdrop::parse(&item),
                count,
            })
            .collect(),
        symbols: counted("symbols", "symbol")
            .into_iter()
            .map(|(item, count)| Counted {
                item: GiftAttribute::parse(&item),
                count,
            })
            .collect(),
    };
    // tdesktop sorts each list by count, most first, keeping the order of
    // equal counts.
    attributes
        .models
        .sort_by_key(|entry| std::cmp::Reverse(entry.count));
    attributes
        .backdrops
        .sort_by_key(|entry| std::cmp::Reverse(entry.count));
    attributes
        .symbols
        .sort_by_key(|entry| std::cmp::Reverse(entry.count));
    ResalePage {
        total_count: int32(value.get("total_count")).max(0),
        listings,
        attributes,
        next_offset: text(value, "next_offset"),
    }
}

/// "{n} gifts in resale" / "No listings" (`lng_gift_resale_count`).
pub fn resale_count_label(count: i32) -> String {
    if count <= 0 {
        "No listings".into()
    } else {
        plural(i64::from(count), "gift in resale", "gifts in resale")
    }
}

/// "★ 1,250+" on a resale tile: the lowest price, with "+" when more
/// than one is listed.
pub fn resale_tile_price(min_stars: i64, count: i32) -> String {
    let price = crate::premium_hub::group_digits(min_stars.max(0));
    if count > 1 {
        format!("{price}+")
    } else {
        price
    }
}

/// The price a listing is bought at: Grams when asked for or when the
/// seller takes only Grams, else Stars.
pub fn listing_price(
    gift: &Collectible,
    pay_in_grams: bool,
) -> Option<crate::marketplace::GiftPrice> {
    let resale = gift.resale?;
    if pay_in_grams || resale.gram_only {
        (resale.gram_cents > 0)
            .then_some(crate::marketplace::GiftPrice::TonCents(resale.gram_cents))
    } else {
        (resale.stars > 0).then_some(crate::marketplace::GiftPrice::Stars(resale.stars))
    }
}

/// A listing's Stars price, for tiles and the buy button.
pub fn listing_stars(gift: &Collectible) -> i64 {
    gift.resale.map_or(0, |resale| resale.stars)
}
