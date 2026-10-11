//! Star packs for sale (`getStarPaymentOptions`), shown under "Buy more
//! Stars" in the Stars dialog. Telegram Desktop buys a pack through
//! `inputInvoiceTelegram` with `telegramPaymentPurposeStars`; Quill's
//! checkout only handles invoices in messages, so the list is read-only
//! here and the purchase happens in an official app or @PremiumBot.
use serde_json::Value;

/// One `starPaymentOption`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarPack {
    pub currency: String,
    /// Price in the smallest units of `currency`.
    pub amount: i64,
    pub star_count: i64,
    /// Offered only after "Show more options" in Telegram Desktop.
    pub is_additional: bool,
}

/// The options list and its loading state.
#[derive(Debug, Clone, Default)]
pub struct StarPacksState {
    pub options: Vec<StarPack>,
    pub loading: bool,
    pub loaded: bool,
    pub error: Option<String>,
    /// "Buy more Stars" is expanded in the dialog.
    pub open: bool,
}

/// Parses a `starPaymentOptions` answer, smallest pack first.
pub fn parse_star_packs(value: &Value) -> Vec<StarPack> {
    let int = |v: &Value, key: &str| v.get(key).and_then(Value::as_i64).unwrap_or(0);
    let mut out: Vec<StarPack> = value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| StarPack {
            currency: option
                .get("currency")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            amount: int(option, "amount"),
            star_count: int(option, "star_count"),
            is_additional: option
                .get("is_additional")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
        .filter(|pack| pack.star_count > 0)
        .collect();
    out.sort_by_key(|pack| pack.star_count);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn packs_parse_and_sort() {
        let packs = parse_star_packs(&json!({
            "@type": "starPaymentOptions",
            "options": [
                {"currency": "USD", "amount": 1999, "star_count": 1000, "is_additional": false},
                {"currency": "USD", "amount": 199, "star_count": 100, "is_additional": false},
                {"currency": "USD", "amount": 9999, "star_count": 5000, "is_additional": true},
                {"currency": "USD", "amount": 1, "star_count": 0}
            ]
        }));
        assert_eq!(packs.len(), 3);
        assert_eq!(packs[0].star_count, 100);
        assert_eq!(packs[0].amount, 199);
        assert!(packs[2].is_additional);
        assert!(parse_star_packs(&json!({})).is_empty());
    }
}
