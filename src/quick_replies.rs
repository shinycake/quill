//! Quick replies (Telegram Business shortcuts): rules, parsing and request
//! bodies. The settings list, the shortcut editor and the composer `/` rows
//! live in `ui/quick_replies*.rs`.
//!
//! What Telegram Desktop does:
//!
//! - `settings/business/settings_quick_replies.cpp`: Settings -> Quick
//!   Replies lists every shortcut that has messages as `/name` with its
//!   message count. "Add Quick Reply" asks for a name (1-32 letters, digits
//!   or `_`), then opens the shortcut's messages. Without Premium, adding
//!   shows the Premium preview.
//! - `settings/business/settings_shortcut_messages.cpp`: the messages of one
//!   shortcut are composed, edited and deleted like chat messages. The menu
//!   renames the shortcut (a taken name says so) and deletes it, asking
//!   first when it has messages. A shortcut with no messages is removed.
//! - `chat_helpers/field_autocomplete.cpp`: typing `/` in a private chat
//!   with a user (not a bot) lists the shortcuts that have messages, filtered
//!   by what follows the slash; choosing one sends all its messages
//!   (`sendQuickReplyShortcutMessages`).
//!
//! The editor here handles plain text messages. Media messages of a shortcut
//! are listed (with their caption) and can be deleted, but not created or
//! edited.

use serde_json::{Value, json};

/// tdesktop `kShortcutLimit`: the longest shortcut name.
pub const NAME_MAX: usize = 32;
/// tdesktop `quick_replies_limit` default.
pub const SHORTCUTS_MAX: usize = 100;
/// tdesktop `quick_reply_messages_limit` default.
pub const MESSAGES_MAX: usize = 20;

/// Why a shortcut name is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    TooLong,
    BadSymbols,
}

impl NameProblem {
    pub fn line(self) -> &'static str {
        match self {
            Self::Empty => "Enter a shortcut.",
            Self::TooLong => "The shortcut can have at most 32 characters.",
            Self::BadSymbols => "Use letters, digits and underscores only.",
        }
    }
}

/// tdesktop `ValidShortcutName`: letters, digits, `_`, ZWNJ, the middle dot
/// and the Sinhala block.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric()
        || c == '_'
        || c == '\u{200c}'
        || c == '\u{00b7}'
        || ('\u{0d80}'..='\u{0dff}').contains(&c)
}

/// The trimmed name when it is acceptable.
pub fn validate_name(raw: &str) -> Result<String, NameProblem> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(NameProblem::Empty);
    }
    if name.chars().count() > NAME_MAX {
        return Err(NameProblem::TooLong);
    }
    if !name.chars().all(is_name_char) {
        return Err(NameProblem::BadSymbols);
    }
    Ok(name.to_string())
}

/// One shortcut (`quickReplyShortcut`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickReplyShortcut {
    pub id: i32,
    pub name: String,
    pub message_count: i32,
}

/// One message of a shortcut (`quickReplyMessage`), reduced to what the
/// editor shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickReplyMessage {
    pub id: i64,
    /// The message text, or the caption of a media message.
    pub text: String,
    /// What a media message is ("Photo"); empty for text.
    pub media: String,
    pub can_be_edited: bool,
}

impl QuickReplyMessage {
    /// Only plain text messages are edited in Quill.
    pub fn is_editable_text(&self) -> bool {
        self.can_be_edited && self.media.is_empty()
    }

    /// The row's text: the text, or the media kind and its caption.
    pub fn summary(&self) -> String {
        match (self.media.is_empty(), self.text.is_empty()) {
            (true, _) => self.text.clone(),
            (false, true) => self.media.clone(),
            (false, false) => format!("{}: {}", self.media, self.text),
        }
    }
}

fn media_label(kind: &str) -> &'static str {
    match kind {
        "messagePhoto" => "Photo",
        "messageVideo" => "Video",
        "messageAnimation" => "GIF",
        "messageAudio" => "Audio",
        "messageDocument" => "File",
        "messageVoiceNote" => "Voice message",
        "messageVideoNote" => "Video message",
        "messageSticker" => "Sticker",
        "messageContact" => "Contact",
        "messageLocation" | "messageVenue" => "Location",
        "messagePoll" => "Poll",
        "messageChecklist" => "Checklist",
        _ => "Message",
    }
}

/// `quickReplyMessage` -> row. `None` for a malformed object.
pub fn parse_message(value: &Value) -> Option<QuickReplyMessage> {
    let id = value.get("id")?.as_i64()?;
    let content = value.get("content")?;
    let kind = content.get("@type")?.as_str()?;
    let (text, media) = if kind == "messageText" {
        (formatted(content.get("text")), String::new())
    } else {
        (
            formatted(content.get("caption")),
            media_label(kind).to_string(),
        )
    };
    Some(QuickReplyMessage {
        id,
        text,
        media,
        can_be_edited: value
            .get("can_be_edited")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn formatted(value: Option<&Value>) -> String {
    value
        .and_then(|v| v.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `quickReplyShortcut` -> shortcut. `None` for a malformed object.
pub fn parse_shortcut(value: &Value) -> Option<QuickReplyShortcut> {
    Some(QuickReplyShortcut {
        id: i32::try_from(value.get("id")?.as_i64()?).ok()?,
        name: value.get("name")?.as_str()?.to_string(),
        message_count: value
            .get("message_count")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0),
    })
}

/// A list of `quickReplyMessage`; null or malformed entries are dropped.
pub fn parse_messages(value: Option<&Value>) -> Vec<QuickReplyMessage> {
    value
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(parse_message).collect())
        .unwrap_or_default()
}

/// A TDLib update about quick replies, carried as
/// `SettingsPayload::QuickReplies`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickRepliesUpdate {
    /// `updateQuickReplyShortcut`
    Shortcut(QuickReplyShortcut),
    /// `updateQuickReplyShortcutDeleted`
    Deleted { shortcut_id: i32 },
    /// `updateQuickReplyShortcuts`: the new order.
    Order { shortcut_ids: Vec<i32> },
    /// `updateQuickReplyShortcutMessages`
    Messages {
        shortcut_id: i32,
        messages: Vec<QuickReplyMessage>,
    },
}

/// The update for a TDLib object type, `None` when it is not one of ours or
/// is malformed.
pub fn parse_update(type_name: &str, value: &Value) -> Option<QuickRepliesUpdate> {
    let shortcut_id = || {
        value
            .get("shortcut_id")
            .and_then(Value::as_i64)
            .and_then(|id| i32::try_from(id).ok())
    };
    match type_name {
        "updateQuickReplyShortcut" => {
            parse_shortcut(value.get("shortcut")?).map(QuickRepliesUpdate::Shortcut)
        }
        "updateQuickReplyShortcutDeleted" => Some(QuickRepliesUpdate::Deleted {
            shortcut_id: shortcut_id()?,
        }),
        "updateQuickReplyShortcuts" => Some(QuickRepliesUpdate::Order {
            shortcut_ids: value
                .get("shortcut_ids")?
                .as_array()?
                .iter()
                .filter_map(|id| i32::try_from(id.as_i64()?).ok())
                .collect(),
        }),
        "updateQuickReplyShortcutMessages" => Some(QuickRepliesUpdate::Messages {
            shortcut_id: shortcut_id()?,
            messages: parse_messages(value.get("messages")),
        }),
        _ => None,
    }
}

/// "1 message" / "3 messages" (tdesktop `lng_forum_messages`).
pub fn count_label(count: i32) -> String {
    if count == 1 {
        "1 message".to_string()
    } else {
        format!("{count} messages")
    }
}

fn text_content(text: &str) -> Value {
    json!({
        "@type": "inputMessageText",
        "text": {"@type": "formattedText", "text": text, "entities": []},
    })
}

pub fn load_shortcuts_request(extra: &str) -> String {
    json!({"@type": "loadQuickReplyShortcuts", "@extra": extra}).to_string()
}

pub fn load_messages_request(extra: &str, shortcut_id: i32) -> String {
    json!({"@type": "loadQuickReplyShortcutMessages", "@extra": extra, "shortcut_id": shortcut_id})
        .to_string()
}

/// Adds a text message; TDLib creates the shortcut when `name` is new.
pub fn add_message_request(extra: &str, name: &str, text: &str) -> String {
    json!({
        "@type": "addQuickReplyShortcutMessage",
        "@extra": extra,
        "shortcut_name": name,
        "reply_to_message_id": 0,
        "input_message_content": text_content(text),
    })
    .to_string()
}

pub fn edit_message_request(extra: &str, shortcut_id: i32, message_id: i64, text: &str) -> String {
    json!({
        "@type": "editQuickReplyMessage",
        "@extra": extra,
        "shortcut_id": shortcut_id,
        "message_id": message_id,
        "input_message_content": text_content(text),
    })
    .to_string()
}

pub fn delete_messages_request(extra: &str, shortcut_id: i32, message_ids: &[i64]) -> String {
    json!({
        "@type": "deleteQuickReplyShortcutMessages",
        "@extra": extra,
        "shortcut_id": shortcut_id,
        "message_ids": message_ids,
    })
    .to_string()
}

pub fn rename_request(extra: &str, shortcut_id: i32, name: &str) -> String {
    json!({
        "@type": "setQuickReplyShortcutName",
        "@extra": extra,
        "shortcut_id": shortcut_id,
        "name": name,
    })
    .to_string()
}

pub fn delete_shortcut_request(extra: &str, shortcut_id: i32) -> String {
    json!({"@type": "deleteQuickReplyShortcut", "@extra": extra, "shortcut_id": shortcut_id})
        .to_string()
}

pub fn send_request(extra: &str, chat_id: i64, shortcut_id: i32, sending_id: i32) -> String {
    json!({
        "@type": "sendQuickReplyShortcutMessages",
        "@extra": extra,
        "chat_id": chat_id,
        "shortcut_id": shortcut_id,
        "sending_id": sending_id,
    })
    .to_string()
}

/// tdesktop `lng_replies_error_occupied`; shown when a name is already used
/// (checked here, since TDLib errors carry no readable text in Quill).
pub const NAME_TAKEN_LINE: &str = "This shortcut is already used.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_tdesktop() {
        assert_eq!(validate_name("  hello_1 "), Ok("hello_1".into()));
        assert_eq!(validate_name("שלום"), Ok("שלום".into()));
        assert_eq!(validate_name("   "), Err(NameProblem::Empty));
        assert_eq!(validate_name("a b"), Err(NameProblem::BadSymbols));
        assert_eq!(validate_name("a-b"), Err(NameProblem::BadSymbols));
        assert_eq!(validate_name("/hi"), Err(NameProblem::BadSymbols));
        assert_eq!(validate_name(&"x".repeat(32)), Ok("x".repeat(32)));
        assert_eq!(validate_name(&"x".repeat(33)), Err(NameProblem::TooLong));
        assert_eq!(
            validate_name("a\u{200c}b\u{b7}c"),
            Ok("a\u{200c}b\u{b7}c".into())
        );
    }

    #[test]
    fn a_text_message_and_a_captioned_photo_parse() {
        let text = parse_message(&json!({
            "id": 5, "can_be_edited": true,
            "content": {"@type": "messageText", "text": {"text": "Hello", "entities": []}},
        }))
        .unwrap();
        assert_eq!(text.summary(), "Hello");
        assert!(text.is_editable_text());
        let photo = parse_message(&json!({
            "id": 6, "can_be_edited": true,
            "content": {"@type": "messagePhoto", "caption": {"text": "Menu"}},
        }))
        .unwrap();
        assert_eq!(photo.summary(), "Photo: Menu");
        assert!(!photo.is_editable_text());
        assert!(parse_message(&json!({"id": 7})).is_none());
    }

    #[test]
    fn a_shortcut_parses_and_counts_read_well() {
        let shortcut = parse_shortcut(&json!({"id": 3, "name": "hours", "message_count": 2}));
        assert_eq!(
            shortcut,
            Some(QuickReplyShortcut {
                id: 3,
                name: "hours".into(),
                message_count: 2
            })
        );
        assert!(parse_shortcut(&json!({"name": "x"})).is_none());
        assert_eq!(count_label(1), "1 message");
        assert_eq!(count_label(4), "4 messages");
    }

    #[test]
    fn request_bodies_match_the_schema() {
        let add: Value = serde_json::from_str(&add_message_request("1", "hi", "Hello")).unwrap();
        assert_eq!(add["@type"], "addQuickReplyShortcutMessage");
        assert_eq!(add["shortcut_name"], "hi");
        assert_eq!(add["reply_to_message_id"], 0);
        assert_eq!(add["input_message_content"]["@type"], "inputMessageText");
        assert_eq!(add["input_message_content"]["text"]["text"], "Hello");
        let edit: Value = serde_json::from_str(&edit_message_request("2", 3, 9, "New")).unwrap();
        assert_eq!(edit["@type"], "editQuickReplyMessage");
        assert_eq!(
            (edit["shortcut_id"].as_i64(), edit["message_id"].as_i64()),
            (Some(3), Some(9))
        );
        let del: Value = serde_json::from_str(&delete_messages_request("3", 3, &[9, 10])).unwrap();
        assert_eq!(del["message_ids"], json!([9, 10]));
        let send: Value = serde_json::from_str(&send_request("4", 77, 3, 12)).unwrap();
        assert_eq!(send["@type"], "sendQuickReplyShortcutMessages");
        assert_eq!(send["chat_id"], 77);
        assert_eq!(send["sending_id"], 12);
        let rename: Value = serde_json::from_str(&rename_request("5", 3, "new")).unwrap();
        assert_eq!(rename["@type"], "setQuickReplyShortcutName");
        let load: Value = serde_json::from_str(&load_shortcuts_request("6")).unwrap();
        assert_eq!(load["@type"], "loadQuickReplyShortcuts");
    }
}
