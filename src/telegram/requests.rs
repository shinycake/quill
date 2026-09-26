use crate::ids::{ChatId, FileId, MessageId, RequestId, TopicId};
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
    load_chats_list(extra, json!({ "@type": "chatListMain" }), limit)
}

/// `loadChats` for an arbitrary chat list (Phase 7.1: `chatListFolder`).
/// Schema 1.8.67: `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595).
pub fn load_chats_list(extra: RequestId, chat_list: Value, limit: i32) -> String {
    json!({
        "@type": "loadChats",
        "@extra": extra.as_extra(),
        "chat_list": chat_list,
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

/// `searchPublicChats` (TDLib 1.8.67, schema line 11609). Public username /
/// title lookup across all public chats (private chats, supergroups,
/// channels) — unlike `searchChats`, not limited to known chats.
/// `type_filter` is null = all chat types. Returns `chats`.
pub fn search_public_chats(extra: RequestId, query: &str) -> String {
    json!({
        "@type": "searchPublicChats",
        "@extra": extra.as_extra(),
        "query": query,
        "type_filter": Value::Null,
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

/// Typed `topic_id` JSON for TDLib 1.8.67 requests (schema: `MessageTopic`
/// constructors at `schema/td_api.tl:3001-3010`). `TopicId::None` encodes as
/// JSON null ("all topics"), matching the existing null-encoding convention.
pub fn topic_id_json(topic: &TopicId) -> Value {
    match topic {
        TopicId::None => Value::Null,
        TopicId::Forum { forum_topic_id } => json!({
            "@type": "messageTopicForum",
            "forum_topic_id": forum_topic_id,
        }),
        TopicId::DirectMessages {
            direct_messages_chat_topic_id,
        } => json!({
            "@type": "messageTopicDirectMessages",
            "direct_messages_chat_topic_id": direct_messages_chat_topic_id,
        }),
        TopicId::SavedMessages {
            saved_messages_topic_id,
        } => json!({
            "@type": "messageTopicSavedMessages",
            "saved_messages_topic_id": saved_messages_topic_id,
        }),
    }
}

/// `searchChatMessages` (TDLib 1.8.67). In-chat text search; returns
/// `foundChatMessages`. `topic` null-equivalent (`TopicId::None`) = all topics,
/// `sender_id` / `filter` null = any sender, all message types (tdesktop
/// ComposeSearch default). Passing `TopicId::Forum` with an empty `query`
/// lists that topic's messages — this is how per-topic history is fetched
/// (`getChatHistory` has no `topic_id` parameter, schema line 11829).
/// First page: `from_message_id` 0 (schema: last message), `offset` 0.
pub fn search_chat_messages(
    extra: RequestId,
    chat_id: ChatId,
    topic: &TopicId,
    query: &str,
    from_message_id: MessageId,
    offset: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "searchChatMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": topic_id_json(topic),
        "query": query,
        "sender_id": Value::Null,
        "from_message_id": from_message_id.0,
        "offset": offset,
        "limit": limit,
        "filter": Value::Null,
    })
    .to_string()
}

/// `getForumTopics` (TDLib 1.8.67, `schema/td_api.tl:12701`):
/// `getForumTopics chat_id:int53 query:string offset_date:int32
/// offset_message_id:int53 offset_forum_topic_id:int32 limit:int32 =
/// ForumTopics`. First page: empty query, all offsets 0.
pub fn get_forum_topics(
    extra: RequestId,
    chat_id: ChatId,
    query: &str,
    offset_date: i32,
    offset_message_id: MessageId,
    offset_forum_topic_id: i32,
    limit: i32,
) -> String {
    json!({
        "@type": "getForumTopics",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "query": query,
        "offset_date": offset_date,
        "offset_message_id": offset_message_id.0,
        "offset_forum_topic_id": offset_forum_topic_id,
        "limit": limit,
    })
    .to_string()
}

/// `getSupergroup` (TDLib 1.8.67, `schema/td_api.tl:11510`):
/// `getSupergroup supergroup_id:int53 = Supergroup`. Response carries
/// `supergroup.is_forum` (schema line 2746), which is how Quill learns a
/// supergroup is a forum (`chatTypeSupergroup` itself has no forum flag).
pub fn get_supergroup(extra: RequestId, supergroup_id: i64) -> String {
    json!({
        "@type": "getSupergroup",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
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

/// `getMe` (TDLib 1.8.67). Sent once so `getChatMember` can resolve the
/// current user; only the response id is kept.
pub fn get_me(extra: RequestId) -> String {
    json!({
        "@type": "getMe",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getChatMember` for the current user in a channel (TDLib 1.8.67). Response
/// is `chatMember`.
pub fn get_chat_member(extra: RequestId, chat_id: ChatId, user_id: i64) -> String {
    json!({
        "@type": "getChatMember",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "member_id": { "@type": "messageSenderUser", "user_id": user_id },
    })
    .to_string()
}

/// `getUserFullInfo` for a bot user (TDLib 1.8.67,
/// `getUserFullInfo user_id:int53 = UserFullInfo`). Response is
/// `userFullInfo`; `bot_info` feeds the bot panel.
pub fn get_user_full_info(extra: RequestId, user_id: i64) -> String {
    json!({
        "@type": "getUserFullInfo",
        "@extra": extra.as_extra(),
        "user_id": user_id,
    })
    .to_string()
}

/// Phase 6: `getContacts` (TDLib 1.8.67, `schema/td_api.tl:14520`):
/// `getContacts = Users;` — no parameters. Response is `users`
/// (`total_count:int32 user_ids:vector<int53>`, line 2471); the user
/// objects themselves arrive via `updateUser`.
pub fn get_contacts(extra: RequestId) -> String {
    json!({
        "@type": "getContacts",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// Phase 6: `addContact` (TDLib 1.8.67, `schema/td_api.tl:14513`):
/// `addContact user_id:int53 contact:importedContact
/// share_phone_number:Bool = Ok;`
/// with `importedContact phone_number:string first_name:string
/// last_name:string note:formattedText = ImportedContact` (line 7382).
/// The note is sent as an empty `formattedText` — Quill has no contact
/// notes UI.
pub fn add_contact(
    extra: RequestId,
    user_id: i64,
    phone_number: &str,
    first_name: &str,
    last_name: &str,
) -> String {
    json!({
        "@type": "addContact",
        "@extra": extra.as_extra(),
        "user_id": user_id,
        "contact": {
            "@type": "importedContact",
            "phone_number": phone_number,
            "first_name": first_name,
            "last_name": last_name,
            "note": { "@type": "formattedText", "text": "", "entities": [] },
        },
        "share_phone_number": false,
    })
    .to_string()
}

/// Phase 6: `getSupergroupFullInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:11513`):
/// `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo;`
/// Response is `supergroupFullInfo` (carries no id — correlated via the
/// pending request in `Session::apply`).
pub fn get_supergroup_full_info(extra: RequestId, supergroup_id: i64) -> String {
    json!({
        "@type": "getSupergroupFullInfo",
        "@extra": extra.as_extra(),
        "supergroup_id": supergroup_id,
    })
    .to_string()
}

/// Phase 3.3: `getCommands` for a bot's global (default) command scope
/// (TDLib 1.8.67, `schema/td_api.tl:14953`):
/// `getCommands scope:BotCommandScope language_code:string = BotCommands;`
/// The schema annotates the method "for bots only", so a user session
/// gets an `error` answer instead of `botCommands`; the driver absorbs it
/// and the `/` menu falls back to the `botInfo` commands. The scope is
/// `botCommandScopeDefault` (line 10360, "a scope covering all users");
/// the chat-specific commands already arrive via `botInfo`.
pub fn get_commands(extra: RequestId) -> String {
    json!({
        "@type": "getCommands",
        "@extra": extra.as_extra(),
        "scope": null,
        "language_code": "",
    })
    .to_string()
}

/// `joinChat` for a public channel (TDLib 1.8.67). Response is
/// `ChatJoinResult`.
pub fn join_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "joinChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `leaveChat` for a channel (TDLib 1.8.67). Response is `ok`.
pub fn leave_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "leaveChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `getChatSponsoredMessages` (TDLib 1.8.67). For channel chats (and chats
/// with bots); rows render Sponsored / Recommended per `is_recommended`.
pub fn get_chat_sponsored_messages(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatSponsoredMessages",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
    })
    .to_string()
}

/// `reportChatSponsoredMessage` (TDLib 1.8.67). `option_id` is the base64
/// `reportOption.id`; empty for the initial request.
pub fn report_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    option_id: &str,
) -> String {
    json!({
        "@type": "reportChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "option_id": option_id,
    })
    .to_string()
}

/// `viewSponsoredChat` (TDLib 1.8.67). `unique_id` is the `sponsoredChat`
/// unique id (from sponsored search results).
pub fn view_sponsored_chat(extra: RequestId, sponsored_chat_unique_id: i64) -> String {
    json!({
        "@type": "viewSponsoredChat",
        "@extra": extra.as_extra(),
        "sponsored_chat_unique_id": sponsored_chat_unique_id,
    })
    .to_string()
}

/// `clickChatSponsoredMessage` (TDLib 1.8.67). Sent when the user opens a
/// sponsored message's sponsor link/button (`is_media_click = false`) or its
/// media (`is_media_click = true`). `from_fullscreen` is true when the media
/// was opened from the fullscreen viewer.
pub fn click_chat_sponsored_message(
    extra: RequestId,
    chat_id: ChatId,
    message_id: i64,
    is_media_click: bool,
    from_fullscreen: bool,
) -> String {
    json!({
        "@type": "clickChatSponsoredMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id,
        "is_media_click": is_media_click,
        "from_fullscreen": from_fullscreen,
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

/// `setChatDraftMessage` (TDLib 1.8.67). `topic_id` null updates the chat itself.
/// `draft_message` null removes the draft. Text uses `draftMessageContentText`.
/// `date` 0 and `effect_id` `"0"` match Unigram's `DraftMessage` constructor
/// (`int64` is a JSON string). `link_preview_options` null keeps the default.
pub fn set_chat_draft_message(
    extra: RequestId,
    chat_id: ChatId,
    text: Option<&str>,
    reply_to: Option<MessageId>,
) -> String {
    let draft_message = match text {
        None if reply_to.is_none() => Value::Null,
        text => {
            let body = text.unwrap_or("");
            json!({
                "@type": "draftMessage",
                "reply_to": input_message_reply_to(reply_to),
                "date": 0,
                "content": {
                    "@type": "draftMessageContentText",
                    "text": {
                        "@type": "formattedText",
                        "text": body,
                        "entities": []
                    },
                    "link_preview_options": Value::Null
                },
                "effect_id": "0",
                "suggested_post_info": Value::Null
            })
        }
    };
    json!({
        "@type": "setChatDraftMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "draft_message": draft_message,
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
/// Parity slice 4: posting into a forum topic passes
/// `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67, lines
/// 12200 and 3004); `None` sends null (no topic).
fn message_topic_value(topic_id: Option<i32>) -> Value {
    match topic_id {
        Some(forum_topic_id) => json!({
            "@type": "messageTopicForum",
            "forum_topic_id": forum_topic_id,
        }),
        None => Value::Null,
    }
}

pub fn send_text(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    text: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
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

fn formatted_caption(caption: &str) -> Value {
    json!({
        "@type": "formattedText",
        "text": caption,
        "entities": []
    })
}

/// `inputMessagePhoto` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_photo(path: &str, caption: &str) -> Value {
    json!({
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
        "caption": formatted_caption(caption),
        "show_caption_above_media": false,
        "self_destruct_type": Value::Null,
        "has_spoiler": false
    })
}

/// `sendMessage` + `inputMessagePhoto` / `inputPhoto` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_photo(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_photo(path, caption)
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
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
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
        "topic_id": message_topic_value(sticker.topic_id),
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

/// Fields for `inputMessageAnimation` (TDLib 1.8.67). Saved GIFs use `inputFileId`.
/// Thumbnail stays null: `inputThumbnail` says file_id upload is not supported, and a
/// saved animation is already known to the server (Unigram sends the file id only).
pub struct AnimationSend {
    pub file_id: FileId,
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub reply_to: Option<MessageId>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `getSavedAnimations` — saved GIFs, no query and no third-party key.
pub fn get_saved_animations(extra: RequestId) -> String {
    json!({
        "@type": "getSavedAnimations",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `sendMessage` + `inputMessageAnimation` / `inputAnimation` / `inputFileId` (1.8.67).
pub fn send_animation(extra: RequestId, chat_id: ChatId, animation: AnimationSend) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(animation.topic_id),
        "reply_to": input_message_reply_to(animation.reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageAnimation",
            "animation": {
                "@type": "inputAnimation",
                "animation": { "@type": "inputFileId", "id": animation.file_id.0 },
                "thumbnail": Value::Null,
                "added_sticker_file_ids": [],
                "duration": animation.duration,
                "width": animation.width,
                "height": animation.height,
            },
            "caption": Value::Null,
            "show_caption_above_media": false,
            "has_spoiler": false,
        }
    })
    .to_string()
}

/// Fields for `inputVideo` (TDLib 1.8.67). Thumbnail stays null: the schema says
/// pass null to skip thumbnail uploading, and TDLib fills one for small files.
pub struct VideoSend {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub supports_streaming: bool,
}

/// `inputMessageVideo` body (TDLib 1.8.67). Shared by `sendMessage` and `sendMessageAlbum`.
pub fn input_message_video(path: &str, video: &VideoSend, caption: &str) -> Value {
    json!({
        "@type": "inputMessageVideo",
        "video": {
            "@type": "inputVideo",
            "video": {
                "@type": "inputFileLocal",
                "path": path
            },
            "thumbnail": Value::Null,
            "cover": Value::Null,
            "start_timestamp": 0,
            "added_sticker_file_ids": [],
            "duration": video.duration,
            "width": video.width,
            "height": video.height,
            "supports_streaming": video.supports_streaming
        },
        "caption": formatted_caption(caption),
        "show_caption_above_media": false,
        "self_destruct_type": Value::Null,
        "has_spoiler": false
    })
}

/// `inputVideoNote.thumbnail` when a JPEG was written locally. `None` is JSON null
/// (schema: pass null to skip thumbnail uploading).
pub struct VideoNoteThumbnailSend {
    pub path: String,
    pub width: i32,
    pub height: i32,
}

/// Fields for `inputVideoNote` (TDLib 1.8.67). `duration` is 0–60. `length` is
/// the square side, positive and at most 640.
pub struct VideoNoteSend {
    pub duration: i32,
    pub length: i32,
    pub thumbnail: Option<VideoNoteThumbnailSend>,
}

fn input_video_note_thumbnail(thumb: Option<&VideoNoteThumbnailSend>) -> Value {
    match thumb {
        Some(thumb) => json!({
            "@type": "inputThumbnail",
            "thumbnail": {
                "@type": "inputFileLocal",
                "path": thumb.path
            },
            "width": thumb.width,
            "height": thumb.height
        }),
        None => Value::Null,
    }
}

/// `sendMessage` + `inputMessageVideoNote` / `inputVideoNote` / `inputFileLocal` (1.8.67).
/// No caption: the constructor is `video_note` and `self_destruct_type` only.
/// `path` must already be an explicitly picked local file.
pub fn send_video_note(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    note: &VideoNoteSend,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVideoNote",
            "video_note": {
                "@type": "inputVideoNote",
                "video_note": {
                    "@type": "inputFileLocal",
                    "path": path
                },
                "thumbnail": input_video_note_thumbnail(note.thumbnail.as_ref()),
                "duration": note.duration,
                "length": note.length
            },
            "self_destruct_type": Value::Null
        }
    })
    .to_string()
}

/// `sendMessage` + `inputMessageVideo` / `inputVideo` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_video(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    video: &VideoSend,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": input_message_video(path, video, caption)
    })
    .to_string()
}

/// `sendMessageAlbum` (TDLib 1.8.67). 2–10 contents, same `show_caption_above_media`.
/// Caption sits on the last item (`show_caption_above_media` is false).
pub fn send_message_album(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    reply_to: Option<MessageId>,
    input_message_contents: Vec<Value>,
) -> String {
    json!({
        "@type": "sendMessageAlbum",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
        "reply_to": input_message_reply_to(reply_to),
        "options": Value::Null,
        "input_message_contents": input_message_contents
    })
    .to_string()
}

/// `sendMessage` + `inputMessageDocument` / `inputDocument` / `inputFileLocal` (1.8.67).
/// `path` must already be an explicitly picked local file — never a JSON `local.path`.
pub fn send_document(
    extra: RequestId,
    chat_id: ChatId,
    topic_id: Option<i32>,
    path: &str,
    caption: &str,
    reply_to: Option<MessageId>,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(topic_id),
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

/// `setPollAnswer` (TDLib 1.8.67, `schema/td_api.tl:12932`): `option_ids` are
/// 0-based indexes into the poll's option list (not the `pollOption.id`
/// strings). Response is `ok`; the new counts arrive via `updatePoll`.
pub fn set_poll_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    option_ids: &[i32],
) -> String {
    json!({
        "@type": "setPollAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "option_ids": option_ids,
    })
    .to_string()
}

/// Fields for `inputMessagePoll` (TDLib 1.8.67, `schema/td_api.tl:6193`).
/// Quiz creation stays out of this slice (Phase 4.2) — polls are created as
/// `inputPollTypeRegular` (schema line 481) even for quiz-flagged drafts.
pub struct PollSend<'a> {
    pub question: &'a str,
    pub options: &'a [&'a str],
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub reply_to: Option<MessageId>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessagePoll` / `inputPollOption` / `inputPollTypeRegular`
/// (TDLib 1.8.67). Options must already be trimmed and non-empty (2–10);
/// the question 1–255 chars — validated by `PollDraft::validate` before this
/// is called. `allows_revoting` is true for regular polls (official clients
/// let the user change their vote); `members_only`, `country_codes`,
/// `shuffle_options`, `hide_results_until_closes`, `open_period`,
/// `close_date` all stay at the zero value.
pub fn send_poll(extra: RequestId, chat_id: ChatId, poll: PollSend<'_>) -> String {
    let options: Vec<Value> = poll
        .options
        .iter()
        .map(|text| {
            json!({
                "@type": "inputPollOption",
                "text": {
                    "@type": "formattedText",
                    "text": text,
                    "entities": []
                },
                "media": Value::Null
            })
        })
        .collect();
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(poll.topic_id),
        "reply_to": input_message_reply_to(poll.reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessagePoll",
            "question": {
                "@type": "formattedText",
                "text": poll.question,
                "entities": []
            },
            "options": options,
            "description": Value::Null,
            "media": Value::Null,
            "is_anonymous": poll.is_anonymous,
            "allows_multiple_answers": poll.allows_multiple_answers,
            "allows_revoting": true,
            "members_only": false,
            "country_codes": [],
            "shuffle_options": false,
            "hide_results_until_closes": false,
            "type": {
                "@type": "inputPollTypeRegular",
                "allow_adding_options": false
            },
            "open_period": 0,
            "close_date": 0,
            "is_closed": false
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
            "clear_draft": false
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

/// `getCallbackQueryAnswer` (TDLib 1.8.67, `schema/td_api.tl:13138`).
/// Pressing an `inlineKeyboardButtonTypeCallback` button: sends the callback
/// query to the bot; TDLib returns `callbackQueryAnswer`. (Not
/// `answerCallbackQuery` — that one is bots-only per its schema doc
/// comment.) `payload` is `callbackQueryPayloadData` (line 7737); schema
/// `bytes` is base64 in the JSON interface.
pub fn get_callback_query_answer(
    extra: RequestId,
    chat_id: ChatId,
    message_id: MessageId,
    data: &[u8],
) -> String {
    use base64::Engine;
    json!({
        "@type": "getCallbackQueryAnswer",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "message_id": message_id.0,
        "payload": {
            "@type": "callbackQueryPayloadData",
            "data": base64::engine::general_purpose::STANDARD.encode(data),
        },
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

/// `getSavedNotificationSounds` (TDLib 1.8.67, line 13647): the user's saved
/// notification sounds. "If a sound isn't in the list, then default sound
/// needs to be used."
pub fn get_saved_notification_sounds(extra: RequestId) -> String {
    json!({
        "@type": "getSavedNotificationSounds",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

/// `getScopeNotificationSettings` (TDLib 1.8.67, line 13662).
pub fn get_scope_notification_settings(
    extra: RequestId,
    scope: crate::telegram::envelope::NotificationSettingsScope,
) -> String {
    json!({
        "@type": "getScopeNotificationSettings",
        "@extra": extra.as_extra(),
        "scope": { "@type": scope.type_name() },
    })
    .to_string()
}

/// `setScopeNotificationSettings` (TDLib 1.8.67, line 13665). Full settings
/// object; callers copy the scope's current settings and change one field.
pub fn set_scope_notification_settings(
    extra: RequestId,
    scope: crate::telegram::envelope::NotificationSettingsScope,
    settings: &crate::telegram::envelope::ScopeNotificationSettings,
) -> String {
    json!({
        "@type": "setScopeNotificationSettings",
        "@extra": extra.as_extra(),
        "scope": { "@type": scope.type_name() },
        "notification_settings": {
            "@type": "scopeNotificationSettings",
            "mute_for": settings.mute_for,
            "sound_id": settings.sound_id,
            "show_preview": settings.show_preview,
            "use_default_mute_stories": settings.use_default_mute_stories,
            "mute_stories": settings.mute_stories,
            "story_sound_id": settings.story_sound_id,
            "show_story_poster": settings.show_story_poster,
            "disable_pinned_message_notifications": settings.disable_pinned_message_notifications,
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
/// Voice-note send parameters. Bundled into a struct so the send constructor
/// stays under clippy's argument limit as topic/reply support grows.
pub struct VoiceNoteSend<'a> {
    pub path: &'a str,
    pub duration: i32,
    pub waveform_b64: &'a str,
    pub caption: &'a str,
    pub reply_to: Option<MessageId>,
    /// Parity slice 4: forum topic the send is addressed to (`None` = no topic).
    pub topic_id: Option<i32>,
}

/// `sendMessage` + `inputMessageVoiceNote` / `inputVoiceNote` / `inputFileLocal`.
/// `path` must already be an explicitly recorded or picked file.
/// `waveform_b64` is the 5-bit waveform as TDLib `bytes` (base64); empty if unknown.
pub fn send_voice_note(extra: RequestId, chat_id: ChatId, voice: VoiceNoteSend<'_>) -> String {
    let caption_json = if voice.caption.is_empty() {
        Value::Null
    } else {
        json!({
            "@type": "formattedText",
            "text": voice.caption,
            "entities": []
        })
    };
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": message_topic_value(voice.topic_id),
        "reply_to": input_message_reply_to(voice.reply_to),
        "options": Value::Null,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageVoiceNote",
            "voice_note": {
                "@type": "inputVoiceNote",
                "voice_note": {
                    "@type": "inputFileLocal",
                    "path": voice.path
                },
                "duration": voice.duration,
                "waveform": voice.waveform_b64
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

/// Phase 9.1: `loadActiveStories` (TDLib 1.8.67, `schema/td_api.tl:13762`).
/// The loaded stories arrive as `updateChatActiveStories` updates — they
/// feed the story tray above the chat list.
pub fn load_active_stories(extra: RequestId) -> String {
    json!({
        "@type": "loadActiveStories",
        "@extra": extra.as_extra(),
        "story_list": {"@type": "storyListMain"}
    })
    .to_string()
}

/// Phase 9.1: `getChatActiveStories` (TDLib 1.8.67,
/// `schema/td_api.tl:13768`). Response is `chatActiveStories`; handled like
/// the `updateChatActiveStories` update.
pub fn get_chat_active_stories(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatActiveStories",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0
    })
    .to_string()
}

/// Phase 9.1: `getStory` (TDLib 1.8.67, `schema/td_api.tl:13695`).
/// `only_local: false` — the viewer wants the full content.
pub fn get_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "getStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "only_local": false
    })
    .to_string()
}

/// Phase 9.1: `openStory` (TDLib 1.8.67, `schema/td_api.tl:13794`) — the
/// user opened a story for viewing. Response is `ok`.
pub fn open_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "openStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.1: `closeStory` (TDLib 1.8.67, `schema/td_api.tl:13799`) — the
/// user closed a story. Response is `ok`.
pub fn close_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "closeStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.2: `getStoryAvailableReactions` (TDLib 1.8.67,
/// `schema/td_api.tl:13802`) — emoji reactions the story picker can offer.
/// `row_size` must be 5–25; the viewer requests 10. Response is
/// `availableReactions`.
pub fn get_story_available_reactions(extra: RequestId, row_size: i32) -> String {
    json!({
        "@type": "getStoryAvailableReactions",
        "@extra": extra.as_extra(),
        "row_size": row_size
    })
    .to_string()
}

/// Phase 9.2: `setStoryReaction` (TDLib 1.8.67, `schema/td_api.tl:13809`) —
/// changes the user's chosen reaction on a story. `emoji: None` removes the
/// reaction (`reaction_type: null`); `Some("❤")` sets it. Only
/// `reactionTypeEmoji` is offered (custom emoji is Premium-only; paid
/// reactions can't be set — schema comment). `update_recent_reactions: true`
/// matches the official picker click. Not supported for live stories (the
/// driver gates that). Response is `ok`.
pub fn set_story_reaction(
    extra: RequestId,
    chat_id: ChatId,
    story_id: i32,
    emoji: Option<&str>,
) -> String {
    let reaction_type = match emoji {
        Some(emoji) => reaction_type_emoji(emoji),
        None => Value::Null,
    };
    json!({
        "@type": "setStoryReaction",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id,
        "reaction_type": reaction_type,
        "update_recent_reactions": true
    })
    .to_string()
}

/// Phase 9.2: `deleteStory` (TDLib 1.8.67, `schema/td_api.tl:13754`) —
/// deletes a story posted by the current user (`story.can_be_deleted`
/// gates the button). Response is `ok`; the deletion lands as
/// `updateStoryDeleted`.
pub fn delete_story(extra: RequestId, chat_id: ChatId, story_id: i32) -> String {
    json!({
        "@type": "deleteStory",
        "@extra": extra.as_extra(),
        "story_poster_chat_id": chat_id.0,
        "story_id": story_id
    })
    .to_string()
}

/// Phase 9.2: story reply target — `inputMessageReplyToStory` (TDLib 1.8.67,
/// `schema/td_api.tl:3099`). Replying to a story sends a message to the
/// story poster quoting the story.
pub fn input_message_reply_to_story(poster_chat_id: ChatId, story_id: i32) -> Value {
    json!({
        "@type": "inputMessageReplyToStory",
        "story_poster_chat_id": poster_chat_id.0,
        "story_id": story_id
    })
}

/// Phase 9.2: `sendMessage` with `inputMessageReplyToStory` — a reply to a
/// story, sent to the poster chat (`story.can_be_replied` gates it).
pub fn send_text_story_reply(
    extra: RequestId,
    chat_id: ChatId,
    poster_chat_id: ChatId,
    story_id: i32,
    text: &str,
) -> String {
    json!({
        "@type": "sendMessage",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "topic_id": Value::Null,
        "reply_to": input_message_reply_to_story(poster_chat_id, story_id),
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

pub fn add_chat_to_list(extra: RequestId, chat_id: ChatId, archive: bool) -> String {
    add_chat_to_list_value(
        extra,
        chat_id,
        json!({ "@type": if archive { "chatListArchive" } else { "chatListMain" } }),
    )
}

/// `addChatToList` (TDLib 1.8.67, `schema/td_api.tl:13352`) with an explicit
/// `ChatList` value. Parity slice: adding a chat to a folder uses
/// `{"@type":"chatListFolder","chat_folder_id":<id>}` — the schema doc says
/// "Use getChatListsToAddChat to get suitable chat lists".
pub fn add_chat_to_list_value(extra: RequestId, chat_id: ChatId, chat_list: Value) -> String {
    json!({
        "@type": "addChatToList",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
        "chat_list": chat_list
    })
    .to_string()
}

/// The `chatFolder` constructor body (TDLib 1.8.67, `schema/td_api.tl:3476`)
/// for `createChatFolder` / `editChatFolder`. Quill sends `icon: null`
/// (default icon; `getChatFolderDefaultIconName` is an async TDLib call
/// clients may use — the null default is what the schema allows),
/// rendering and folder invite links are out of scope. Folder names are
/// 1–12 characters without line feeds (schema doc on `chatFolderName`).
pub fn chat_folder_json(spec: &crate::telegram::envelope::ChatFolderSpec) -> Value {
    json!({
        "@type": "chatFolder",
        "name": {
            "@type": "chatFolderName",
            "text": { "@type": "formattedText", "text": spec.name, "entities": [] },
            "animate_custom_emoji": false,
        },
        "icon": null,
        "color_id": -1,
        "is_shareable": false,
        "pinned_chat_ids": spec.pinned_chat_ids,
        "included_chat_ids": spec.included_chat_ids,
        "excluded_chat_ids": spec.excluded_chat_ids,
        "exclude_muted": spec.exclude_muted,
        "exclude_read": spec.exclude_read,
        "exclude_archived": spec.exclude_archived,
        "include_contacts": spec.include_contacts,
        "include_non_contacts": spec.include_non_contacts,
        "include_bots": spec.include_bots,
        "include_groups": spec.include_groups,
        "include_channels": spec.include_channels,
    })
}

/// `createChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13358`). Response is
/// `chatFolderInfo`.
pub fn create_chat_folder(
    extra: RequestId,
    spec: &crate::telegram::envelope::ChatFolderSpec,
) -> String {
    json!({
        "@type": "createChatFolder",
        "@extra": extra.as_extra(),
        "folder": chat_folder_json(spec),
    })
    .to_string()
}

/// `editChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13361`). Response is
/// `chatFolderInfo`.
pub fn edit_chat_folder(
    extra: RequestId,
    folder_id: i32,
    spec: &crate::telegram::envelope::ChatFolderSpec,
) -> String {
    json!({
        "@type": "editChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
        "folder": chat_folder_json(spec),
    })
    .to_string()
}

/// `deleteChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13364`). Response is
/// `ok`. `leave_chat_ids` are chats to leave; they must be pinned or
/// always included in the folder (empty = keep all chats in the main list).
pub fn delete_chat_folder(extra: RequestId, folder_id: i32, leave_chat_ids: &[i64]) -> String {
    json!({
        "@type": "deleteChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
        "leave_chat_ids": leave_chat_ids,
    })
    .to_string()
}

/// `reorderChatFolders` (TDLib 1.8.67, `schema/td_api.tl:13373`). Response
/// is `ok`. `main_chat_list_position` is always 0 — a non-zero position is
/// Premium-only per the schema doc, and Quill keeps Main first.
pub fn reorder_chat_folders(extra: RequestId, folder_ids: &[i32]) -> String {
    json!({
        "@type": "reorderChatFolders",
        "@extra": extra.as_extra(),
        "chat_folder_ids": folder_ids,
        "main_chat_list_position": 0,
    })
    .to_string()
}

/// `toggleChatFolderTags` (TDLib 1.8.67, `schema/td_api.tl:13376`).
/// Response is `ok`.
pub fn toggle_chat_folder_tags(extra: RequestId, are_tags_enabled: bool) -> String {
    json!({
        "@type": "toggleChatFolderTags",
        "@extra": extra.as_extra(),
        "are_tags_enabled": are_tags_enabled,
    })
    .to_string()
}

/// `getChatFolder` (TDLib 1.8.67, `schema/td_api.tl:13355`). Response is
/// the full `chatFolder` spec — used to prefill the edit dialog.
pub fn get_chat_folder(extra: RequestId, folder_id: i32) -> String {
    json!({
        "@type": "getChatFolder",
        "@extra": extra.as_extra(),
        "chat_folder_id": folder_id,
    })
    .to_string()
}

/// `getChatListsToAddChat` (TDLib 1.8.67, `schema/td_api.tl:13347`): "Returns
/// chat lists to which the chat can be added. This is an offline method".
/// Response is `chatLists`. Drives the per-chat folder picker (and the
/// archive/unarchive menu) as the schema intends.
pub fn get_chat_lists_to_add_chat(extra: RequestId, chat_id: ChatId) -> String {
    json!({
        "@type": "getChatListsToAddChat",
        "@extra": extra.as_extra(),
        "chat_id": chat_id.0,
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
    fn get_commands_shape_matches_1_8_67() {
        // `getCommands scope:BotCommandScope language_code:string =
        // BotCommands` (schema 1.8.67 line 14953); a null scope selects the
        // default scope (`botCommandScopeDefault`, line 10360) and an empty
        // language code is allowed.
        let json = get_commands(RequestId(9));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCommands");
        assert_eq!(v["@extra"], "9");
        assert!(v["scope"].is_null());
        assert_eq!(v["language_code"], "");
    }

    #[test]
    fn send_text_includes_topic_id_null() {
        let json = send_text(RequestId(9), ChatId(1), None, "hi", None);
        assert!(json.contains("\"topic_id\":null"));
        assert!(!json.contains("message_thread_id"));
        assert!(json.contains("\"@extra\":\"9\""));
        assert!(json.contains("\"reply_to\":null"));
    }

    #[test]
    fn send_text_topic_id_uses_message_topic_forum() {
        // Parity slice 4: `sendMessage.topic_id` (schema 1.8.67, line 12200)
        // takes `messageTopicForum{forum_topic_id}` (line 3004).
        let json = send_text(RequestId(9), ChatId(16), Some(2), "hi", None);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 16);
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 2);
    }

    #[test]
    fn send_text_reply_uses_input_message_reply_to_message() {
        let json = send_text(
            RequestId(10),
            ChatId(11),
            None,
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
            None,
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
                topic_id: None,
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
                topic_id: None,
            },
        );
        let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(
            bare["input_message_content"]["sticker"]["thumbnail"],
            Value::Null
        );
    }

    #[test]
    fn send_animation_uses_input_animation_and_saved_list_has_no_query() {
        let json = send_animation(
            RequestId(21),
            ChatId(7),
            AnimationSend {
                file_id: FileId(33),
                duration: 2,
                width: 240,
                height: 140,
                reply_to: Some(MessageId(101)),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "21");
        assert_eq!(v["input_message_content"]["@type"], "inputMessageAnimation");
        let animation = &v["input_message_content"]["animation"];
        assert_eq!(animation["@type"], "inputAnimation");
        assert_eq!(animation["animation"]["@type"], "inputFileId");
        assert_eq!(animation["animation"]["id"], 33);
        assert_eq!(animation["thumbnail"], Value::Null);
        assert_eq!(animation["added_sticker_file_ids"], serde_json::json!([]));
        assert_eq!(animation["duration"], 2);
        assert_eq!(animation["width"], 240);
        assert_eq!(animation["height"], 140);
        assert_eq!(v["input_message_content"]["caption"], Value::Null);
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(v["reply_to"]["message_id"], 101);
        assert!(!json.contains("tenor"));
        let saved = get_saved_animations(RequestId(22));
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved["@type"], "getSavedAnimations");
        assert_eq!(saved["@extra"], "22");
        assert!(saved.get("query").is_none());
    }

    #[test]
    fn send_video_shape_matches_1_8_67() {
        let json = send_video(
            RequestId(17),
            ChatId(7),
            None,
            "/tmp/picked.mp4",
            &VideoSend {
                duration: 1,
                width: 320,
                height: 180,
                supports_streaming: true,
            },
            "CANARY_VIDEO",
            Some(MessageId(9)),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "17");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVideo");
        let video = &v["input_message_content"]["video"];
        assert_eq!(video["@type"], "inputVideo");
        assert_eq!(video["video"]["@type"], "inputFileLocal");
        assert_eq!(video["video"]["path"], "/tmp/picked.mp4");
        assert_eq!(video["thumbnail"], Value::Null);
        assert_eq!(video["cover"], Value::Null);
        assert_eq!(video["start_timestamp"], 0);
        assert_eq!(video["added_sticker_file_ids"], serde_json::json!([]));
        assert_eq!(video["duration"], 1);
        assert_eq!(video["width"], 320);
        assert_eq!(video["height"], 180);
        assert_eq!(video["supports_streaming"], true);
        assert_eq!(
            v["input_message_content"]["caption"]["text"],
            "CANARY_VIDEO"
        );
        assert_eq!(
            v["input_message_content"]["show_caption_above_media"],
            false
        );
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert_eq!(v["input_message_content"]["has_spoiler"], false);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("inputMessageVideoNote"));
        assert!(!json.contains("api_hash"));
    }

    #[test]
    fn send_video_note_shape_matches_1_8_67() {
        let json = send_video_note(
            RequestId(18),
            ChatId(7),
            None,
            "/tmp/round.mp4",
            &VideoNoteSend {
                duration: 1,
                length: 240,
                thumbnail: Some(VideoNoteThumbnailSend {
                    path: "/tmp/round.jpg".into(),
                    width: 240,
                    height: 240,
                }),
            },
            Some(MessageId(9)),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "18");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["input_message_content"]["@type"], "inputMessageVideoNote");
        let note = &v["input_message_content"]["video_note"];
        assert_eq!(note["@type"], "inputVideoNote");
        assert_eq!(note["video_note"]["@type"], "inputFileLocal");
        assert_eq!(note["video_note"]["path"], "/tmp/round.mp4");
        assert_eq!(note["thumbnail"]["@type"], "inputThumbnail");
        assert_eq!(note["thumbnail"]["thumbnail"]["path"], "/tmp/round.jpg");
        assert_eq!(note["thumbnail"]["width"], 240);
        assert_eq!(note["thumbnail"]["height"], 240);
        assert_eq!(note["duration"], 1);
        assert_eq!(note["length"], 240);
        assert_eq!(
            v["input_message_content"]["self_destruct_type"],
            Value::Null
        );
        assert!(v["input_message_content"].get("caption").is_none());
        assert_eq!(v["reply_to"]["message_id"], 9);
        assert!(!json.contains("api_hash"));

        let bare = send_video_note(
            RequestId(19),
            ChatId(7),
            None,
            "/tmp/round.mp4",
            &VideoNoteSend {
                duration: 0,
                length: 1,
                thumbnail: None,
            },
            None,
        );
        let bare: serde_json::Value = serde_json::from_str(&bare).unwrap();
        assert_eq!(
            bare["input_message_content"]["video_note"]["thumbnail"],
            Value::Null
        );
        assert_eq!(bare["reply_to"], Value::Null);
    }

    #[test]
    fn send_document_shape_matches_1_8_67() {
        let json = send_document(RequestId(12), ChatId(7), None, "/tmp/picked.txt", "", None);
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
    fn get_contacts_shape_matches_1_8_67() {
        // `getContacts = Users;` — no parameters (schema 1.8.67, line 14520).
        let json = get_contacts(RequestId(60));
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getContacts");
        assert_eq!(v["@extra"], "60");
        assert_eq!(v.as_object().unwrap().len(), 2);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn add_contact_shape_matches_1_8_67() {
        // `addContact user_id:int53 contact:importedContact
        // share_phone_number:Bool = Ok;` (schema 1.8.67, line 14513) with
        // `importedContact phone_number:string first_name:string
        // last_name:string note:formattedText` (line 7382).
        let json = add_contact(
            RequestId(61),
            31,
            "+15550131",
            "CANARY-first",
            "CANARY-last",
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "addContact");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["user_id"], 31);
        let contact = &v["contact"];
        assert_eq!(contact["@type"], "importedContact");
        assert_eq!(contact["phone_number"], "+15550131");
        assert_eq!(contact["first_name"], "CANARY-first");
        assert_eq!(contact["last_name"], "CANARY-last");
        assert_eq!(contact["note"]["@type"], "formattedText");
        assert_eq!(contact["note"]["text"], "");
        assert_eq!(v["share_phone_number"], false);
    }

    #[test]
    fn get_supergroup_full_info_shape_matches_1_8_67() {
        // `getSupergroupFullInfo supergroup_id:int53 = SupergroupFullInfo;`
        // (schema 1.8.67, line 11513).
        let json = get_supergroup_full_info(RequestId(62), 77);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getSupergroupFullInfo");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["supergroup_id"], 77);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn search_public_chats_shape_matches_1_8_67() {
        // `searchPublicChats query:string type_filter:SearchChatTypeFilter =
        // Chats` (schema 1.8.67, line 11609); type_filter null = all types.
        let json = search_public_chats(RequestId(23), "quill");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchPublicChats");
        assert_eq!(v["@extra"], "23");
        assert_eq!(v["query"], "quill");
        assert!(v["type_filter"].is_null());
        let schema = include_str!("../../schema/td_api.tl");
        let line = schema
            .lines()
            .find(|l| l.starts_with("searchPublicChats "))
            .expect("searchPublicChats in schema");
        assert_eq!(
            line,
            "searchPublicChats query:string type_filter:SearchChatTypeFilter = Chats;"
        );
    }

    #[test]
    fn load_chats_folder_list_shape_matches_1_8_67() {
        // `loadChats chat_list:ChatList limit:int32 = Ok` (line 11595) with
        // `chatListFolder` (line 3524) — Phase 7.1 folder load.
        let json = load_chats_list(
            RequestId(24),
            json!({ "@type": "chatListFolder", "chat_folder_id": 3 }),
            100,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "loadChats");
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 3);
        assert_eq!(v["limit"], 100);
    }

    #[test]
    fn folder_request_shapes_match_1_8_67() {
        use crate::telegram::envelope::ChatFolderSpec;
        let spec = ChatFolderSpec {
            name: "Work".into(),
            pinned_chat_ids: vec![11],
            included_chat_ids: vec![12],
            excluded_chat_ids: vec![],
            exclude_muted: true,
            exclude_read: false,
            exclude_archived: false,
            include_contacts: true,
            include_non_contacts: false,
            include_bots: false,
            include_groups: true,
            include_channels: false,
        };
        // `createChatFolder folder:chatFolder = ChatFolderInfo` (line 13358).
        let v: serde_json::Value =
            serde_json::from_str(&create_chat_folder(RequestId(30), &spec)).unwrap();
        assert_eq!(v["@type"], "createChatFolder");
        assert_eq!(v["folder"]["@type"], "chatFolder");
        assert_eq!(v["folder"]["name"]["text"]["text"], "Work");
        assert!(v["folder"]["icon"].is_null());
        assert_eq!(v["folder"]["color_id"], -1);
        assert_eq!(v["folder"]["pinned_chat_ids"], json!([11]));
        assert_eq!(v["folder"]["included_chat_ids"], json!([12]));
        assert!(v["folder"]["exclude_muted"].as_bool().unwrap());
        assert!(v["folder"]["include_contacts"].as_bool().unwrap());
        assert!(v["folder"]["include_groups"].as_bool().unwrap());
        // `editChatFolder chat_folder_id:int32 folder:chatFolder =
        // ChatFolderInfo` (line 13361).
        let v: serde_json::Value =
            serde_json::from_str(&edit_chat_folder(RequestId(31), 7, &spec)).unwrap();
        assert_eq!(v["@type"], "editChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        assert_eq!(v["folder"]["@type"], "chatFolder");
        // `deleteChatFolder chat_folder_id:int32 leave_chat_ids:vector<int53>
        // = Ok` (line 13364).
        let v: serde_json::Value =
            serde_json::from_str(&delete_chat_folder(RequestId(32), 7, &[12])).unwrap();
        assert_eq!(v["@type"], "deleteChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        assert_eq!(v["leave_chat_ids"], json!([12]));
        // `reorderChatFolders chat_folder_ids:vector<int32>
        // main_chat_list_position:int32 = Ok` (line 13373).
        let v: serde_json::Value =
            serde_json::from_str(&reorder_chat_folders(RequestId(33), &[2, 1])).unwrap();
        assert_eq!(v["@type"], "reorderChatFolders");
        assert_eq!(v["chat_folder_ids"], json!([2, 1]));
        assert_eq!(v["main_chat_list_position"], 0);
        // `toggleChatFolderTags are_tags_enabled:Bool = Ok` (line 13376).
        let v: serde_json::Value =
            serde_json::from_str(&toggle_chat_folder_tags(RequestId(34), true)).unwrap();
        assert_eq!(v["@type"], "toggleChatFolderTags");
        assert!(v["are_tags_enabled"].as_bool().unwrap());
        // `getChatFolder chat_folder_id:int32 = ChatFolder` (line 13355).
        let v: serde_json::Value =
            serde_json::from_str(&get_chat_folder(RequestId(35), 7)).unwrap();
        assert_eq!(v["@type"], "getChatFolder");
        assert_eq!(v["chat_folder_id"], 7);
        // `getChatListsToAddChat chat_id:int53 = ChatLists` (line 13347).
        let v: serde_json::Value =
            serde_json::from_str(&get_chat_lists_to_add_chat(RequestId(36), ChatId(12))).unwrap();
        assert_eq!(v["@type"], "getChatListsToAddChat");
        assert_eq!(v["chat_id"], 12);
        // `addChatToList` with `chatListFolder` (line 13352 + 3524).
        let v: serde_json::Value = serde_json::from_str(&add_chat_to_list_value(
            RequestId(37),
            ChatId(12),
            json!({ "@type": "chatListFolder", "chat_folder_id": 7 }),
        ))
        .unwrap();
        assert_eq!(v["@type"], "addChatToList");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["chat_list"]["@type"], "chatListFolder");
        assert_eq!(v["chat_list"]["chat_folder_id"], 7);
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
    fn get_callback_query_answer_shape_matches_1_8_67() {
        // `getCallbackQueryAnswer chat_id:int53 message_id:int53
        // payload:CallbackQueryPayload = CallbackQueryAnswer` (schema line
        // 13138); `callbackQueryPayloadData data:bytes` (line 7737) with
        // base64 `bytes` in JSON.
        let json = get_callback_query_answer(RequestId(51), ChatId(21), MessageId(301), &[1, 2, 3]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getCallbackQueryAnswer");
        assert_eq!(v["@extra"], "51");
        assert_eq!(v["chat_id"], 21);
        assert_eq!(v["message_id"], 301);
        assert_eq!(v["payload"]["@type"], "callbackQueryPayloadData");
        assert_eq!(v["payload"]["data"], "AQID");
        assert!(!json.contains("answerCallbackQuery"));
        assert!(!json.contains("CANARY"));
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
        assert_eq!(v["input_message_content"]["clear_draft"], false);
        assert!(!json.contains("CANARY"));
        assert!(!json.contains("message_thread_id"));
    }

    #[test]
    fn set_chat_draft_message_matches_1_8_67() {
        let save = set_chat_draft_message(
            RequestId(41),
            ChatId(11),
            Some("meet at 6"),
            Some(MessageId(101)),
        );
        let v: serde_json::Value = serde_json::from_str(&save).unwrap();
        assert_eq!(v["@type"], "setChatDraftMessage");
        assert_eq!(v["@extra"], "41");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["topic_id"], Value::Null);
        assert_eq!(v["draft_message"]["@type"], "draftMessage");
        assert_eq!(v["draft_message"]["date"], 0);
        assert_eq!(v["draft_message"]["effect_id"], "0");
        assert_eq!(v["draft_message"]["suggested_post_info"], Value::Null);
        assert_eq!(
            v["draft_message"]["content"]["@type"],
            "draftMessageContentText"
        );
        assert_eq!(v["draft_message"]["content"]["text"]["text"], "meet at 6");
        assert_eq!(
            v["draft_message"]["content"]["link_preview_options"],
            Value::Null
        );
        assert_eq!(
            v["draft_message"]["reply_to"]["@type"],
            "inputMessageReplyToMessage"
        );
        assert_eq!(v["draft_message"]["reply_to"]["message_id"], 101);
        let clear = set_chat_draft_message(RequestId(42), ChatId(11), None, None);
        let v: serde_json::Value = serde_json::from_str(&clear).unwrap();
        assert_eq!(v["draft_message"], Value::Null);
        assert!(!save.contains("CANARY"));
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
        let json = search_chat_messages(
            RequestId(25),
            ChatId(11),
            &TopicId::None,
            "hello",
            MessageId(0),
            0,
            50,
        );
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
    fn search_chat_messages_forum_topic_uses_message_topic_forum() {
        let json = search_chat_messages(
            RequestId(26),
            ChatId(11),
            &TopicId::Forum { forum_topic_id: 7 },
            "",
            MessageId(0),
            0,
            50,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "searchChatMessages");
        assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
        assert_eq!(v["topic_id"]["forum_topic_id"], 7);
        assert_eq!(v["query"], "");
    }

    #[test]
    fn topic_id_json_variants() {
        assert_eq!(topic_id_json(&TopicId::None), Value::Null);
        let dm = topic_id_json(&TopicId::DirectMessages {
            direct_messages_chat_topic_id: 42,
        });
        assert_eq!(dm["@type"], "messageTopicDirectMessages");
        assert_eq!(dm["direct_messages_chat_topic_id"], 42);
        let sm = topic_id_json(&TopicId::SavedMessages {
            saved_messages_topic_id: 9,
        });
        assert_eq!(sm["@type"], "messageTopicSavedMessages");
        assert_eq!(sm["saved_messages_topic_id"], 9);
    }

    #[test]
    fn get_forum_topics_shape_matches_1_8_67() {
        let json = get_forum_topics(RequestId(31), ChatId(12), "", 0, MessageId(0), 0, 100);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getForumTopics");
        assert_eq!(v["@extra"], "31");
        assert_eq!(v["chat_id"], 12);
        assert_eq!(v["query"], "");
        assert_eq!(v["offset_date"], 0);
        assert_eq!(v["offset_message_id"], 0);
        assert_eq!(v["offset_forum_topic_id"], 0);
        assert_eq!(v["limit"], 100);
        assert!(!json.contains("CANARY"));
    }

    #[test]
    fn get_supergroup_shape_matches_1_8_67() {
        let json = get_supergroup(RequestId(32), 77);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "getSupergroup");
        assert_eq!(v["@extra"], "32");
        assert_eq!(v["supergroup_id"], 77);
        assert!(!json.contains("CANARY"));
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
    fn sponsored_message_requests_match_1_8_67() {
        let fetch = get_chat_sponsored_messages(RequestId(50), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&fetch).unwrap();
        assert_eq!(v["@type"], "getChatSponsoredMessages");
        assert_eq!(v["@extra"], "50");
        assert_eq!(v["chat_id"], 13);

        let report = report_chat_sponsored_message(RequestId(51), ChatId(13), 777, "");
        let v: serde_json::Value = serde_json::from_str(&report).unwrap();
        assert_eq!(v["@type"], "reportChatSponsoredMessage");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["message_id"], 777);
        assert_eq!(v["option_id"], "");

        let with_option = report_chat_sponsored_message(RequestId(52), ChatId(13), 777, "b3B0aW9u");
        let v: serde_json::Value = serde_json::from_str(&with_option).unwrap();
        assert_eq!(v["option_id"], "b3B0aW9u");

        let view = view_sponsored_chat(RequestId(53), 4242);
        let v: serde_json::Value = serde_json::from_str(&view).unwrap();
        assert_eq!(v["@type"], "viewSponsoredChat");
        assert_eq!(v["sponsored_chat_unique_id"], 4242);

        let click = click_chat_sponsored_message(RequestId(54), ChatId(13), 9001, false, false);
        let v: serde_json::Value = serde_json::from_str(&click).unwrap();
        assert_eq!(v["@type"], "clickChatSponsoredMessage");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["message_id"], 9001);
        assert_eq!(v["is_media_click"], false);
        assert_eq!(v["from_fullscreen"], false);

        let media_click =
            click_chat_sponsored_message(RequestId(55), ChatId(13), 9002, true, false);
        let v: serde_json::Value = serde_json::from_str(&media_click).unwrap();
        assert_eq!(v["is_media_click"], true);
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
            VoiceNoteSend {
                path: "/tmp/picked.ogg",
                duration: 3,
                waveform_b64: "BASE64WAVE",
                caption: "",
                reply_to: Some(MessageId(9)),
                topic_id: None,
            },
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

#[cfg(test)]
mod channel_requests_tests {
    use super::*;

    #[test]
    fn channel_request_shapes_match_1_8_67() {
        let me = get_me(RequestId(60));
        let v: serde_json::Value = serde_json::from_str(&me).unwrap();
        assert_eq!(v["@type"], "getMe");
        assert_eq!(v["@extra"], "60");

        let member = get_chat_member(RequestId(61), ChatId(13), 777);
        let v: serde_json::Value = serde_json::from_str(&member).unwrap();
        assert_eq!(v["@type"], "getChatMember");
        assert_eq!(v["@extra"], "61");
        assert_eq!(v["chat_id"], 13);
        assert_eq!(v["member_id"]["@type"], "messageSenderUser");
        assert_eq!(v["member_id"]["user_id"], 777);

        let join = join_chat(RequestId(62), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&join).unwrap();
        assert_eq!(v["@type"], "joinChat");
        assert_eq!(v["@extra"], "62");
        assert_eq!(v["chat_id"], 13);

        let leave = leave_chat(RequestId(63), ChatId(13));
        let v: serde_json::Value = serde_json::from_str(&leave).unwrap();
        assert_eq!(v["@type"], "leaveChat");
        assert_eq!(v["@extra"], "63");
        assert_eq!(v["chat_id"], 13);
    }

    #[test]
    fn story_request_shapes_match_1_8_67() {
        // Phase 9.2 story reactions / picker / delete / reply builders.
        let set = set_story_reaction(RequestId(70), ChatId(11), 7, Some("❤"));
        let v: serde_json::Value = serde_json::from_str(&set).unwrap();
        // `setStoryReaction story_poster_chat_id:int53 story_id:int32
        // reaction_type:ReactionType update_recent_reactions:Bool = Ok`
        // (schema 1.8.67 line 13809).
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["@extra"], "70");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 7);
        assert_eq!(v["reaction_type"]["@type"], "reactionTypeEmoji");
        assert_eq!(v["reaction_type"]["emoji"], "❤");
        assert_eq!(v["update_recent_reactions"], true);

        // Removing sends `reaction_type: null` (schema comment, line 13809).
        let remove = set_story_reaction(RequestId(71), ChatId(11), 7, None);
        let v: serde_json::Value = serde_json::from_str(&remove).unwrap();
        assert_eq!(v["@type"], "setStoryReaction");
        assert_eq!(v["reaction_type"], serde_json::Value::Null);

        // `getStoryAvailableReactions row_size:int32 = AvailableReactions`
        // (schema 1.8.67 line 13802); row_size 10 is inside 5–25.
        let avail = get_story_available_reactions(RequestId(72), 10);
        let v: serde_json::Value = serde_json::from_str(&avail).unwrap();
        assert_eq!(v["@type"], "getStoryAvailableReactions");
        assert_eq!(v["row_size"], 10);

        // `deleteStory story_poster_chat_id:int53 story_id:int32 = Ok`
        // (schema 1.8.67 line 13754).
        let delete = delete_story(RequestId(73), ChatId(11), 7);
        let v: serde_json::Value = serde_json::from_str(&delete).unwrap();
        assert_eq!(v["@type"], "deleteStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 7);

        // Story reply: `sendMessage` with `inputMessageReplyToStory
        // story_poster_chat_id:int53 story_id:int32 = InputMessageReplyTo`
        // (schema 1.8.67 line 3099).
        let reply = send_text_story_reply(RequestId(74), ChatId(11), ChatId(11), 7, "Nice!");
        let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["chat_id"], 11);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToStory");
        assert_eq!(v["reply_to"]["story_poster_chat_id"], 11);
        assert_eq!(v["reply_to"]["story_id"], 7);
        assert_eq!(v["input_message_content"]["text"]["text"], "Nice!");
    }

    #[test]
    fn set_poll_answer_shape_matches_1_8_67() {
        // `setPollAnswer chat_id:int53 message_id:int53
        // option_ids:vector<int32> = Ok` (schema 1.8.67 line 12932).
        // `option_ids` are 0-based option *positions*, not `pollOption.id`
        // strings (those are `updatePollAnswer.option_ids`, line 11186).
        let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[0, 2]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["@extra"], "9");
        assert_eq!(v["chat_id"], 1);
        assert_eq!(v["message_id"], 42);
        assert_eq!(v["option_ids"], serde_json::json!([0, 2]));
    }

    #[test]
    fn set_poll_answer_retract_is_empty_option_ids() {
        // Retracting a vote sends an empty `option_ids` vector (still a
        // `setPollAnswer`, not a different constructor).
        let json = set_poll_answer(RequestId(9), ChatId(1), MessageId(42), &[]);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "setPollAnswer");
        assert_eq!(v["option_ids"], serde_json::json!([]));
    }

    #[test]
    fn send_poll_shape_matches_1_8_67() {
        // `inputMessagePoll` (schema 1.8.67 line 6193) wrapped in
        // `sendMessage`; options are `inputPollOption` (line 462) and the
        // type is `inputPollTypeRegular` (line 481). Quiz types are not
        // sent by this slice.
        let options = ["Sushi place", "Pizza"];
        let json = send_poll(
            RequestId(11),
            ChatId(7),
            PollSend {
                question: "Where should we eat lunch?",
                options: &options,
                is_anonymous: true,
                allows_multiple_answers: false,
                reply_to: Some(MessageId(101)),
                topic_id: None,
            },
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["@type"], "sendMessage");
        assert_eq!(v["@extra"], "11");
        assert_eq!(v["chat_id"], 7);
        assert_eq!(v["reply_to"]["@type"], "inputMessageReplyToMessage");
        assert_eq!(v["reply_to"]["message_id"], 101);
        let content = &v["input_message_content"];
        assert_eq!(content["@type"], "inputMessagePoll");
        assert_eq!(content["question"]["text"], "Where should we eat lunch?");
        assert_eq!(content["question"]["entities"], serde_json::json!([]));
        assert_eq!(content["options"][0]["@type"], "inputPollOption");
        assert_eq!(content["options"][0]["text"]["text"], "Sushi place");
        assert!(content["options"][0]["media"].is_null());
        assert_eq!(content["options"][1]["text"]["text"], "Pizza");
        assert!(content["description"].is_null());
        assert!(content["media"].is_null());
        assert_eq!(content["is_anonymous"], true);
        assert_eq!(content["allows_multiple_answers"], false);
        assert_eq!(content["allows_revoting"], true);
        assert_eq!(content["type"]["@type"], "inputPollTypeRegular");
        assert_eq!(content["type"]["allow_adding_options"], false);
        assert_eq!(content["open_period"], 0);
        assert_eq!(content["close_date"], 0);
        assert_eq!(content["is_closed"], false);
    }

    #[test]
    fn story_requests_match_1_8_67() {
        // `loadActiveStories story_list:StoryList = Ok` (schema line
        // 13762); `getChatActiveStories chat_id:int53 = ChatActiveStories`
        // (line 13768); `getStory story_poster_chat_id:int53 story_id:int32
        // only_local:Bool = Story` (line 13695); `openStory` / `closeStory`
        // `story_poster_chat_id:int53 story_id:int32 = Ok` (lines 13794,
        // 13799).
        let v: serde_json::Value =
            serde_json::from_str(&load_active_stories(RequestId(1))).unwrap();
        assert_eq!(v["@type"], "loadActiveStories");
        assert_eq!(v["story_list"]["@type"], "storyListMain");

        let v: serde_json::Value =
            serde_json::from_str(&get_chat_active_stories(RequestId(2), ChatId(11))).unwrap();
        assert_eq!(v["@type"], "getChatActiveStories");
        assert_eq!(v["chat_id"], 11);

        let v: serde_json::Value =
            serde_json::from_str(&get_story(RequestId(3), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "getStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);
        assert_eq!(v["only_local"], false);

        let v: serde_json::Value =
            serde_json::from_str(&open_story(RequestId(4), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "openStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);

        let v: serde_json::Value =
            serde_json::from_str(&close_story(RequestId(5), ChatId(11), 5)).unwrap();
        assert_eq!(v["@type"], "closeStory");
        assert_eq!(v["story_poster_chat_id"], 11);
        assert_eq!(v["story_id"], 5);
    }
}
