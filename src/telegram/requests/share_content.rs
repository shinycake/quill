//! `sendMessage` bodies for the attach menu's Contact and Location items and
//! for the dice emoji (tdesktop `Api::SendAction` with
//! `SendMediaType::Contact` / `Location` and `MTPDinputMediaDice`).

use super::{SendReply, message_send_options, message_topic_value, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// The emoji that send as a dice instead of text (tdesktop
/// `Stickers::DicePack` defaults; the server's `emojies_send_dice`
/// app-config list starts with exactly these). Each is the bare glyph; the
/// variation-selector form is accepted by [`dice_emoji`].
pub const DICE_EMOJIS: [&str; 6] = [
    "\u{1F3B2}", // game die
    "\u{1F3AF}", // direct hit
    "\u{1F3C0}", // basketball
    "\u{26BD}",  // soccer ball
    "\u{1F3B3}", // bowling
    "\u{1F3B0}", // slot machine
];

/// The dice a message sends as: when `text` is exactly one dice emoji
/// (surrounding whitespace and a trailing U+FE0F variation selector are
/// ignored), that emoji; otherwise `None` and the text sends as text.
pub fn dice_emoji(text: &str) -> Option<&'static str> {
    let trimmed = text.trim().trim_end_matches('\u{FE0F}');
    DICE_EMOJIS.iter().copied().find(|emoji| *emoji == trimmed)
}

/// Like [`dice_emoji`], against the list the server sent in
/// `updateDiceEmojis`; the built-in list stands in until it arrives.
pub fn dice_emoji_in(text: &str, server_list: &[String]) -> Option<String> {
    if server_list.is_empty() {
        return dice_emoji(text).map(str::to_string);
    }
    let trimmed = text.trim().trim_end_matches('\u{FE0F}');
    server_list
        .iter()
        .find(|emoji| emoji.trim_end_matches('\u{FE0F}') == trimmed)
        .cloned()
}

/// What a shared contact carries (`contact`, schema 1.8.67 line 640).
/// `user_id` is 0 for a bare phone number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactShare {
    pub phone_number: String,
    pub first_name: String,
    pub last_name: String,
    pub user_id: i64,
}

/// Parse a latitude / longitude pair typed by the user. Accepts a decimal
/// comma, trims whitespace, and rejects values outside -90..=90 /
/// -180..=180 or non-finite numbers, with the reason to show.
pub fn parse_coordinates(latitude: &str, longitude: &str) -> Result<(f64, f64), &'static str> {
    fn number(text: &str) -> Option<f64> {
        text.trim()
            .replace(',', ".")
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
    }
    let lat = number(latitude).ok_or("Enter the latitude as a number, e.g. 37.7749.")?;
    let lon = number(longitude).ok_or("Enter the longitude as a number, e.g. -122.4194.")?;
    if !(-90.0..=90.0).contains(&lat) {
        return Err("Latitude must be between -90 and 90.");
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err("Longitude must be between -180 and 180.");
    }
    Ok((lat, lon))
}

fn send_content(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<&SendReply>,
    options: &SendOptions,
    content: Value,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(reply_to),
        "options": message_send_options(options),
        "reply_markup": Value::Null,
        "input_message_content": content,
    })
    .to_string()
}

/// `inputMessageDice emoji:string clear_draft:Bool` (schema line 6154):
/// the server rolls the die and answers with a `messageDice`.
pub fn send_dice(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    emoji: &str,
    reply_to: Option<&SendReply>,
    options: &SendOptions,
) -> String {
    send_content(
        extra,
        chat_id,
        topic_id,
        reply_to,
        options,
        json!({
            "@type": "inputMessageDice",
            "emoji": emoji,
            "clear_draft": false,
        }),
    )
}

/// `inputMessageContact contact:contact` (schema line 6151). The vCard
/// stays empty: Quill shares the address-book entry's phone and names.
pub fn send_contact_card(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    contact: &ContactShare,
    reply_to: Option<&SendReply>,
    options: &SendOptions,
) -> String {
    send_content(
        extra,
        chat_id,
        topic_id,
        reply_to,
        options,
        json!({
            "@type": "inputMessageContact",
            "contact": {
                "@type": "contact",
                "phone_number": contact.phone_number,
                "first_name": contact.first_name,
                "last_name": contact.last_name,
                "vcard": "",
                "user_id": contact.user_id,
            },
        }),
    )
}

/// `inputMessageStory story_poster_chat_id:int53 story_id:int32` (schema
/// `td_api.tl:6539`): shares a story as a message (tdesktop's story share
/// box sends the story the same way).
pub fn send_story_card(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    poster_chat_id: ChatId,
    story_id: i32,
    options: &SendOptions,
) -> String {
    send_content(
        extra,
        chat_id,
        topic_id,
        None,
        options,
        json!({
            "@type": "inputMessageStory",
            "story_poster_chat_id": poster_chat_id.0,
            "story_id": story_id,
        }),
    )
}

/// `inputMessageLocation location:location live_period heading
/// proximity_alert_radius` (schema line 6145); a static location sends
/// `live_period` 0.
pub fn send_location(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    latitude: f64,
    longitude: f64,
    reply_to: Option<&SendReply>,
    options: &SendOptions,
) -> String {
    send_content(
        extra,
        chat_id,
        topic_id,
        reply_to,
        options,
        json!({
            "@type": "inputMessageLocation",
            "location": {
                "@type": "location",
                "latitude": latitude,
                "longitude": longitude,
                "horizontal_accuracy": 0.0,
            },
            "live_period": 0,
            "heading": 0,
            "proximity_alert_radius": 0,
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn dice_emoji_in_prefers_the_server_list() {
        let list = vec!["\u{1F3B2}".to_string(), "\u{1F3B3}".to_string()];
        assert_eq!(
            dice_emoji_in(" \u{1F3B3}\u{FE0F}", &list).as_deref(),
            Some("\u{1F3B3}")
        );
        // Not in the server's list: sends as text.
        assert_eq!(dice_emoji_in("\u{1F3B0}", &list), None);
        // No list yet: the built-in one decides.
        assert_eq!(
            dice_emoji_in("\u{1F3B0}", &[]).as_deref(),
            Some("\u{1F3B0}")
        );
        assert_eq!(dice_emoji_in("hi", &[]), None);
    }

    #[test]
    fn dice_emoji_matches_only_a_lone_dice_glyph() {
        assert_eq!(dice_emoji("\u{1F3B2}"), Some("\u{1F3B2}"));
        assert_eq!(dice_emoji("  \u{1F3B0}\n"), Some("\u{1F3B0}"));
        assert_eq!(dice_emoji("\u{26BD}\u{FE0F}"), Some("\u{26BD}"));
        assert_eq!(dice_emoji("\u{1F3B2}\u{1F3B2}"), None);
        assert_eq!(dice_emoji("roll \u{1F3B2}"), None);
        assert_eq!(dice_emoji("\u{1F600}"), None);
        assert_eq!(dice_emoji(""), None);
    }

    #[test]
    fn send_story_card_shape() {
        let json = send_story_card(
            RequestId(5),
            ChatId(77),
            None,
            ChatId(11),
            4,
            &SendOptions::default(),
        );
        let v = parse(&json);
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageStory");
        assert_eq!(v["input_message_content"]["story_poster_chat_id"], 11);
        assert_eq!(v["input_message_content"]["story_id"], 4);
    }

    #[test]
    fn send_dice_shape_matches_1_8_67() {
        let json = send_dice(
            RequestId(5),
            ChatId(77),
            None,
            "\u{1F3AF}",
            None,
            &SendOptions::default(),
        );
        let v = parse(&json);
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 77);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageDice");
        assert_eq!(v["input_message_content"]["emoji"], "\u{1F3AF}");
        assert_eq!(v["input_message_content"]["clear_draft"], false);
        assert!(v["reply_to"].is_null());
    }

    #[test]
    fn send_contact_carries_the_card() {
        let contact = ContactShare {
            phone_number: "+14155550123".into(),
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            user_id: 42,
        };
        let reply = SendReply::plain(crate::ids::MessageId(9));
        let json = send_contact_card(
            RequestId(6),
            ChatId(77),
            None,
            &contact,
            Some(&reply),
            &SendOptions {
                disable_notification: true,
                ..SendOptions::default()
            },
        );
        let v = parse(&json);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageContact");
        assert_eq!(content["contact"]["@type"], "contact");
        assert_eq!(content["contact"]["phone_number"], "+14155550123");
        assert_eq!(content["contact"]["first_name"], "Ada");
        assert_eq!(content["contact"]["last_name"], "Lovelace");
        assert_eq!(content["contact"]["user_id"], 42);
        assert_eq!(content["contact"]["vcard"], "");
        assert_eq!(v["options"]["disable_notification"], true);
        assert!(v["reply_to"].is_object());
    }

    #[test]
    fn send_location_is_static() {
        let json = send_location(
            RequestId(7),
            ChatId(77),
            None,
            37.7749,
            -122.4194,
            None,
            &SendOptions::default(),
        );
        let v = parse(&json);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageLocation");
        assert_eq!(content["location"]["latitude"], 37.7749);
        assert_eq!(content["location"]["longitude"], -122.4194);
        assert_eq!(content["live_period"], 0);
    }

    #[test]
    fn coordinates_are_validated() {
        assert_eq!(
            parse_coordinates("37.7749", " -122,4194 "),
            Ok((37.7749, -122.4194))
        );
        assert!(parse_coordinates("", "1").is_err());
        assert!(parse_coordinates("91", "0").is_err());
        assert!(parse_coordinates("0", "-181").is_err());
        assert!(parse_coordinates("NaN", "0").is_err());
        assert!(parse_coordinates("inf", "0").is_err());
        assert_eq!(parse_coordinates("-90", "180"), Ok((-90.0, 180.0)));
    }
}
