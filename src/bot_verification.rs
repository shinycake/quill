//! Bot verification: the icon a verifying bot puts next to a verified
//! account, the description on its profile, and the owner flow that
//! verifies or unverifies accounts.
//!
//! Telegram Desktop: `Ui::PeerBadge::drawVerified` (`ui/unread_badge.cpp`)
//! draws the bot's custom emoji before the name in chat rows and the chat
//! header; `DetailsFiller::addBotVerify` (`info/profile/info_profile_actions.cpp`)
//! adds the description under the details (hidden for bots with a main
//! app); `MakeVerifyPeersBox` (`boxes/peers/verify_peers_box.cpp`) is the
//! owner's "Verify Accounts" list, reached from the bot's edit screen when
//! the bot has `verifierSettings`.

use crate::telegram::envelope::{int64, parse_formatted_text};
use serde_json::{Value, json};

/// `getOption("bot_verification_custom_description_length_max")` falls
/// back to the value tdesktop uses when the server sends none.
pub const DESCRIPTION_LIMIT_DEFAULT: usize = 70;

/// `botVerification` (TDLib `schema/td_api.tl:854`): who verified the
/// account, with which icon, and why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotVerification {
    pub bot_user_id: i64,
    pub icon_custom_emoji_id: i64,
    pub description: String,
}

/// `botVerificationParameters` (TDLib `schema/td_api.tl:848`), sent in
/// `botInfo.verification_parameters` to the bot's owner only.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BotVerificationParameters {
    pub icon_custom_emoji_id: i64,
    pub organization_name: String,
    pub default_description: String,
    pub can_set_custom_description: bool,
}

/// `botVerification`, or `None` for null / a payload without a bot.
pub fn parse_bot_verification(value: Option<&Value>) -> Option<BotVerification> {
    let value = value.filter(|v| !v.is_null())?;
    let bot_user_id = int64(value.get("bot_user_id"))?;
    Some(BotVerification {
        bot_user_id,
        icon_custom_emoji_id: int64(value.get("icon_custom_emoji_id")).unwrap_or(0),
        description: parse_formatted_text(value.get("custom_description")),
    })
}

/// `botVerificationParameters`, or `None` for null.
pub fn parse_bot_verification_parameters(
    value: Option<&Value>,
) -> Option<BotVerificationParameters> {
    let value = value.filter(|v| !v.is_null())?;
    Some(BotVerificationParameters {
        icon_custom_emoji_id: int64(value.get("icon_custom_emoji_id")).unwrap_or(0),
        organization_name: value
            .get("organization_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        default_description: parse_formatted_text(value.get("default_custom_description")),
        can_set_custom_description: value
            .get("can_set_custom_description")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The custom emoji to draw before a name: the verifying bot's icon from
/// `verificationStatus.bot_verification_icon_custom_emoji_id`. Zero means
/// none (tdesktop skips an empty `iconId`).
pub fn leading_icon(status: crate::peer_badge::VerificationStatus) -> Option<i64> {
    (status.bot_icon != 0).then_some(status.bot_icon)
}

/// The profile line under the details. Tdesktop hides it for a bot with a
/// main app (the app shows its own text) and when the description is empty.
pub fn profile_description(
    verification: Option<&BotVerification>,
    has_main_app: bool,
) -> Option<&str> {
    let text = verification?.description.trim();
    (!has_main_app && !text.is_empty()).then_some(text)
}

/// Whether the account already carries this bot's verification: the full
/// info names the bot, or the row's icon is the bot's own icon. Tdesktop
/// compares `botVerifyDetails()->botId`; the icon is the fallback while
/// the full info is not loaded.
pub fn verified_by_bot(
    bot_user_id: i64,
    parameters: Option<&BotVerificationParameters>,
    full_info: Option<&BotVerification>,
    status_icon: i64,
) -> bool {
    if let Some(info) = full_info {
        return info.bot_user_id == bot_user_id;
    }
    parameters.is_some_and(|p| p.icon_custom_emoji_id != 0 && p.icon_custom_emoji_id == status_icon)
}

/// Whether the owner sees "Verify Accounts" on the bot's profile: only
/// when TDLib sent `verification_parameters` (null for non-owners).
pub fn offers_verify_accounts(parameters: Option<&BotVerificationParameters>) -> bool {
    parameters.is_some()
}

/// The description to send: only bots allowed to customise it send one,
/// trimmed. Over-long text is an error so the dialog can flag the field.
pub fn description_to_send(
    parameters: &BotVerificationParameters,
    typed: &str,
    limit: usize,
) -> Result<String, DescriptionError> {
    if !parameters.can_set_custom_description {
        return Ok(String::new());
    }
    let text = typed.trim();
    if text.chars().count() > limit {
        return Err(DescriptionError::TooLong);
    }
    Ok(text.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionError {
    TooLong,
}

/// `setMessageSenderBotVerification` (TDLib `schema/td_api.tl:15500`).
/// `sender` is the `messageSenderUser` / `messageSenderChat` value.
pub fn set_request(extra: &str, bot_user_id: i64, sender: Value, description: &str) -> String {
    json!({
        "@type": "setMessageSenderBotVerification",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "verified_id": sender,
        "custom_description": {
            "@type": "formattedText",
            "text": description,
            "entities": [],
        },
    })
    .to_string()
}

/// `removeMessageSenderBotVerification` (TDLib `schema/td_api.tl:15505`).
pub fn remove_request(extra: &str, bot_user_id: i64, sender: Value) -> String {
    json!({
        "@type": "removeMessageSenderBotVerification",
        "@extra": extra,
        "bot_user_id": bot_user_id,
        "verified_id": sender,
    })
    .to_string()
}

/// Wording of tdesktop's `PeerVerifyPhrases`, by kind of account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyTarget {
    User,
    Bot,
    Channel,
    Group,
}

impl VerifyTarget {
    pub fn title(self) -> &'static str {
        match self {
            Self::User => "Verify User",
            Self::Bot => "Verify Bot",
            Self::Channel => "Verify Channel",
            Self::Group => "Verify Group",
        }
    }

    pub fn noun(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Bot => "bot",
            Self::Channel => "channel",
            Self::Group => "group",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_badge::VerificationStatus;

    fn params(custom: bool, icon: i64) -> BotVerificationParameters {
        BotVerificationParameters {
            icon_custom_emoji_id: icon,
            organization_name: "Acme".into(),
            default_description: "Checked by Acme".into(),
            can_set_custom_description: custom,
        }
    }

    #[test]
    fn parses_bot_verification_with_string_ids() {
        let value = json!({
            "@type": "botVerification",
            "bot_user_id": 77,
            "icon_custom_emoji_id": "5368324170671202286",
            "custom_description": {"@type": "formattedText", "text": "Verified by Acme", "entities": []},
        });
        let parsed = parse_bot_verification(Some(&value)).unwrap();
        assert_eq!(parsed.bot_user_id, 77);
        assert_eq!(parsed.icon_custom_emoji_id, 5368324170671202286);
        assert_eq!(parsed.description, "Verified by Acme");
        assert_eq!(parse_bot_verification(Some(&Value::Null)), None);
        assert_eq!(parse_bot_verification(None), None);
    }

    #[test]
    fn parses_verification_parameters() {
        let value = json!({
            "@type": "botVerificationParameters",
            "icon_custom_emoji_id": "42",
            "organization_name": "Acme",
            "default_custom_description": null,
            "can_set_custom_description": true,
        });
        let parsed = parse_bot_verification_parameters(Some(&value)).unwrap();
        assert_eq!(parsed.icon_custom_emoji_id, 42);
        assert_eq!(parsed.organization_name, "Acme");
        assert_eq!(parsed.default_description, "");
        assert!(parsed.can_set_custom_description);
        assert_eq!(parse_bot_verification_parameters(Some(&Value::Null)), None);
    }

    #[test]
    fn leading_icon_is_the_bot_icon_when_set() {
        let none = VerificationStatus::default();
        assert_eq!(leading_icon(none), None);
        let some = VerificationStatus {
            bot_icon: 9,
            ..Default::default()
        };
        assert_eq!(leading_icon(some), Some(9));
    }

    #[test]
    fn profile_description_skips_empty_and_main_app_bots() {
        let v = BotVerification {
            bot_user_id: 1,
            icon_custom_emoji_id: 2,
            description: "  Verified by Acme ".into(),
        };
        assert_eq!(
            profile_description(Some(&v), false),
            Some("Verified by Acme")
        );
        assert_eq!(profile_description(Some(&v), true), None);
        assert_eq!(profile_description(None, false), None);
        let empty = BotVerification::default();
        assert_eq!(profile_description(Some(&empty), false), None);
    }

    #[test]
    fn already_verified_prefers_the_full_info() {
        let p = params(false, 42);
        let info = BotVerification {
            bot_user_id: 5,
            ..Default::default()
        };
        assert!(verified_by_bot(5, Some(&p), Some(&info), 0));
        // Another bot verified it, even though the icon matches.
        assert!(!verified_by_bot(6, Some(&p), Some(&info), 42));
        // No full info yet: fall back to the icon.
        assert!(verified_by_bot(6, Some(&p), None, 42));
        assert!(!verified_by_bot(6, Some(&p), None, 7));
        assert!(!verified_by_bot(6, None, None, 42));
        assert!(!verified_by_bot(6, Some(&params(false, 0)), None, 0));
    }

    #[test]
    fn verify_accounts_needs_parameters() {
        assert!(!offers_verify_accounts(None));
        assert!(offers_verify_accounts(Some(&params(false, 1))));
    }

    #[test]
    fn description_only_goes_out_when_the_bot_may_customise() {
        assert_eq!(
            description_to_send(&params(false, 1), "hello", 70),
            Ok(String::new())
        );
        assert_eq!(
            description_to_send(&params(true, 1), "  hello ", 70),
            Ok("hello".to_string())
        );
        assert_eq!(
            description_to_send(&params(true, 1), &"x".repeat(71), 70),
            Err(DescriptionError::TooLong)
        );
    }

    #[test]
    fn request_json_matches_the_schema() {
        let sender = json!({"@type": "messageSenderUser", "user_id": 12});
        let set: Value =
            serde_json::from_str(&set_request("e1", 77, sender.clone(), "Checked")).unwrap();
        assert_eq!(set["@type"], "setMessageSenderBotVerification");
        assert_eq!(set["bot_user_id"], 77);
        assert_eq!(set["verified_id"], sender);
        assert_eq!(set["custom_description"]["text"], "Checked");
        let remove: Value =
            serde_json::from_str(&remove_request("e2", 77, sender.clone())).unwrap();
        assert_eq!(remove["@type"], "removeMessageSenderBotVerification");
        assert_eq!(remove["verified_id"], sender);
        assert!(remove.get("custom_description").is_none());
    }
}
