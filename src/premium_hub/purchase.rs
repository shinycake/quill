//! Buying Premium inside Quill, as tdesktop does.
//!
//! tdesktop `settings/sections/settings_premium.cpp` `StartPremiumPayment`:
//! the app config key `premium_bot_username` names the bot to open with a
//! `/start <ref>` that is sent right away; without it, the config key
//! `premium_invoice_slug` is the invoice link `t.me/$<slug>`, which opens
//! the in-app payment form. With neither, nothing happens. `ref` is the
//! `EntryMap` key of the feature the user came from (`LookupPremiumRef`).

use serde_json::Value;

/// `premium_bot_username` and `premium_invoice_slug` from the app config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PremiumPurchase {
    pub bot_username: String,
    pub invoice_slug: String,
}

/// What the subscribe button does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PremiumPurchaseRoute {
    /// Open the bot and send `/start <start_ref>` without asking.
    Bot { username: String, start_ref: String },
    /// Open the in-app payment form of this invoice link.
    Invoice { slug: String },
    /// The config names neither (tdesktop does nothing).
    Unavailable,
}

/// A string member of a `jsonValueObject` app config.
fn config_string(config: &Value, key: &str) -> String {
    config
        .get("members")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|m| m.get("key").and_then(Value::as_str) == Some(key))
        .and_then(|m| m.get("value")?.get("value")?.as_str())
        .map(|s| s.trim().trim_start_matches('@').to_string())
        .unwrap_or_default()
}

/// The purchase keys of a `getApplicationConfig` answer.
pub fn purchase_from_config(config: &Value) -> PremiumPurchase {
    PremiumPurchase {
        bot_username: config_string(config, "premium_bot_username"),
        invoice_slug: config_string(config, "premium_invoice_slug"),
    }
}

/// tdesktop's `StartPremiumPayment` decision.
pub fn purchase_route(purchase: &PremiumPurchase, start_ref: &str) -> PremiumPurchaseRoute {
    if !purchase.bot_username.is_empty() {
        PremiumPurchaseRoute::Bot {
            username: purchase.bot_username.clone(),
            start_ref: start_ref.to_string(),
        }
    } else if !purchase.invoice_slug.is_empty() {
        PremiumPurchaseRoute::Invoice {
            slug: purchase.invoice_slug.clone(),
        }
    } else {
        PremiumPurchaseRoute::Unavailable
    }
}

/// tdesktop `EntryMap` key of a feature row (`LookupPremiumRef`), by the
/// row title `feature_row` gives it. Features without an entry have none.
pub fn feature_ref(title: &str) -> Option<&'static str> {
    Some(match title {
        "Doubled limits" => "double_limits",
        "Files up to 4 GB" => "more_upload",
        "Faster download speed" => "faster_download",
        "Voice-to-text conversion" => "voice_to_text",
        "No ads" => "no_ads",
        "Unique reactions" => "infinite_reactions",
        "Premium stickers" => "premium_stickers",
        "Animated emoji" => "animated_emoji",
        "Advanced chat management" => "advanced_chat_management",
        "Profile badge" => "profile_badge",
        "Emoji status" => "emoji_status",
        "Animated profile pictures" => "animated_userpics",
        "Real-time translation" => "translations",
        "Upgraded stories" => "stories",
        "Name and profile colors" => "peer_colors",
        "Wallpapers for both sides" => "wallpapers",
        "Tags for Saved Messages" => "saved_tags",
        "Message privacy" => "message_privacy",
        "Last seen times" => "last_seen",
        "Telegram Business" => "business",
        "Message effects" => "effects",
        "Checklists" => "todo",
        "Protect private chat content" => "no_forwards",
        "AI text tools" => "ai_compose",
        "Rich messages" => "rich_formatting",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        PremiumPurchase, PremiumPurchaseRoute, feature_ref, purchase_from_config, purchase_route,
    };
    use serde_json::json;

    fn config(bot: &str, slug: &str) -> serde_json::Value {
        json!({"@type": "jsonValueObject", "members": [
            {"key": "premium_bot_username", "value": {"@type": "jsonValueString", "value": bot}},
            {"key": "premium_invoice_slug", "value": {"@type": "jsonValueString", "value": slug}},
        ]})
    }

    #[test]
    fn config_keys_are_read() {
        let p = purchase_from_config(&config("PremiumBot", "abc"));
        assert_eq!(p.bot_username, "PremiumBot");
        assert_eq!(p.invoice_slug, "abc");
        assert_eq!(purchase_from_config(&json!({})), PremiumPurchase::default());
    }

    #[test]
    fn the_bot_wins_over_the_invoice_and_neither_does_nothing() {
        let both = purchase_from_config(&config("PremiumBot", "abc"));
        assert_eq!(
            purchase_route(&both, "stories"),
            PremiumPurchaseRoute::Bot {
                username: "PremiumBot".into(),
                start_ref: "stories".into()
            }
        );
        let slug = purchase_from_config(&config("", "abc"));
        assert_eq!(
            purchase_route(&slug, "x"),
            PremiumPurchaseRoute::Invoice { slug: "abc".into() }
        );
        assert_eq!(
            purchase_route(&PremiumPurchase::default(), "x"),
            PremiumPurchaseRoute::Unavailable
        );
    }

    #[test]
    fn feature_rows_with_an_entry_have_a_ref() {
        assert_eq!(feature_ref("Doubled limits"), Some("double_limits"));
        assert_eq!(feature_ref("Message effects"), Some("effects"));
        assert_eq!(feature_ref("Topic icons"), None);
    }
}
