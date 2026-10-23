//! `sendMessage` bodies for the attach menu's Contact and Location items and
//! for the dice emoji (tdesktop `Api::SendAction` with
//! `SendMediaType::Contact` / `Location` and `MTPDinputMediaDice`), and the
//! Location picker's venue search and venue icons.

use super::{SendReply, message_send_options, message_topic_value, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, RequestId};
use crate::location_picker::{GeoPoint, Venue};
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

fn location_value(point: GeoPoint) -> Value {
    json!({
        "@type": "location",
        "latitude": point.latitude(),
        "longitude": point.longitude(),
        "horizontal_accuracy": 0.0,
    })
}

/// `inputMessageVenue venue:venue`: a place picked from the Location
/// picker's venue list.
pub fn send_venue(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    venue: &Venue,
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
            "@type": "inputMessageVenue",
            "venue": {
                "@type": "venue",
                "location": location_value(venue.point),
                "title": venue.title,
                "address": venue.address,
                "provider": venue.provider,
                "id": venue.id,
                "type": venue.venue_type,
            },
        }),
    )
}

/// `getInlineQueryResults` to the venue search bot with the place to look
/// around as `user_location` (tdesktop `LocationPicker::venuesRequest`).
pub fn get_venue_results(
    extra: RequestId,
    bot_user_id: i64,
    chat_id: ChatId,
    point: GeoPoint,
    query: &str,
) -> String {
    json!({
        "@type": "getInlineQueryResults",
        "@extra": extra.as_extra(),
        "bot_user_id": bot_user_id,
        "chat_id": chat_id.0,
        "user_location": location_value(point),
        "query": query,
        "offset": "",
    })
    .to_string()
}

/// `getRemoteFile` for an image at `url` (a venue category icon), which
/// TDLib downloads like any other thumbnail.
pub fn get_remote_thumbnail_file(extra: RequestId, url: &str) -> String {
    json!({
        "@type": "getRemoteFile",
        "@extra": extra.as_extra(),
        "remote_file_id": url,
        "file_type": {"@type": "fileTypeThumbnail"},
    })
    .to_string()
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
    fn send_venue_carries_the_place() {
        let venue = Venue {
            point: GeoPoint::new(48.85, 2.35).expect("finite"),
            title: "Cafe".into(),
            address: "Rue de Rivoli".into(),
            provider: "foursquare".into(),
            id: "v1".into(),
            venue_type: "food/cafe".into(),
        };
        let v = parse(&send_venue(
            RequestId(8),
            ChatId(77),
            None,
            &venue,
            None,
            &SendOptions::default(),
        ));
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessageVenue");
        assert_eq!(content["venue"]["@type"], "venue");
        assert_eq!(content["venue"]["location"]["latitude"], 48.85);
        assert_eq!(content["venue"]["title"], "Cafe");
        assert_eq!(content["venue"]["address"], "Rue de Rivoli");
        assert_eq!(content["venue"]["provider"], "foursquare");
        assert_eq!(content["venue"]["id"], "v1");
        assert_eq!(content["venue"]["type"], "food/cafe");
    }

    #[test]
    fn venue_search_sends_the_users_location() {
        let at = GeoPoint::new(48.85, 2.35).expect("finite");
        let v = parse(&get_venue_results(
            RequestId(9),
            99,
            ChatId(77),
            at,
            "pizza",
        ));
        assert_eq!(v["@type"], "getInlineQueryResults");
        assert_eq!(v["bot_user_id"], 99);
        assert_eq!(v["user_location"]["@type"], "location");
        assert_eq!(v["user_location"]["longitude"], 2.35);
        assert_eq!(v["query"], "pizza");
        assert_eq!(v["offset"], "");
    }

    #[test]
    fn venue_icon_is_a_remote_thumbnail() {
        let v = parse(&get_remote_thumbnail_file(RequestId(10), "https://x/y.png"));
        assert_eq!(v["@type"], "getRemoteFile");
        assert_eq!(v["remote_file_id"], "https://x/y.png");
        assert_eq!(v["file_type"]["@type"], "fileTypeThumbnail");
    }
}
