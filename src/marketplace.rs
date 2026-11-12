//! Collectible gift quotes and explicit, price-bound Marketplace purchases.
#![deny(clippy::indexing_slicing)]
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

/// Where a Marketplace purchase stands. Each stage leaves by its answer,
/// its error, or the box dropping the purchase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftPurchaseStage {
    /// `getMarketplaceGift` is out as this request.
    Loading(crate::ids::RequestId),
    /// The server quote; `price` is the one picked (`None` when the gift is
    /// not for resale), `note` a line about the last try.
    Quoted {
        quote: GiftQuote,
        price: Option<GiftPrice>,
        note: Option<String>,
    },
    /// `sendResoldGift` is out as `request`, at `price`.
    Sending {
        quote: GiftQuote,
        price: GiftPrice,
        request: crate::ids::RequestId,
    },
    /// The gift was sent.
    Completed(GiftQuote),
    /// No quote to buy from: the load failed, the answer was not the gift
    /// asked for, or the price went away. Load again.
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct GiftPurchase {
    pub chat_id: ChatId,
    pub recipient: MessageSender,
    pub recipient_name: String,
    pub requested_name: String,
    pub stage: GiftPurchaseStage,
}

impl GiftPurchase {
    /// A purchase whose quote was asked as `request`.
    pub fn loading(
        chat_id: ChatId,
        recipient: MessageSender,
        recipient_name: String,
        requested_name: String,
        request: crate::ids::RequestId,
    ) -> Self {
        Self {
            chat_id,
            recipient,
            recipient_name,
            requested_name,
            stage: GiftPurchaseStage::Loading(request),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.stage, GiftPurchaseStage::Loading(_))
    }

    pub fn is_sending(&self) -> bool {
        matches!(self.stage, GiftPurchaseStage::Sending { .. })
    }

    /// A request of this purchase is out.
    pub fn is_busy(&self) -> bool {
        self.is_loading() || self.is_sending()
    }

    pub fn is_completed(&self) -> bool {
        matches!(self.stage, GiftPurchaseStage::Completed(_))
    }

    pub fn quote(&self) -> Option<&GiftQuote> {
        match &self.stage {
            GiftPurchaseStage::Quoted { quote, .. }
            | GiftPurchaseStage::Sending { quote, .. }
            | GiftPurchaseStage::Completed(quote) => Some(quote),
            GiftPurchaseStage::Loading(_) | GiftPurchaseStage::Failed(_) => None,
        }
    }

    /// The price picked; none once the gift is sent.
    pub fn price(&self) -> Option<GiftPrice> {
        match &self.stage {
            GiftPurchaseStage::Quoted { price, .. } => *price,
            GiftPurchaseStage::Sending { price, .. } => Some(*price),
            _ => None,
        }
    }

    /// The line under the title, derived from the stage.
    pub fn note(&self) -> Option<String> {
        match &self.stage {
            GiftPurchaseStage::Loading(_) => None,
            GiftPurchaseStage::Quoted { note, .. } => note.clone(),
            GiftPurchaseStage::Sending { .. } => Some("Sending the gift…".into()),
            GiftPurchaseStage::Completed(_) => Some("Gift sent successfully.".into()),
            GiftPurchaseStage::Failed(note) => Some(note.clone()),
        }
    }

    /// Pick `price` among the quote's, while nothing is on its way.
    pub fn select_price(&mut self, price: GiftPrice) {
        if let GiftPurchaseStage::Quoted { price: held, .. } = &mut self.stage {
            *held = Some(price);
        }
    }

    /// Say why the last try did nothing; the stage stays.
    pub fn set_note(&mut self, line: String) {
        if let GiftPurchaseStage::Quoted { note, .. } = &mut self.stage {
            *note = Some(line);
        }
    }

    /// The request this purchase waits for.
    pub fn waits_for(&self, request: crate::ids::RequestId) -> bool {
        match &self.stage {
            GiftPurchaseStage::Loading(held) | GiftPurchaseStage::Sending { request: held, .. } => {
                *held == request
            }
            _ => false,
        }
    }

    /// The quote `request` answered: it must be the request this purchase
    /// waits for, and the gift asked for.
    pub fn accept_quote(&mut self, request: crate::ids::RequestId, quote: Option<GiftQuote>) {
        if !self.is_loading() || !self.waits_for(request) {
            return;
        }
        self.stage = match quote.filter(|q| q.name == self.requested_name) {
            Some(quote) => {
                let price = quote.stars.or(quote.ton);
                GiftPurchaseStage::Quoted {
                    price,
                    note: price
                        .is_none()
                        .then(|| "This gift is not available for resale.".into()),
                    quote,
                }
            }
            None => GiftPurchaseStage::Failed(
                "Could not verify the returned gift. Load the gift again.".into(),
            ),
        };
    }

    /// The send answered. A higher price never submits by itself: it goes
    /// back to the quote with the new amount.
    pub fn accept_result(&mut self, request: crate::ids::RequestId, result: GiftPurchaseResult) {
        if !matches!(self.stage, GiftPurchaseStage::Sending { .. }) || !self.waits_for(request) {
            return;
        }
        let GiftPurchaseStage::Sending { quote, .. } =
            std::mem::replace(&mut self.stage, GiftPurchaseStage::Failed(String::new()))
        else {
            return;
        };
        self.stage = match result {
            // The receipt id is intentionally empty for gifts sent to others.
            GiftPurchaseResult::Sent(_) => GiftPurchaseStage::Completed(quote),
            GiftPurchaseResult::PriceIncreased(None) => GiftPurchaseStage::Failed(
                "The price increased. Review the new amount and confirm again; nothing was purchased."
                    .into(),
            ),
            GiftPurchaseResult::PriceIncreased(Some(price)) => {
                let mut quote = quote;
                match price {
                    GiftPrice::Stars(_) => quote.stars = Some(price),
                    GiftPrice::TonCents(_) => quote.ton = Some(price),
                }
                GiftPurchaseStage::Quoted {
                    quote,
                    price: Some(price),
                    note: Some(
                        "The price increased. Review the new amount and confirm again; nothing was purchased."
                            .into(),
                    ),
                }
            }
        };
    }

    /// `request` failed: loading ends with no quote, a send goes back to the
    /// quote it was sent from. A quote, a receipt or an earlier failure has
    /// no request to fail and stays as it is, and so does an answer of a
    /// request this purchase does not wait for.
    pub fn fail(&mut self, request: crate::ids::RequestId, line: String) {
        if !self.waits_for(request) {
            return;
        }
        self.stage =
            match std::mem::replace(&mut self.stage, GiftPurchaseStage::Failed(String::new())) {
                GiftPurchaseStage::Sending { quote, price, .. } => GiftPurchaseStage::Quoted {
                    quote,
                    price: Some(price),
                    note: Some(line),
                },
                _ => GiftPurchaseStage::Failed(line),
            };
    }
}

#[cfg(test)]
mod stage_tests {
    use super::*;
    use crate::ids::RequestId;

    fn quote() -> GiftQuote {
        GiftQuote {
            name: "Plush".into(),
            title: "Plush".into(),
            stars: Some(GiftPrice::Stars(25)),
            ton: None,
        }
    }

    fn purchase() -> GiftPurchase {
        GiftPurchase::loading(
            ChatId(1),
            MessageSender::User { user_id: 1 },
            "Ada".into(),
            "Plush".into(),
            RequestId(1),
        )
    }

    #[test]
    fn a_failed_load_leaves_loading_and_a_failed_send_returns_to_the_quote() {
        let mut gift = purchase();
        assert!(gift.is_loading() && gift.waits_for(RequestId(1)));
        gift.fail(RequestId(9), "other".into());
        assert!(gift.is_loading(), "another request's failure changes nothing");
        gift.fail(RequestId(1), "no".into());
        assert!(!gift.is_loading() && gift.quote().is_none());

        let mut gift = purchase();
        gift.accept_quote(RequestId(9), Some(quote()));
        assert!(gift.is_loading(), "another request's quote is dropped");
        gift.accept_quote(RequestId(1), Some(quote()));
        gift.stage = GiftPurchaseStage::Sending {
            quote: quote(),
            price: GiftPrice::Stars(25),
            request: RequestId(2),
        };
        assert!(gift.is_sending() && gift.waits_for(RequestId(2)));
        gift.accept_result(RequestId(9), GiftPurchaseResult::Sent(String::new()));
        assert!(gift.is_sending(), "another request's result is dropped");
        gift.fail(RequestId(2), "refused".into());
        assert!(!gift.is_sending());
        assert_eq!(gift.price(), Some(GiftPrice::Stars(25)));
        assert_eq!(gift.note().as_deref(), Some("refused"));
    }

    #[test]
    fn a_higher_price_asks_again_and_a_late_answer_changes_nothing() {
        let mut gift = purchase();
        gift.accept_quote(RequestId(1), Some(quote()));
        // An answer with nothing sent is ignored.
        gift.accept_result(RequestId(2), GiftPurchaseResult::Sent(String::new()));
        assert!(!gift.is_completed());
        assert_eq!(gift.price(), Some(GiftPrice::Stars(25)), "the quote stays");
        gift.fail(RequestId(2), "stray".into());
        assert_eq!(
            gift.price(),
            Some(GiftPrice::Stars(25)),
            "no request failed"
        );
        assert_eq!(gift.note(), None);
        gift.stage = GiftPurchaseStage::Sending {
            quote: quote(),
            price: GiftPrice::Stars(25),
            request: RequestId(2),
        };
        gift.accept_result(
            RequestId(2),
            GiftPurchaseResult::PriceIncreased(Some(GiftPrice::Stars(40))),
        );
        assert_eq!(gift.price(), Some(GiftPrice::Stars(40)));
        assert!(!gift.is_sending() && !gift.is_completed());
    }
}
