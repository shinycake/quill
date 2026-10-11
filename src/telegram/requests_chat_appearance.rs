//! Requests that change how a channel or supergroup looks (TDLib 1.8.68,
//! `schema/td_api.tl`): name color, profile color and emoji status. All
//! need the chat's boost level; none needs Telegram Premium.
use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// `setChatAccentColor chat_id:int53 accent_color_id:int32
/// background_custom_emoji_id:int64 = Ok;` (line 13835): the name color and
/// the reply icon of a channel. Pass 0 for no icon.
pub fn set_chat_accent_color(
    extra: RequestId,
    chat_id: ChatId,
    accent_color_id: i32,
    background_custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "setChatAccentColor",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "accent_color_id": accent_color_id,
        "background_custom_emoji_id": background_custom_emoji_id,
    })
    .to_string()
}

/// `setChatProfileAccentColor chat_id:int53 profile_accent_color_id:int32
/// profile_background_custom_emoji_id:int64 = Ok;` (line 13842): -1 clears
/// the color, 0 the icon.
pub fn set_chat_profile_accent_color(
    extra: RequestId,
    chat_id: ChatId,
    profile_accent_color_id: i32,
    profile_background_custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "setChatProfileAccentColor",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "profile_accent_color_id": profile_accent_color_id,
        "profile_background_custom_emoji_id": profile_background_custom_emoji_id,
    })
    .to_string()
}

/// `setChatEmojiStatus chat_id:int53 emoji_status:emojiStatus = Ok;` (line
/// 13853). `None` passes null, which removes the status. A status set here
/// has no expiration.
pub fn set_chat_emoji_status(
    extra: RequestId,
    chat_id: ChatId,
    custom_emoji_id: Option<i64>,
) -> String {
    let emoji_status = custom_emoji_id.map(|id| {
        json!({
            "@type": "emojiStatus",
            "type": { "@type": "emojiStatusTypeCustomEmoji", "custom_emoji_id": id.to_string() },
            "expiration_date": 0,
        })
    });
    json!({
        "@type": "setChatEmojiStatus",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "emoji_status": emoji_status,
    })
    .to_string()
}

/// `setChatBackground` (line 13867) with a chat theme background for a boosted
/// channel or supergroup: no input background, type `backgroundTypeChatTheme`
/// (line 8801). Never `only_for_self` in boosted chats.
pub fn set_chat_theme_background(extra: RequestId, chat_id: ChatId, theme_name: &str) -> String {
    json!({
        "@type": "setChatBackground",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "background": null,
        "type": { "@type": "backgroundTypeChatTheme", "theme_name": theme_name },
        "dark_theme_dimming": 0,
        "only_for_self": false,
    })
    .to_string()
}

/// `getDefaultChatEmojiStatuses = EmojiStatusCustomEmojis;` (line 14367):
/// the statuses offered for chats (tdesktop `channelStatusMode`).
pub fn get_default_chat_emoji_statuses(extra: RequestId) -> String {
    json!({
        "@type": "getDefaultChatEmojiStatuses",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: String) -> Value {
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn accent_color_request_targets_the_chat() {
        let v = parse(set_chat_accent_color(RequestId(5), ChatId(-1001), 8, 77));
        assert_eq!(v["@type"], "setChatAccentColor");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["chat_id"], -1001);
        assert_eq!(v["accent_color_id"], 8);
        assert_eq!(v["background_custom_emoji_id"], 77);
    }

    #[test]
    fn profile_accent_color_request_targets_the_chat() {
        let v = parse(set_chat_profile_accent_color(
            RequestId(6),
            ChatId(-1002),
            -1,
            0,
        ));
        assert_eq!(v["@type"], "setChatProfileAccentColor");
        assert_eq!(v["chat_id"], -1002);
        assert_eq!(v["profile_accent_color_id"], -1);
        assert_eq!(v["profile_background_custom_emoji_id"], 0);
    }

    #[test]
    fn emoji_status_request_sets_or_clears() {
        let v = parse(set_chat_emoji_status(
            RequestId(7),
            ChatId(-1003),
            Some(12345),
        ));
        assert_eq!(v["@type"], "setChatEmojiStatus");
        assert_eq!(v["chat_id"], -1003);
        assert_eq!(v["emoji_status"]["@type"], "emojiStatus");
        assert_eq!(
            v["emoji_status"]["type"]["@type"],
            "emojiStatusTypeCustomEmoji"
        );
        assert_eq!(v["emoji_status"]["type"]["custom_emoji_id"], "12345");
        assert_eq!(v["emoji_status"]["expiration_date"], 0);
        let cleared = parse(set_chat_emoji_status(RequestId(8), ChatId(-1003), None));
        assert_eq!(cleared["emoji_status"], Value::Null);
    }

    #[test]
    fn chat_theme_background_request_is_for_everyone() {
        let v = parse(set_chat_theme_background(RequestId(4), ChatId(-1005), "🦁"));
        assert_eq!(v["@type"], "setChatBackground");
        assert_eq!(v["chat_id"], -1005);
        assert_eq!(v["background"], Value::Null);
        assert_eq!(v["type"]["@type"], "backgroundTypeChatTheme");
        assert_eq!(v["type"]["theme_name"], "🦁");
        assert_eq!(v["only_for_self"], false);
    }

    #[test]
    fn default_chat_statuses_request() {
        let v = parse(get_default_chat_emoji_statuses(RequestId(9)));
        assert_eq!(v["@type"], "getDefaultChatEmojiStatuses");
    }
}
