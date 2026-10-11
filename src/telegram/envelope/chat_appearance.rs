//! The part of a chat's appearance that `ChatAccent` does not cover: the
//! profile color, the profile background icon and the emoji status
//! (`chat.profile_accent_color_id`, `chat.profile_background_custom_emoji_id`,
//! `chat.emoji_status`; TDLib 1.8.68 `schema/td_api.tl:3937`).
use super::*;
use serde_json::Value;

/// A channel's or group's profile color (-1 = none), profile icon and
/// emoji status (0 = none). Defaults mean "nothing set".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatProfileLook {
    pub profile_accent_color_id: i32,
    pub profile_background_custom_emoji_id: i64,
    /// The custom emoji of `emoji_status.type`; for an upgraded-gift status
    /// the gift's model emoji.
    pub emoji_status_id: i64,
}

impl Default for ChatProfileLook {
    fn default() -> Self {
        Self {
            profile_accent_color_id: -1,
            profile_background_custom_emoji_id: 0,
            emoji_status_id: 0,
        }
    }
}

/// `emojiStatus.type` as one custom emoji id; 0 for a missing status.
pub(crate) fn parse_chat_emoji_status(status: Option<&Value>) -> i64 {
    let Some(kind) = status.and_then(|s| s.get("type")) else {
        return 0;
    };
    int64(kind.get("custom_emoji_id"))
        .or_else(|| int64(kind.get("model_custom_emoji_id")))
        .unwrap_or(0)
}

impl ChatProfileLook {
    pub(crate) fn parse(chat: &Value) -> Self {
        Self {
            profile_accent_color_id: chat
                .get("profile_accent_color_id")
                .and_then(Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or(-1),
            profile_background_custom_emoji_id: int64(
                chat.get("profile_background_custom_emoji_id"),
            )
            .unwrap_or(0),
            emoji_status_id: parse_chat_emoji_status(chat.get("emoji_status")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChatProfileLook;
    use serde_json::json;

    #[test]
    fn parses_profile_color_icon_and_status() {
        let look = ChatProfileLook::parse(&json!({
            "profile_accent_color_id": 4,
            "profile_background_custom_emoji_id": "5000000000000000001",
            "emoji_status": {
                "type": {"@type": "emojiStatusTypeCustomEmoji", "custom_emoji_id": "77"},
                "expiration_date": 0
            }
        }));
        assert_eq!(look.profile_accent_color_id, 4);
        assert_eq!(
            look.profile_background_custom_emoji_id,
            5_000_000_000_000_000_001
        );
        assert_eq!(look.emoji_status_id, 77);
    }

    #[test]
    fn missing_fields_mean_nothing_set() {
        assert_eq!(
            ChatProfileLook::parse(&json!({})),
            ChatProfileLook::default()
        );
        let gift = ChatProfileLook::parse(&json!({
            "emoji_status": {"type": {"@type": "emojiStatusTypeUpgradedGift", "model_custom_emoji_id": "9"}}
        }));
        assert_eq!(gift.emoji_status_id, 9);
    }
}
