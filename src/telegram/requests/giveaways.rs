//! Requests behind giveaways (tdesktop `Api::Premium::resolveGiveawayInfo`,
//! `PremiumGiftCodeOptions` and `CreditsGiveawayOptions`).
use crate::ids::RequestId;
use serde_json::{Value, json};

/// `getGiveawayInfo`. Response is `giveawayInfoOngoing` or
/// `giveawayInfoCompleted`.
pub fn get_giveaway_info(extra: RequestId, chat_id: i64, message_id: i64) -> String {
    json!({
        "@type": "getGiveawayInfo",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
        "message_id": message_id,
    })
    .to_string()
}

/// `getPremiumGiveawayPaymentOptions`. Response is
/// `premiumGiveawayPaymentOptions`.
pub fn get_premium_giveaway_payment_options(extra: RequestId, boosted_chat_id: i64) -> String {
    json!({
        "@type": "getPremiumGiveawayPaymentOptions",
        "@extra": extra.as_extra(),
        "boosted_chat_id": boosted_chat_id,
    })
    .to_string()
}

/// `getStarGiveawayPaymentOptions`. Response is
/// `starGiveawayPaymentOptions`.
pub fn get_star_giveaway_payment_options(extra: RequestId) -> String {
    json!({
        "@type": "getStarGiveawayPaymentOptions",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `launchPrepaidGiveaway`. Response is `ok`. `star_count` is 0 for Telegram
/// Premium giveaways.
pub fn launch_prepaid_giveaway(
    extra: RequestId,
    giveaway_id: i64,
    parameters: Value,
    winner_count: i32,
    star_count: i64,
) -> String {
    json!({
        "@type": "launchPrepaidGiveaway",
        "@extra": extra.as_extra(),
        "giveaway_id": giveaway_id.to_string(),
        "parameters": parameters,
        "winner_count": winner_count,
        "star_count": star_count,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn giveaway_requests_use_the_schema_names() {
        let info = parse(get_giveaway_info(RequestId(1), -100, 55));
        assert_eq!(info["@type"], "getGiveawayInfo");
        assert_eq!(
            (info["chat_id"].as_i64(), info["message_id"].as_i64()),
            (Some(-100), Some(55))
        );
        let premium = parse(get_premium_giveaway_payment_options(RequestId(2), -100));
        assert_eq!(premium["boosted_chat_id"], -100);
        assert_eq!(
            parse(get_star_giveaway_payment_options(RequestId(3)))["@type"],
            "getStarGiveawayPaymentOptions"
        );
    }

    #[test]
    fn launch_carries_the_parameters_unchanged() {
        let params = json!({"@type": "giveawayParameters", "boosted_chat_id": -100});
        let launch = parse(launch_prepaid_giveaway(
            RequestId(4),
            123_456_789_012,
            params.clone(),
            7,
            0,
        ));
        assert_eq!(launch["@type"], "launchPrepaidGiveaway");
        assert_eq!(launch["giveaway_id"].as_str(), Some("123456789012"));
        assert_eq!(launch["parameters"], params);
        assert_eq!(
            (
                launch["winner_count"].as_i64(),
                launch["star_count"].as_i64()
            ),
            (Some(7), Some(0))
        );
    }
}
