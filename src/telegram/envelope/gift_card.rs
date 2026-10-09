//! Bubble cards for gift and giveaway service messages (tdesktop
//! `HistoryView::MediaGift` / `ServiceBox`): the sticker, a title, the
//! sender's message and a few facts. The one-line wording stays in
//! `crate::service_text`; this holds only the card's own data.

use super::*;
use crate::premium_hub::{group_digits, parse_gift_sticker};
use serde_json::Value;

/// What a gift card depicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GiftCardKind {
    Regular,
    Collectible,
    Premium,
    Stars,
    GiftCode,
    Giveaway,
    GiveawayWinners,
}

/// Card data parsed next to the [`ServiceAction`] it decorates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiftCard {
    pub kind: GiftCardKind,
    pub title: String,
    pub subtitle: String,
    /// The sender's message, if any.
    pub text: String,
    pub sticker: Option<StickerContent>,
    /// Collectible backdrop as 0xRRGGBB (center, edge).
    pub backdrop: Option<(u32, u32)>,
    /// Label/value rows (Model, Backdrop, Symbol, Winners…).
    pub facts: Vec<(String, String)>,
    /// `received_gift_id` of a received gift (empty otherwise).
    pub received_gift_id: String,
}

fn str_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn int_of(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn text_field(value: &Value) -> String {
    value
        .get("text")
        .and_then(|t| t.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn plural(n: i64, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{} {many}", group_digits(n))
    }
}

fn duration(months: i64, days: i64) -> String {
    if months > 0 {
        if months % 12 == 0 {
            plural(months / 12, "year", "years")
        } else {
            plural(months, "month", "months")
        }
    } else {
        plural(days, "day", "days")
    }
}

fn sticker_of(value: Option<&Value>, files: &mut Vec<ParsedFile>) -> Option<StickerContent> {
    let (sticker, found) = parse_gift_sticker(value);
    files.extend(found);
    sticker
}

/// Build the card for a gift/giveaway constructor; `None` for every other
/// service constructor (they stay centered pills).
pub(crate) fn build_gift_card(
    kind: &str,
    value: &Value,
    files: &mut Vec<ParsedFile>,
) -> Option<GiftCard> {
    let mut card = GiftCard {
        kind: GiftCardKind::Regular,
        title: String::new(),
        subtitle: String::new(),
        text: String::new(),
        sticker: None,
        backdrop: None,
        facts: Vec::new(),
        received_gift_id: String::new(),
    };
    match kind {
        "messageGift" => {
            let gift = value.get("gift")?;
            card.title = "Gift".into();
            let stars = int_of(gift, "star_count");
            card.subtitle = if value
                .get("is_prepaid_upgrade")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                "Prepaid upgrade".into()
            } else {
                plural(stars, "Star", "Stars")
            };
            card.text = text_field(value);
            card.sticker = sticker_of(gift.get("sticker"), files);
            card.received_gift_id = str_of(value, "received_gift_id");
            let sell = int_of(value, "sell_star_count");
            if sell > 0
                && !value
                    .get("was_converted")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                card.facts
                    .push(("Converts to".into(), plural(sell, "Star", "Stars")));
            }
        }
        "messageUpgradedGift" => {
            let gift = value.get("gift")?;
            card.kind = GiftCardKind::Collectible;
            let number = int_of(gift, "number");
            let title = str_of(gift, "title");
            card.title = if number > 0 {
                format!("{title} #{}", group_digits(number))
            } else {
                title
            };
            card.subtitle = "Collectible gift".into();
            card.text = text_field(value);
            card.received_gift_id = str_of(value, "received_gift_id");
            card.sticker = sticker_of(gift.get("model").and_then(|m| m.get("sticker")), files);
            card.backdrop = gift.get("backdrop").and_then(|b| b.get("colors")).map(|c| {
                (
                    (int_of(c, "center_color") as u32) & 0x00FF_FFFF,
                    (int_of(c, "edge_color") as u32) & 0x00FF_FFFF,
                )
            });
            for (label, key) in [
                ("Model", "model"),
                ("Backdrop", "backdrop"),
                ("Symbol", "symbol"),
            ] {
                let name = gift.get(key).map(|a| str_of(a, "name")).unwrap_or_default();
                if !name.is_empty() {
                    card.facts.push((label.into(), name));
                }
            }
        }
        "messageGiftedPremium" => {
            card.kind = GiftCardKind::Premium;
            card.title = "Telegram Premium".into();
            card.subtitle = duration(int_of(value, "month_count"), int_of(value, "day_count"));
            card.text = text_field(value);
            card.sticker = sticker_of(value.get("sticker"), files);
        }
        "messageGiftedStars" => {
            card.kind = GiftCardKind::Stars;
            card.title = "Gifted Stars".into();
            card.subtitle = plural(int_of(value, "star_count"), "Star", "Stars");
            card.sticker = sticker_of(value.get("sticker"), files);
        }
        "messagePremiumGiftCode" => {
            card.kind = GiftCardKind::GiftCode;
            card.title = "Gift Code".into();
            card.subtitle = format!(
                "Telegram Premium for {}",
                duration(int_of(value, "month_count"), int_of(value, "day_count"))
            );
            card.text = text_field(value);
            card.sticker = sticker_of(value.get("sticker"), files);
        }
        "messageGiveaway" => {
            card.kind = GiftCardKind::Giveaway;
            card.title = "Giveaway".into();
            let winners = int_of(value, "winner_count");
            let prize = value.get("prize");
            let prize_type = prize
                .and_then(|p| p.get("@type"))
                .and_then(Value::as_str)
                .unwrap_or("");
            card.subtitle = match prize_type {
                "giveawayPrizeStars" => format!(
                    "{} to {}",
                    plural(prize.map_or(0, |p| int_of(p, "star_count")), "Star", "Stars"),
                    plural(winners, "winner", "winners")
                ),
                _ => format!(
                    "{} Telegram Premium for {}",
                    plural(winners, "subscription", "subscriptions"),
                    duration(prize.map_or(0, |p| int_of(p, "month_count")), 0)
                ),
            };
            card.text = value
                .get("parameters")
                .map(|p| str_of(p, "prize_description"))
                .unwrap_or_default();
            card.sticker = sticker_of(value.get("sticker"), files);
        }
        "messageGiveawayWinners" => {
            card.kind = GiftCardKind::GiveawayWinners;
            card.title = "Giveaway winners".into();
            card.subtitle = plural(int_of(value, "winner_count"), "winner", "winners");
            card.text = str_of(value, "prize_description");
            let unclaimed = int_of(value, "unclaimed_prize_count");
            if unclaimed > 0 {
                card.facts.push(("Unclaimed".into(), unclaimed.to_string()));
            }
        }
        _ => return None,
    }
    Some(card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn build(kind: &str, v: Value) -> (Option<GiftCard>, usize) {
        let mut files = Vec::new();
        let card = build_gift_card(kind, &v, &mut files);
        (card, files.len())
    }

    fn sticker_json() -> Value {
        json!({"@type": "sticker", "id": 1, "emoji": "x", "width": 512, "height": 512,
               "format": {"@type": "stickerFormatWebp"},
               "sticker": {"@type": "file", "id": 21, "size": 10, "expected_size": 10,
                           "local": {"path": "", "is_downloading_completed": false, "can_be_downloaded": true},
                           "remote": {"id": "r", "unique_id": "u"}}})
    }

    #[test]
    fn regular_gift_card_shows_price_message_and_sticker() {
        let (card, files) = build(
            "messageGift",
            json!({"gift": {"star_count": 100, "sticker": sticker_json()},
                   "text": {"text": "Enjoy!", "entities": []},
                   "received_gift_id": "rg", "sell_star_count": 80}),
        );
        let card = card.unwrap();
        assert_eq!(card.kind, GiftCardKind::Regular);
        assert_eq!(card.subtitle, "100 Stars");
        assert_eq!(card.text, "Enjoy!");
        assert_eq!(card.received_gift_id, "rg");
        assert!(card.sticker.is_some());
        assert_eq!(files, 1);
        assert_eq!(
            card.facts,
            vec![("Converts to".to_string(), "80 Stars".to_string())]
        );
    }

    #[test]
    fn converted_gift_hides_conversion_fact() {
        let (card, _) = build(
            "messageGift",
            json!({"gift": {"star_count": 1}, "sell_star_count": 5, "was_converted": true}),
        );
        let card = card.unwrap();
        assert_eq!(card.subtitle, "1 Star");
        assert!(card.facts.is_empty());
    }

    #[test]
    fn collectible_card_has_backdrop_and_attributes() {
        let (card, _) = build(
            "messageUpgradedGift",
            json!({"gift": {"title": "Plush Pepe", "number": 4242,
                            "model": {"name": "Classic", "sticker": sticker_json()},
                            "symbol": {"name": "Star"},
                            "backdrop": {"name": "Onyx", "colors": {"center_color": 1, "edge_color": 2}}}}),
        );
        let card = card.unwrap();
        assert_eq!(card.title, "Plush Pepe #4,242");
        assert_eq!(card.backdrop, Some((1, 2)));
        assert_eq!(card.facts.len(), 3);
        assert_eq!(card.facts[0].0, "Model");
    }

    #[test]
    fn premium_stars_code_and_giveaway_cards() {
        let (premium, _) = build(
            "messageGiftedPremium",
            json!({"month_count": 12, "text": {"text": "Hi", "entities": []}}),
        );
        assert_eq!(premium.unwrap().subtitle, "1 year");
        let (stars, _) = build("messageGiftedStars", json!({"star_count": 250}));
        assert_eq!(stars.unwrap().subtitle, "250 Stars");
        let (code, _) = build(
            "messagePremiumGiftCode",
            json!({"month_count": 3, "code": "secret"}),
        );
        let code = code.unwrap();
        assert_eq!(code.subtitle, "Telegram Premium for 3 months");
        // The redeemable code never lands in the card.
        assert!(!format!("{code:?}").contains("secret"));
        let (giveaway, _) = build(
            "messageGiveaway",
            json!({"winner_count": 5, "prize": {"@type": "giveawayPrizeStars", "star_count": 500},
                   "parameters": {"prize_description": "Plus a mug"}}),
        );
        let giveaway = giveaway.unwrap();
        assert_eq!(giveaway.subtitle, "500 Stars to 5 winners");
        assert_eq!(giveaway.text, "Plus a mug");
        let (winners, _) = build(
            "messageGiveawayWinners",
            json!({"winner_count": 1, "unclaimed_prize_count": 2}),
        );
        assert_eq!(
            winners.unwrap().facts,
            vec![("Unclaimed".to_string(), "2".to_string())]
        );
    }

    #[test]
    fn other_constructors_have_no_card() {
        assert!(build("messagePinMessage", json!({})).0.is_none());
    }
}
