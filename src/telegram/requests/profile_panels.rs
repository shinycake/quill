//! B10: profile and contact panel request builders (birthday, personal
//! channel, private note, groups in common, similar channels, profile
//! photos). Schema references are TDLib 1.8.67 `schema/td_api.tl`.
use super::{message_send_options, message_topic_value, send_reply_value};
use crate::composer::SendOptions;
use crate::ids::{ChatId, RequestId};
use serde_json::{Value, json};

/// `setBirthdate birthdate:birthdate = Ok;` (line 14841); `None` removes
/// the birthday. The year is optional (0 = not shared), like `birthdate`
/// (line 868).
pub fn set_birthdate(extra: RequestId, birthdate: Option<(u8, u8, Option<i32>)>) -> String {
    let value = birthdate.map_or(Value::Null, |(day, month, year)| {
        json!({
            "@type": "birthdate",
            "day": day,
            "month": month,
            "year": year.unwrap_or(0),
        })
    });
    json!({
        "@type": "setBirthdate",
        "@extra": extra.as_extra(),
        "birthdate": value,
    })
    .to_string()
}

/// `getSuitablePersonalChats = Chats;` (line 11693): the channels that
/// can be shown as the personal channel.
pub fn get_suitable_personal_chats(extra: RequestId) -> String {
    json!({
        "@type": "getSuitablePersonalChats",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `setPersonalChat chat_id:int53 = Ok;` (line 14847); 0 removes it.
pub fn set_personal_chat(extra: RequestId, chat_id: i64) -> String {
    json!({
        "@type": "setPersonalChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id,
    })
    .to_string()
}

/// `setUserNote user_id:int53 note:formattedText = Ok;` (line 14553).
/// The note is plain text; an empty string clears it.
pub fn set_user_note(extra: RequestId, user_id: i64, note: &str) -> String {
    json!({
        "@type": "setUserNote",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "note": { "@type": "formattedText", "text": note, "entities": [] },
    })
    .to_string()
}

/// `getGroupsInCommon user_id:int53 offset_chat_id:int53 limit:int32 =
/// Chats;` (line 11818).
pub fn get_groups_in_common(
    extra: RequestId,
    user_id: i64,
    offset_chat_id: i64,
    limit: i32,
) -> String {
    json!({
        "@type": "getGroupsInCommon",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "offset_chat_id": offset_chat_id,
        "limit": limit,
    })
    .to_string()
}

/// `getChatSimilarChats chat_id:int53 = Chats;` (line 11627); the chat
/// must be a channel.
pub fn get_chat_similar_chats(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatSimilarChats",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `getUserProfilePhotos user_id:int53 offset:int32 limit:int32 =
/// ChatPhotos;` (line 14591); `limit` is at most 100.
pub fn get_user_profile_photos(extra: RequestId, user_id: i64, offset: i32, limit: i32) -> String {
    json!({
        "@type": "getUserProfilePhotos",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `setProfilePhoto` with `inputChatPhotoPrevious` (lines 1038, 14803):
/// "Set as main photo" for one of the current user's earlier photos.
pub fn set_profile_photo_previous(extra: RequestId, chat_photo_id: i64) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoPrevious",
            "chat_photo_id": chat_photo_id,
        },
        "is_public": false,
    })
    .to_string()
}

/// `sendMessage` + `inputMessageContact` (line 6151, `contact` line 640):
/// share a contact card into a chat. Rides
/// `RequestPurpose::SendMessage` like the other content-type sends.
pub fn send_contact(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    phone_number: &str,
    first_name: &str,
    last_name: &str,
    user_id: i64,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": send_reply_value(None),
        "options": message_send_options(&SendOptions::default()),
        "input_message_content": {
            "@type": "inputMessageContact",
            "contact": {
                "@type": "contact",
                "phone_number": phone_number,
                "first_name": first_name,
                "last_name": last_name,
                "vcard": "",
                "user_id": user_id,
            },
        }
    })
    .to_string()
}
