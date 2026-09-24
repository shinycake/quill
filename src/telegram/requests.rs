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
}
