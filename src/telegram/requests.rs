use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::pins::{TDLIB_CMAKE_VERSION, TDLIB_GIT_COMMIT};
use serde_json::{Value, json};

pub struct SetTdlibParameters {
    pub use_test_dc: bool,
    pub database_directory: String,
    pub files_directory: String,
    pub database_encryption_key_b64: String,
    pub api_id: i32,
    pub api_hash: String,
    pub device_model: String,
    pub system_version: String,
    pub application_version: String,
    pub system_language_code: String,
}

impl SetTdlibParameters {
    pub fn to_json(&self, extra: RequestId) -> String {
        json!({
            "@type": "setTdlibParameters",
            "@extra": extra.as_extra(),
            "use_test_dc": self.use_test_dc,
            "database_directory": self.database_directory,
            "files_directory": self.files_directory,
            "database_encryption_key": self.database_encryption_key_b64,
            "use_file_database": true,
            "use_chat_info_database": true,
            "use_message_database": true,
            "use_secret_chats": false,
            "api_id": self.api_id,
            "api_hash": self.api_hash,
            "system_language_code": self.system_language_code,
            "device_model": self.device_model,
            "system_version": self.system_version,
            "application_version": self.application_version,
        })
        .to_string()
    }
}

pub fn get_authorization_state(extra: RequestId) -> String {
    json!({
        "@type": "getAuthorizationState",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `setAuthenticationPhoneNumber`. Callers must not log `phone_number`.
pub fn set_authentication_phone_number(extra: RequestId, phone_number: &str) -> String {
    json!({
        "@type": "setAuthenticationPhoneNumber",
        "@extra": extra.as_extra(),
        "phone_number": phone_number,
        "settings": {
            "@type": "phoneNumberAuthenticationSettings",
            "allow_flash_call": false,
            "allow_missed_call": false,
            "is_current_phone_number": false,
            "has_unknown_phone_number": false,
            "allow_sms_retriever_api": false,
            "firebase_authentication_settings": Value::Null,
            "authentication_tokens": []
        }
    })
    .to_string()
}

/// `checkAuthenticationCode`. Callers must not log `code`.
pub fn check_authentication_code(extra: RequestId, code: &str) -> String {
    json!({
        "@type": "checkAuthenticationCode",
        "@extra": extra.as_extra(),
        "code": code,
    })
    .to_string()
}

/// `checkAuthenticationPassword`. Callers must not log `password`.
pub fn check_authentication_password(extra: RequestId, password: &str) -> String {
    json!({
        "@type": "checkAuthenticationPassword",
        "@extra": extra.as_extra(),
        "password": password,
    })
    .to_string()
}

pub fn close_request(extra: RequestId) -> String {
    json!({
        "@type": "close",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn log_out(extra: RequestId) -> String {
    json!({
        "@type": "logOut",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

pub fn load_chats(extra: RequestId, limit: i32) -> String {
    json!({
        "@type": "loadChats",
        "@extra": extra.as_extra(),
        "chat_list": { "@type": "chatListMain" },
        "limit": limit,
    })
    .to_string()
}

/// `searchChats` (TDLib 1.8.67). Offline title/username search of known chats.
/// `type_filter` is null = all chat types (`SearchChatTypeFilter`).
pub fn search_chats(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
        "limit": limit,
    })
    .to_string()
}

/// `searchMessages` (TDLib 1.8.67). `chat_list` null = all lists (official
/// clients / Unigram); schema: only Main and Archive are searchable.
/// `filter` / `chat_type_filter` null = all messages / all chat types.
pub fn search_messages(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchMessages",
        "@extra": extra.as_extra(),
        "chat_list": Value::Null,
        "query": query,
        "offset": "",
        "limit": limit,
        "filter": Value::Null,
        "chat_type_filter": Value::Null,
        "min_date": 0,
        "max_date": 0,
    })
    .to_string()
}

/// `searchRecentlyFoundChats` (TDLib 1.8.67). Offline; empty `query` is the
/// recently-found list (official empty-search surface). Up to 50 chats.
pub fn search_recently_found_chats(extra: RequestId, query: &str, limit: i32) -> String {
    json!({
        "@type": "searchRecentlyFoundChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
        "limit": limit,
    })
    .to_string()
}

/// `searchChatMessages` (TDLib 1.8.67). In-chat text search; returns
/// `foundChatMessages`. `topic_id` / `sender_id` / `filter` null = all topics,
/// any sender, all message types (tdesktop ComposeSearch default).
/// First page: `from_message_id` 0 (schema: last message), `offset` 0.
pub fn search_chat_messages(
    extra: RequestId,
    chat_id: ChatId,
    query: &str,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "searchChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "query": query,
        "sender_id": Value::Null,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "filter": Value::Null,
    })
    .to_string()
}

/// `addRecentlyFoundChat` (TDLib 1.8.67). Official clients send this on select.
pub fn add_recently_found_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "addRecentlyFoundChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

pub fn get_chat_history(
    extra: RequestId,
    chat_id: ChatId,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
    only_local: bool,
) -> String {
    json!({
        "@type": "getChatHistory",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "only_local": only_local,
    })
    .to_string()
}

/// `openChat` — required before `viewMessages` can mark history as read.
pub fn open_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "openChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `closeChat` when leaving a conversation. Distinct from client `close`.
pub fn close_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "closeChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `viewMessages` (TDLib 1.8.67). `source` is `messageSourceChatHistory`.
/// `force_read` marks the ids read even if `openChat` has not completed.
pub fn view_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    force_read: bool,
) -> String {
    json!({
        "@type": "viewMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "source": { "@type": "messageSourceChatHistory" },
        "force_read": force_read,
    })
    .to_string()
}

/// `downloadFile` (TDLib 1.8.67). `synchronous: false` returns the current
/// `file` immediately; progress continues on `updateFile`.
pub fn download_file(extra: RequestId, file_id: FileId, priority: i32) -> String {
    json!({
        "@type": "downloadFile",
        "@extra": extra.as_extra(),
        "file_id": file_id.0,
        "priority": priority,
        "offset": 0,
        "limit": 0,
        "synchronous": false,
    })
    .to_string()
}

/// Same-chat reply: schema `inputMessageReplyToMessage` (quote null = whole message).
pub fn input_message_reply_to(message_id: Option<MessageId>) -> Value {
    match message_id {
        None => Value::Null,
        Some(id) => json!({
            "@type": "inputMessageReplyToMessage",
            "message_id": id.0,
            "quote": Value::Null,
            "checklist_task_id": 0,
            "poll_option_id": ""
        }),
    }
}

/// `sendMessage` for the pinned 1.8.67 schema: typed `topic_id`, not `message_thread_id`.
pub fn send_text(
    extra: RequestId,
    chat_id: ChatId,
    text: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": text,
                "entities": []
            },
            "link_preview_options": Value::Null,
            "clear_draft": true
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessagePhoto` / `inputPhoto` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_photo(
    extra: RequestId,
    chat_id: ChatId,
    path: &str,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessagePhoto",
            "photo": {
                "@type": "inputPhoto",
                "photo": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": Value::Null,
                "video": Value::Null,
                "added_sticker_file_ids": [],
                "width": 0,
                "height": 0
            },
            "caption": {
                "@type": "formattedText",
                "text": caption,
                "entities": []
            },
            "show_caption_above_media": false,
            "self_destruct_type": Value::Null,
            "has_spoiler": false
        }
    })
    .to_string()
}

/// `getInstalledStickerSets` for regular stickers (Unigram `StickerTypeRegular`).
pub fn get_installed_sticker_sets(extra: RequestId) -> String {
    json!({
        "@type": "getInstalledStickerSets",
        "@extra": extra.as_extra(),
        "sticker_type": { "@type": "stickerTypeRegular" },
    })
    .to_string()
}

/// `getStickerSet`. `set_id` is int64 — JSON string, not a float.
pub fn get_sticker_set(extra: RequestId, set_id: i64) -> String {
    json!({
        "@type": "getStickerSet",
        "@extra": extra.as_extra(),
        "set_id": set_id.to_string(),
    })
    .to_string()
}

/// Fields for `inputMessageSticker` (TDLib 1.8.67). Thumbnail matches Unigram `Thumbnail.ToInput`.
pub struct StickerSend<'a> {
    pub file_id: FileId,
    pub emoji: &'a str,
    pub width: i32,
    pub height: i32,
    pub thumb: Option<(FileId, i32, i32)>,
    pub reply_to: Option<MessageId>,
}

/// `sendMessage` + `inputMessageSticker` / `inputSticker` / `inputFileId` (1.8.67).
/// Thumbnail is `inputThumbnail` + `inputFileId` when the sticker has one (Unigram `Thumbnail.ToInput`).
pub fn send_sticker(extra: RequestId, chat_id: ChatId, sticker: StickerSend<'_>) -> String {
    let thumbnail = match sticker.thumb {
        Some((id, thumb_width, thumb_height)) if id.0 != 0 => json!({
            "@type": "inputThumbnail",
            "thumbnail": { "@type": "inputFileId", "id": id.0 },
            "width": thumb_width,
            "height": thumb_height,
        }),
        _ => Value::Null,
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to(sticker.reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageSticker",
            "sticker": {
                "@type": "inputSticker",
                "sticker": { "@type": "inputFileId", "id": sticker.file_id.0 },
                "thumbnail": thumbnail,
                "width": sticker.width,
                "height": sticker.height,
            },
            "emoji": sticker.emoji,
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageDocument` / `inputDocument` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_document(
    extra: RequestId,
    chat_id: ChatId,
    path: &str,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageDocument",
            "document": {
                "@type": "inputDocument",
                "document": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": Value::Null,
                "disable_content_type_detection": false
            },
            "caption": {
                "@type": "formattedText",
                "text": caption,
                "entities": []
            }
        }
    })
    .to_string()
}

/// `editMessageText` (TDLib 1.8.67). `reply_markup` null — bots only.
/// `input_message_content` must be `inputMessageText` (or `inputMessageRichMessage`).
pub fn edit_message_text(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    text: &str,
) -> String {
    json!({
        "@type": "editMessageText",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": {
                "@type": "formattedText",
                "text": text,
                "entities": []
            },
            "link_preview_options": Value::Null,
            "clear_draft": true
        }
    })
    .to_string()
}

/// `editMessageCaption` (TDLib 1.8.67). Caption-only media edit.
/// `show_caption_above_media` is false unless the original already inverted it.
pub fn edit_message_caption(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    caption: &str,
    show_caption_above_media: bool,
) -> String {
    json!({
        "@type": "editMessageCaption",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reply_markup": Value::Null,
        "caption": {
            "@type": "formattedText",
            "text": caption,
            "entities": []
        },
        "show_caption_above_media": show_caption_above_media
    })
    .to_string()
}

/// `deleteMessages` (TDLib 1.8.67). `revoke` true = delete for all members
/// (tdesktop `DeleteMessagesBox` / Unigram `DeleteMessagesPopup` default for
/// own outgoing that `can_be_deleted_for_all_users`). Always true in
/// supergroups, channels, and secret chats per schema.
pub fn delete_messages(
    extra: RequestId,
    chat_id: ChatId,
    message_ids: &[MessageId],
    revoke: bool,
) -> String {
    json!({
        "@type": "deleteMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "revoke": revoke
    })
    .to_string()
}

/// `forwardMessages` (TDLib 1.8.67). `send_copy` false keeps attribution
/// (`message.forward_info`) — official default, not hide-sender copy.
/// `remove_caption` is ignored unless `send_copy` is true. `topic_id` /
/// `options` null. Ids must already be strictly increasing (≤ 100).
pub fn forward_messages(
    extra: RequestId,
    chat_id: ChatId,
    from_chat_id: ChatId,
    message_ids: &[MessageId],
) -> String {
    json!({
        "@type": "forwardMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "from_chat_id": from_chat_id.0,
        "message_ids": message_ids.iter().map(|id| id.0).collect::<Vec<_>>(),
        "options": Value::Null,
        "send_copy": false,
        "remove_caption": false
    })
    .to_string()
}

/// `reactionTypeEmoji` (TDLib 1.8.67). Custom / paid stay out of this slice.
pub fn reaction_type_emoji(emoji: &str) -> Value {
    json!({
        "@type": "reactionTypeEmoji",
        "emoji": emoji
    })
}

/// `addMessageReaction` (TDLib 1.8.67). Chip / picker add: `is_big` false
/// (tdesktop InlineList click, not the big-animation double-click).
/// `update_recent_reactions` true matches the official picker.
pub fn add_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
    is_big: bool,
    update_recent_reactions: bool,
) -> String {
    json!({
        "@type": "addMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji),
        "is_big": is_big,
        "update_recent_reactions": update_recent_reactions
    })
    .to_string()
}

/// `removeMessageReaction` (TDLib 1.8.67). A chosen reaction can always be
/// removed (schema). Official chip click on `is_chosen` sends this.
pub fn remove_message_reaction(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    emoji: &str,
) -> String {
    json!({
        "@type": "removeMessageReaction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "reaction_type": reaction_type_emoji(emoji)
    })
    .to_string()
}

/// `pinChatMessage` (TDLib 1.8.67). Official Pin: notify when the chat allows
/// it (`disable_notification` false); pin for everyone (`only_for_self` false).
/// Schema: notifications are always disabled in channels and private chats.
pub fn pin_chat_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    disable_notification: bool,
    only_for_self: bool,
) -> String {
    json!({
        "@type": "pinChatMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "disable_notification": disable_notification,
        "only_for_self": only_for_self
    })
    .to_string()
}

/// `setChatNotificationSettings` (TDLib 1.8.67). Full settings object; callers
/// copy the chat's current settings and change only `mute_for`.
pub fn set_chat_notification_settings(
    extra: RequestId,
    chat_id: ChatId,
    settings: &crate::telegram::envelope::ChatNotificationSettings,
) -> String {
    json!({
        "@type": "setChatNotificationSettings",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "notification_settings": {
            "@type": "chatNotificationSettings",
            "use_default_mute_for": settings.use_default_mute_for,
            "mute_for": settings.mute_for,
            "use_default_sound": settings.use_default_sound,
            "sound_id": settings.sound_id,
            "use_default_show_preview": settings.use_default_show_preview,
            "show_preview": settings.show_preview,
            "use_default_mute_stories": settings.use_default_mute_stories,
            "mute_stories": settings.mute_stories,
            "use_default_story_sound": settings.use_default_story_sound,
            "story_sound_id": settings.story_sound_id,
            "use_default_show_story_poster": settings.use_default_show_story_poster,
            "show_story_poster": settings.show_story_poster,
            "use_default_disable_pinned_message_notifications": settings.use_default_disable_pinned_message_notifications,
            "disable_pinned_message_notifications": settings.disable_pinned_message_notifications,
            "use_default_disable_mention_notifications": settings.use_default_disable_mention_notifications,
            "disable_mention_notifications": settings.disable_mention_notifications
        }
    })
    .to_string()
}

/// `addChatToList` (TDLib 1.8.67). Main and Archive are mutually exclusive.
/// `sendChatAction` (TDLib 1.8.67). `typing` sends `chatActionTyping`;
/// otherwise `chatActionCancel` (Unigram `CancelTyping`). `topic_id` null,
/// `business_connection_id` empty (not a bot business connection).
pub fn send_chat_action(extra: RequestId, chat_id: ChatId, typing: bool) -> String {
    send_chat_action_kind(
        extra,
        chat_id,
        if typing {
            "chatActionTyping"
        } else {
            "chatActionCancel"
        },
    )
}

/// `sendChatAction` with an explicit `ChatAction` constructor (1.8.67).
pub fn send_chat_action_kind(extra: RequestId, chat_id: ChatId, action: &str) -> String {
    json!({
        "@type": "sendChatAction",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "business_connection_id": "",
        "action": { "@type": action }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
pub fn send_voice_note(
    extra: RequestId,
    chat_id: ChatId,
    path: &str,
    duration: i32,
    waveform_b64: &str,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    let caption_json = if caption.is_empty() {
        Value::Null
    } else {
        json!({
            "@type": "formattedText",
            "text": caption,
            "entities": []
        })
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVoiceNote",
            "voice_note": {
                "@type": "inputVoiceNote",
                "voice_note": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "duration": duration,
                "waveform": waveform_b64
            },
            "caption": caption_json,
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}

/// `openMessageContent` — user started listening to a voice note (1.8.67).
pub fn open_message_content(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "openMessageContent",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

pub fn add_chat_to_list(extra: RequestId, chat_id: ChatId, archive: bool) -> String {
    json!({
        "@type": "addChatToList",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "chat_list": {
            "@type": if archive { "chatListArchive" } else { "chatListMain" }
        }
    })
    .to_string()
}

/// `unpinChatMessage` (TDLib 1.8.67). Removes one pinned message.
pub fn unpin_chat_message(extra: RequestId, chat_id: ChatId, message_id: MessageId) -> String {
    json!({
        "@type": "unpinChatMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0
    })
    .to_string()
}

pub fn runtime_version_request() -> String {
    json!({
        "@type": "getOption",
        "name": "version",
    })
    .to_string()
}

pub fn expected_runtime_label() -> String {
    format!("{TDLIB_CMAKE_VERSION} ({TDLIB_GIT_COMMIT})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::RequestId;

    #[test]
    fn send_text_includes_topic_id_null() {
        let json = send_text(RequestId(9), ChatId(1), "hi", None);
        assert!(json.contains("\"topic_id\":null"));
        assert!(!json.contains("message_thread_id"));
        assert!(json.contains("\"@extra\":\"9\""));
        assert!(json.contains("\"reply_to\":null"));
    }

    #[test]
    fn send_text_reply_uses_input_message_reply_to_message() {
        let json = send_text(
            RequestId(10),
            ChatId(11),
            "sounds good",
            Some(MessageId(101)),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 101);
        assert_eq!(v["reply_to"]["quote"], Value::Null);
        assert_eq!(v["reply_to"]["checklist_task_id"], 0);
        assert_eq!(v["reply_to"]["poll_option_id"], "");
        assert!(!json.contains("inputMessageReplyToExternalMessage"));
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn send_photo_shape_matches_1_8_67() {
        let json = send_photo(
            RequestId(11),
            ChatId(7),
            "/tmp/picked.png",
            "CANARY_CAP",
            None,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "11");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessagePhoto");
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["photo"]["photo"]["path"],
            "/tmp/picked.png"
        );
        assert_eq!(
            v["input_message_content"]["photo"]["thumbnail"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["photo"]["video"], Value::Null);
        assert_eq!(v["input_message_content"]["photo"]["width"], 0);
        assert_eq!(v["input_message_content"]["photo"]["height"], 0);
        assert_eq!(v["input_message_content"]["caption"]["text"], "CANARY_CAP");
        assert_eq!(
            v["input_message_content"]["show_caption_above_media"],
            false
        );
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn send_sticker_uses_input_file_id_and_int64_set() {
        let json = send_sticker(
            RequestId(13),
            ChatId(7),
            StickerSend {
                file_id: FileId(41),
                emoji: "😀",
                width: 512,
                height: 512,
                thumb: Some((FileId(42), 128, 128)),
                reply_to: Some(MessageId(101)),
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageSticker");
        assert_eq!(v["input_message_content"]["emoji"], "😀");
        let sticker = &v["input_message_content"]["sticker"];
        assert_eq!(sticker["@type"], "inputSticker");
        assert_eq!(sticker["sticker"]["@type"], "inputFileId");
        assert_eq!(sticker["sticker"]["id"], 41);
        assert_eq!(sticker["width"], 512);
        assert_eq!(sticker["height"], 512);
        assert_eq!(sticker["thumbnail"]["@type"], "inputThumbnail");
        assert_eq!(sticker["thumbnail"]["thumbnail"]["id"], 42);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        let installed = get_installed_sticker_sets(RequestId(14));
        let installed: serde_json::Value = serde_json::from_str(&installed).unwrap();
        assert_eq!(installed["@type"], "getInstalledStickerSets");
        assert_eq!(installed["sticker_type"]["@type"], "stickerTypeRegular");
        let set = get_sticker_set(RequestId(15), 77);
        let set: serde_json::Value = serde_json::from_str(&set).unwrap();
        assert_eq!(set["set_id"], "77");
        let bare = send_sticker(
            RequestId(16),
            ChatId(7),
            StickerSend {
                file_id: FileId(41),
                emoji: "",
                width: 512,
                height: 512,
                thumb: None,
                reply_to: None,
            },
        );
        let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(
            bare["input_message_content"]["sticker"]["thumbnail"],
            Value::Null
        );
    }

    #[test]
    fn send_document_shape_matches_1_8_67() {
        let json = send_document(RequestId(12), ChatId(7), "/tmp/picked.txt", "", None);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageDocument");
        assert_eq!(
            v["input_message_content"]["document"]["document"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["document"]["document"]["path"],
            "/tmp/picked.txt"
        );
        assert_eq!(
            v["input_message_content"]["document"]["disable_content_type_detection"],
            false
        );
        assert_eq!(v["input_message_content"]["caption"]["text"], "");
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn extra_is_decimal_string_not_float() {
        let json = get_authorization_state(RequestId(9007199254740993));
        assert!(json.contains("\"@extra\":\"9007199254740993\""));
        assert!(!json.contains("\"@extra\":9007199254740993"));
    }

    #[test]
    fn set_phone_shape_does_not_use_message_thread_id() {
        let json = set_authentication_phone_number(RequestId(3), "+10001112222");
        assert!(json.contains("\"@type\":\"setAuthenticationPhoneNumber\""));
        assert!(json.contains("\"phone_number\":\"+10001112222\""));
        assert!(json.contains("phoneNumberAuthenticationSettings"));
        assert!(json.contains("\"@extra\":\"3\""));
    }

    #[test]
    fn check_authentication_code_shape() {
        let json = check_authentication_code(RequestId(4), "12345");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationCode");
        assert_eq!(v["@extra"], "4");
        assert_eq!(v["code"], "12345");
    }

    #[test]
    fn check_authentication_password_shape() {
        let json = check_authentication_password(RequestId(5), "unit-test-password");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "checkAuthenticationPassword");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["password"], "unit-test-password");
    }

    #[test]
    fn view_messages_uses_chat_history_source() {
        let json = view_messages(
            RequestId(6),
            ChatId(7),
            &[MessageId(11), MessageId(12)],
            true,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "viewMessages");
        assert_eq!(v["@extra"], "6");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["message_ids"], serde_json::json!([11, 12]));
        assert_eq!(v["source"]["@type"], "messageSourceChatHistory");
        assert_eq!(v["force_read"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn download_file_shape_matches_1_8_67() {
        let json = download_file(RequestId(12), FileId(44), 32);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "downloadFile");
        assert_eq!(v["@extra"], "12");
        assert_eq!(v["file_id"], 44);
        assert_eq!(v["priority"], 32);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 0);
        assert_eq!(v["synchronous"], false);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_chats_shape_matches_1_8_67() {
        let json = search_chats(RequestId(21), "alice", 20);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChats");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["query"], "alice");
        assert_eq!(v["type_filter"], Value::Null);
        assert_eq!(v["limit"], 20);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_messages_shape_matches_1_8_67() {
        let json = search_messages(RequestId(22), "hello", 20);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchMessages");
        assert_eq!(v["@extra"], "22");
        assert_eq!(v["chat_list"], Value::Null);
        assert_eq!(v["query"], "hello");
        assert_eq!(v["offset"], "");
        assert_eq!(v["limit"], 20);
        assert_eq!(v["filter"], Value::Null);
        assert_eq!(v["chat_type_filter"], Value::Null);
        assert_eq!(v["min_date"], 0);
        assert_eq!(v["max_date"], 0);
        assert!(!json.contains("chatListMain"));
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_recently_found_and_add_shapes_match_1_8_67() {
        let recents = search_recently_found_chats(RequestId(23), "", 50);
        let v: serde_json::Value = serde_json::from_str(&recents).unwrap();
        assert_eq!(v["@type"], "searchRecentlyFoundChats");
        assert_eq!(v["@extra"], "23");
        assert_eq!(v["query"], "");
        assert_eq!(v["type_filter"], Value::Null);
        assert_eq!(v["limit"], 50);
        let add = add_recently_found_chat(RequestId(24), ChatId(11));
        let v: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(v["@type"], "addRecentlyFoundChat");
        assert_eq!(v["@extra"], "24");
        assert_eq!(v["chat_id"], 11);
        assert!(!recents.contains("CANARY"));
        assert!(!add.contains("CANARY"));
    }

    #[test]
    fn edit_message_text_shape_matches_1_8_67() {
        let json = edit_message_text(RequestId(31), ChatId(11), MessageId(102), "edited body");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageText");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 102);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageText");
        assert_eq!(v["input_message_content"]["text"]["@type"], "formattedText");
        assert_eq!(v["input_message_content"]["text"]["text"], "edited body");
        assert_eq!(
            v["input_message_content"]["text"]["entities"],
            serde_json::json!([])
        );
        assert_eq!(
            v["input_message_content"]["link_preview_options"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["clear_draft"], true);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn edit_message_caption_shape_matches_1_8_67() {
        let json = edit_message_caption(RequestId(32), ChatId(11), MessageId(60), "new cap", false);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "editMessageCaption");
        assert_eq!(v["@extra"], "32");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 60);
        assert_eq!(v["reply_markup"], Value::Null);
        assert_eq!(v["caption"]["@type"], "formattedText");
        assert_eq!(v["caption"]["text"], "new cap");
        assert_eq!(v["caption"]["entities"], serde_json::json!([]));
        assert_eq!(v["show_caption_above_media"], false);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn delete_messages_shape_matches_1_8_67() {
        let json = delete_messages(RequestId(33), ChatId(11), &[MessageId(102)], true);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "deleteMessages");
        assert_eq!(v["@extra"], "33");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_ids"], serde_json::json!([102]));
        assert_eq!(v["revoke"], true);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn forward_messages_shape_matches_1_8_67() {
        let json = forward_messages(
            RequestId(34),
            ChatId(12),
            ChatId(11),
            &[MessageId(101), MessageId(102)],
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "forwardMessages");
        assert_eq!(v["@extra"], "34");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["from_chat_id"], 11);
        assert_eq!(v["message_ids"], serde_json::json!([101, 102]));
        assert_eq!(v["options"], Value::Null);
        assert_eq!(v["send_copy"], false);
        assert_eq!(v["remove_caption"], false);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
        assert!(!json.contains("inputMessageForwarded"));
    }

    #[test]
    fn add_and_remove_message_reaction_shapes_match_1_8_67() {
        let add = add_message_reaction(RequestId(35), ChatId(11), MessageId(101), "❤", false, true);
        let v: serde_json::Value = serde_json::from_str(&add).unwrap();
        assert_eq!(v["@type"], "addMessageReaction");
        assert_eq!(v["@extra"], "35");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["is_big"], false);
        assert_eq!(v["update_recent_reactions"], true);
        assert!(!add.contains("CANARY"));
        assert!(!add.contains("setMessageReactions"));
        assert!(!add.contains("reactionTypeCustomEmoji"));
        assert!(!add.contains("reactionTypePaid"));

        let remove = remove_message_reaction(RequestId(36), ChatId(11), MessageId(101), "❤");
        let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
        assert_eq!(v["@type"], "removeMessageReaction");
        assert_eq!(v["@extra"], "36");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert!(!remove.contains("is_big"));
        assert!(!remove.contains("update_recent_reactions"));
        assert!(!remove.contains("CANARY"));
    }

    #[test]
    fn pin_and_unpin_chat_message_shape_matches_1_8_67() {
        let pin = pin_chat_message(RequestId(40), ChatId(11), MessageId(101), false, false);
        let v: serde_json::Value = serde_json::from_str(&pin).unwrap();
        assert_eq!(v["@type"], "pinChatMessage");
        assert_eq!(v["@extra"], "40");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert_eq!(v["disable_notification"], false);
        assert_eq!(v["only_for_self"], false);
        assert!(!pin.contains("CANARY"));
        assert!(!pin.contains("unpinAllChatMessages"));

        let unpin = unpin_chat_message(RequestId(41), ChatId(11), MessageId(101));
        let v: serde_json::Value = serde_json::from_str(&unpin).unwrap();
        assert_eq!(v["@type"], "unpinChatMessage");
        assert_eq!(v["@extra"], "41");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["message_id"], 101);
        assert!(!unpin.contains("disable_notification"));
        assert!(!unpin.contains("only_for_self"));
        assert!(!unpin.contains("CANARY"));
    }

    #[test]
    fn search_chat_messages_shape_matches_1_8_67() {
        let json = search_chat_messages(RequestId(25), ChatId(11), "hello", MessageId(0), 0, 50);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChatMessages");
        assert_eq!(v["@extra"], "25");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["query"], "hello");
        assert_eq!(v["sender_id"], Value::Null);
        assert_eq!(v["from_message_id"], 0);
        assert_eq!(v["offset"], 0);
        assert_eq!(v["limit"], 50);
        assert_eq!(v["filter"], Value::Null);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn open_and_close_chat_are_distinct_from_client_close() {
        let open = open_chat(RequestId(8), ChatId(3));
        let close = close_chat(RequestId(9), ChatId(3));
        let client_close = close_request(RequestId(10));
        assert!(open.contains("\"@type\":\"openChat\""));
        assert!(close.contains("\"@type\":\"closeChat\""));
        assert!(client_close.contains("\"@type\":\"close\""));
        assert!(!client_close.contains("closeChat"));
        assert_eq!(serde_json::from_str::<Value>(&close).unwrap()["chat_id"], 3);
    }

    #[test]
    fn send_chat_action_typing_and_cancel_match_1_8_67() {
        let typing = send_chat_action(RequestId(42), ChatId(11), true);
        let v: serde_json::Value = serde_json::from_str(&typing).unwrap();
        assert_eq!(v["@type"], "sendChatAction");
        assert_eq!(v["@extra"], "42");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["business_connection_id"], "");
        assert_eq!(v["action"]["@type"], "chatActionTyping");
        assert!(!typing.contains("CANARY"));
        assert!(!typing.contains("message_thread_id"));

        let cancel = send_chat_action(RequestId(43), ChatId(11), false);
        let v: serde_json::Value = serde_json::from_str(&cancel).unwrap();
        assert_eq!(v["action"]["@type"], "chatActionCancel");
        assert!(!cancel.contains("chatActionTyping"));

        let recording =
            send_chat_action_kind(RequestId(44), ChatId(11), "chatActionRecordingVoiceNote");
        let v: serde_json::Value = serde_json::from_str(&recording).unwrap();
        assert_eq!(v["action"]["@type"], "chatActionRecordingVoiceNote");
    }

    #[test]
    fn send_voice_note_shape_matches_1_8_67() {
        let json = send_voice_note(
            RequestId(15),
            ChatId(7),
            "/tmp/picked.ogg",
            3,
            "BASE64WAVE",
            "",
            Some(MessageId(9)),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "15");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVoiceNote");
        assert_eq!(
            v["input_message_content"]["voice_note"]["@type"],
            "inputVoiceNote"
        );
        assert_eq!(
            v["input_message_content"]["voice_note"]["voice_note"]["@type"],
            "inputFileLocal"
        );
        assert_eq!(
            v["input_message_content"]["voice_note"]["voice_note"]["path"],
            "/tmp/picked.ogg"
        );
        assert_eq!(v["input_message_content"]["voice_note"]["duration"], 3);
        assert_eq!(
            v["input_message_content"]["voice_note"]["waveform"],
            "BASE64WAVE"
        );
        assert_eq!(v["input_message_content"]["caption"], Value::Null);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("inputMessageVideoNote"));
        assert!(!json.contains("CANARY"));
    }
}
