use crate::ids::{ChatId, RequestId};
use serde_json::json;

/// A5: `setName` (TDLib 1.8.67, `schema/td_api.tl:14823`). First name is
/// 1-64 chars; last name 0-64.
pub fn set_name(extra: RequestId, first_name: &str, last_name: &str) -> String {
    json!({
        "@type": "setName",
        "@extra": extra.as_extra(),
        "first_name": first_name,
        "last_name": last_name,
    })
    .to_string()
}

/// A5: `setBio` (TDLib 1.8.67, `schema/td_api.tl:14826`).
pub fn set_bio(extra: RequestId, bio: &str) -> String {
    json!({
        "@type": "setBio",
        "@extra": extra.as_extra(),
        "bio": bio,
    })
    .to_string()
}

/// A5: `setUsername` (TDLib 1.8.67, `schema/td_api.tl:14830`). Changes the
/// editable username; empty string removes it.
pub fn set_username(extra: RequestId, username: &str) -> String {
    json!({
        "@type": "setUsername",
        "@extra": extra.as_extra(),
        "username": username,
    })
    .to_string()
}

/// A5: `checkChatUsername` (TDLib 1.8.67, `schema/td_api.tl:11677`). For
/// the current user's own username the schema documents the private chat
/// with self as the chat id (TGX `EditUsernameController` behavior).
pub fn check_chat_username(extra: RequestId, chat_id: ChatId, username: &str) -> String {
    json!({
        "@type": "checkChatUsername",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "username": username,
    })
    .to_string()
}

/// A5: `reorderActiveUsernames` (TDLib 1.8.67, `schema/td_api.tl:14838`).
/// All currently active usernames, in the new order.
pub fn reorder_active_usernames(extra: RequestId, usernames: &[String]) -> String {
    json!({
        "@type": "reorderActiveUsernames",
        "@extra": extra.as_extra(),
        "usernames": usernames,
    })
    .to_string()
}

/// A5: `toggleUsernameIsActive` (TDLib 1.8.67, `schema/td_api.tl:14835`).
pub fn toggle_username_is_active(extra: RequestId, username: &str, is_active: bool) -> String {
    json!({
        "@type": "toggleUsernameIsActive",
        "@extra": extra.as_extra(),
        "username": username,
        "is_active": is_active,
    })
    .to_string()
}

/// A5: `setProfilePhoto` (TDLib 1.8.67, `schema/td_api.tl:14803`) with
/// `inputChatPhotoStatic` / `inputFileLocal` (schema lines 1042, 1039).
/// `is_public` true = the public photo, visible even when the main photo
/// is hidden by privacy settings.
pub fn set_profile_photo(extra: RequestId, photo_path: &str, is_public: bool) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoStatic",
            "photo": { "@type": "inputFileLocal", "path": photo_path },
        },
        "is_public": is_public,
    })
    .to_string()
}

/// A5: `deleteProfilePhoto` (TDLib 1.8.67, `schema/td_api.tl:14806`).
pub fn delete_profile_photo(extra: RequestId, profile_photo_id: i64) -> String {
    json!({
        "@type": "deleteProfilePhoto",
        "@extra": extra.as_extra(),
        "profile_photo_id": profile_photo_id,
    })
    .to_string()
}

/// Slice A12: `setProfileAccentColor profile_accent_color_id:int32
/// profile_background_custom_emoji_id:int64 = Ok;` (TDLib 1.8.67,
/// `schema/td_api.tl:14820`): "Changes the profile accent color and
/// background custom emoji for the current user". The `available_accent_color_ids`
/// from `updateProfileAccentColors` (schema:10964) are the settable ids;
/// pass -1 for no accent color. The caller preserves the current
/// `profile_background_custom_emoji_id` (Quill has no background-emoji
/// picker — a separate unchecked concern).
pub fn set_profile_accent_color(
    extra: RequestId,
    profile_accent_color_id: i32,
    profile_background_custom_emoji_id: i64,
) -> String {
    json!({
        "@type": "setProfileAccentColor",
        "@extra": extra.as_extra(),
        "profile_accent_color_id": profile_accent_color_id,
        "profile_background_custom_emoji_id": profile_background_custom_emoji_id,
    })
    .to_string()
}
