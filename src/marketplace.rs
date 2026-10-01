//! Collectible gift quotes and explicit, price-bound Marketplace purchases.
use crate::ids::ChatId;
use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftPrice {
    Stars(i64),
    TonCents(i64),
}

impl GiftPrice {
    pub fn valid(self) -> bool {
        let amount = match self {
            Self::Stars(n) | Self::TonCents(n) => n,
        };
        (1..=9_007_199_254_740_991).contains(&amount)
    }
    pub fn json(self) -> Value {
        match self {
            Self::Stars(n) => json!({"@type":"giftResalePriceStar","star_count":n}),
            Self::TonCents(n) => json!({"@type":"giftResalePriceGram","gram_cent_count":n}),
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::Stars(n) => format!("{n} Stars"),
            Self::TonCents(n) => format!("{}.{:02} TON", n / 100, n % 100),
        }
    }
    pub fn parse(v: &Value) -> Option<Self> {
        let integer = |key| {
            v.get(key)
                .and_then(|n| n.as_i64().or_else(|| n.as_str()?.parse().ok()))
        };
        let price = match v.get("@type")?.as_str()? {
            "giftResalePriceStar" => Self::Stars(integer("star_count")?),
            "giftResalePriceGram" => Self::TonCents(integer("gram_cent_count")?),
            _ => return None,
        };
        price.valid().then_some(price)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftQuote {
    pub name: String,
    pub title: String,
    pub stars: Option<GiftPrice>,
    pub ton: Option<GiftPrice>,
}
impl GiftQuote {
    pub fn parse(v: &Value) -> Option<Self> {
        let name = v.get("name")?.as_str()?.to_string();
        if !valid_gift_name(&name) {
            return None;
        }
        let resale = v.get("resale_parameters");
        let number = |key| {
            resale
                .and_then(|r| r.get(key))
                .and_then(|n| n.as_i64().or_else(|| n.as_str()?.parse().ok()))
                .unwrap_or(0)
        };
        let stars = GiftPrice::Stars(number("star_count"));
        let ton = GiftPrice::TonCents(number("gram_cent_count"));
        Some(Self {
            name,
            title: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Collectible gift")
                .to_string(),
            stars: (stars.valid()
                && !resale
                    .and_then(|r| r.get("gram_only"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false))
            .then_some(stars),
            ton: ton.valid().then_some(ton),
        })
    }
    pub fn allows(&self, price: GiftPrice) -> bool {
        self.stars == Some(price) || self.ton == Some(price)
    }
}

pub fn valid_gift_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftPurchaseResult {
    Sent(String),
    PriceIncreased(Option<GiftPrice>),
}

#[derive(Debug, Clone)]
pub struct GiftPurchase {
    pub chat_id: ChatId,
    pub recipient: MessageSender,
    pub recipient_name: String,
    pub requested_name: String,
    pub quote: Option<GiftQuote>,
    pub price: Option<GiftPrice>,
    pub loading: bool,
    pub sending: bool,
    pub completed: bool,
    pub note: Option<String>,
}
