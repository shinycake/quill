//! Collectible usernames and phone numbers that were bought on Fragment
//! (tdesktop `ui/boxes/collectible_info_box.cpp`,
//! `SessionNavigation::resolveCollectible`).
//!
//! Tapping a collectible username or phone number asks TDLib
//! `getCollectibleItemInfo` and shows who it belongs to, when it was bought
//! and for how much, with a "Learn More" button to Fragment.
#![deny(clippy::indexing_slicing)]

use crate::ids::ChatId;
use serde_json::Value;

/// A username or a phone number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectibleKind {
    Username,
    Phone,
}

impl CollectibleKind {
    /// tdesktop `DetectCollectibleType`: a leading "+" is a phone number.
    pub fn detect(entity: &str) -> Self {
        if entity.starts_with('+') {
            Self::Phone
        } else {
            Self::Username
        }
    }
}

/// `collectibleItemInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollectibleInfo {
    pub purchase_date: i64,
    /// Fiat currency of `amount` ("USD"), amount in its smallest units.
    pub currency: String,
    pub amount: i64,
    /// "TON" with `crypto_amount` in nanoTON.
    pub crypto_currency: String,
    pub crypto_amount: i64,
    /// Fragment's page for the item ("Learn More").
    pub url: String,
}

/// `collectibleItemInfo` as TDLib sends it.
pub fn parse_collectible_info(value: &Value) -> CollectibleInfo {
    let int = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0);
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    CollectibleInfo {
        purchase_date: int("purchase_date"),
        currency: text("currency"),
        amount: int("amount"),
        crypto_currency: text("cryptocurrency"),
        crypto_amount: int("cryptocurrency_amount"),
        url: text("url"),
    }
}

/// The tapped item and the progress of its lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectibleLookup {
    pub kind: CollectibleKind,
    /// The username without "@", or the phone number as "+digits".
    pub entity: String,
    /// Who owns it: shown in the pill, with their picture.
    pub owner_name: String,
    pub owner_chat: Option<ChatId>,
    /// What "Copy Link" copies; empty for phone numbers.
    pub copy_text: String,
    pub info: Option<CollectibleInfo>,
    pub loading: bool,
    /// The TDLib error of a failed lookup, until the UI has dealt with it.
    pub failed: Option<String>,
}

impl CollectibleLookup {
    pub fn new(
        entity: &str,
        owner_name: &str,
        owner_chat: Option<ChatId>,
        copy_text: String,
    ) -> Self {
        Self {
            kind: CollectibleKind::detect(entity),
            entity: entity.to_string(),
            owner_name: owner_name.to_string(),
            owner_chat,
            copy_text,
            info: None,
            loading: true,
            failed: None,
        }
    }
}

/// The `getCollectibleItemInfo` type of an entity.
pub fn item_type(kind: CollectibleKind, entity: &str) -> Value {
    match kind {
        CollectibleKind::Username => serde_json::json!({
            "@type": "collectibleItemTypeUsername",
            "username": entity.trim_start_matches('@'),
        }),
        CollectibleKind::Phone => serde_json::json!({
            "@type": "collectibleItemTypePhoneNumber",
            "phone_number": entity,
        }),
    }
}

/// `FormatEntity`: "@name", or the phone number in groups.
pub fn format_entity(kind: CollectibleKind, entity: &str) -> String {
    match kind {
        CollectibleKind::Username => {
            if entity.starts_with('@') {
                entity.to_string()
            } else {
                format!("@{entity}")
            }
        }
        CollectibleKind::Phone => {
            let digits: String = entity.chars().filter(char::is_ascii_digit).collect();
            format!("+{digits}")
        }
    }
}

/// `lng_collectible_*_title` around the already formatted entity.
pub fn title_parts(kind: CollectibleKind) -> (&'static str, &'static str) {
    match kind {
        CollectibleKind::Username => ("", " is a collectible username that belongs to"),
        CollectibleKind::Phone => ("", " is a collectible phone number that belongs to"),
    }
}

/// `lng_collectible_*_info` split around the date and the price:
/// ("This username was bought on ", "Fragment", " on ", date, " for ").
pub fn info_prefix(kind: CollectibleKind) -> &'static str {
    match kind {
        CollectibleKind::Username => "This username was bought on ",
        CollectibleKind::Phone => "This phone number was bought on ",
    }
}

/// `lng_collectible_*_copy`.
pub fn copy_label(kind: CollectibleKind) -> &'static str {
    match kind {
        CollectibleKind::Username => "Copy Link",
        CollectibleKind::Phone => "Copy Phone Number",
    }
}

/// The crypto part of the price (`FormatPrice`): TON as "major.minor"
/// of nine decimals, up to seven trailing zeros cut (`MinorPart`), other
/// currencies as "{CUR:amount}".
pub fn crypto_text(info: &CollectibleInfo) -> String {
    if info.crypto_currency != "TON" {
        return format!("{{{}:{}}}", info.crypto_currency, info.crypto_amount);
    }
    const MINOR: usize = 9;
    let digits = info.crypto_amount.max(0).to_string();
    let (major, minor) = if digits.len() > MINOR {
        let split = digits.len() - MINOR;
        (
            digits.get(..split).unwrap_or("0").to_string(),
            digits.get(split..).unwrap_or("").to_string(),
        )
    } else {
        ("0".to_string(), format!("{digits:0>MINOR$}"))
    };
    let mut minor = minor;
    // `MinorPart` chops at most MINOR - 2 trailing zeros.
    let mut chopped = 0;
    while chopped < MINOR - 2 && minor.ends_with('0') {
        minor.pop();
        chopped += 1;
    }
    format!("{major}.{minor}")
}

/// The fiat part, "(USD 12.34)": tdesktop writes it with the currency
/// symbol; Quill keeps the currency code its checkout uses.
pub fn fiat_text(info: &CollectibleInfo) -> String {
    crate::telegram::envelope::format_payment_price(&info.currency, info.amount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_kind_follows_the_plus() {
        assert_eq!(CollectibleKind::detect("+8881234"), CollectibleKind::Phone);
        assert_eq!(CollectibleKind::detect("durov"), CollectibleKind::Username);
    }

    #[test]
    fn the_info_is_read() {
        let info = parse_collectible_info(&json!({
            "@type": "collectibleItemInfo", "purchase_date": 1_700_000_000,
            "currency": "USD", "amount": 1234, "cryptocurrency": "TON",
            "cryptocurrency_amount": 5_500_000_000_i64, "url": "https://fragment.com/username/x",
        }));
        assert_eq!(info.purchase_date, 1_700_000_000);
        assert_eq!(info.url, "https://fragment.com/username/x");
        assert_eq!(crypto_text(&info), "5.50");
        assert_eq!(fiat_text(&info), "USD 12.34");
    }

    #[test]
    fn ton_minor_digits_are_chopped_like_tdesktop() {
        let ton = |amount| CollectibleInfo {
            crypto_currency: "TON".into(),
            crypto_amount: amount,
            ..CollectibleInfo::default()
        };
        assert_eq!(crypto_text(&ton(1_000_000_000)), "1.00");
        assert_eq!(crypto_text(&ton(1_234_567_891)), "1.234567891");
        assert_eq!(crypto_text(&ton(1_230_000_000)), "1.23");
        assert_eq!(crypto_text(&ton(5_000_000)), "0.005");
        assert_eq!(crypto_text(&ton(0)), "0.00");
    }

    #[test]
    fn entities_are_formatted_and_requested_by_kind() {
        assert_eq!(format_entity(CollectibleKind::Username, "name"), "@name");
        assert_eq!(format_entity(CollectibleKind::Username, "@name"), "@name");
        assert_eq!(
            format_entity(CollectibleKind::Phone, "+888 12-3"),
            "+888123"
        );
        assert_eq!(
            item_type(CollectibleKind::Phone, "+888123")["phone_number"],
            "+888123"
        );
        assert_eq!(
            item_type(CollectibleKind::Username, "@name")["username"],
            "name"
        );
    }
}
