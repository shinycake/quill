//! Chat history export to file (`parity:platform-chat-export`).
//!
//! Telegram has no `exportHistory` constructor, so the export pages
//! `getChatHistory` itself (100/page, newest first) and writes the
//! projected messages as one JSON file. The paging state lives on
//! [`ChatExportState`] (held by `Session`); the driver pumps pages in
//! `pump_chat_export` and the UI starts it from the chat header.
use crate::ids::{ChatId, MessageId};
use crate::telegram::envelope::{MessageContent, ParsedMessage};
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Page size for export `getChatHistory` paging (TDLib's max is 100).
pub const EXPORT_PAGE_LIMIT: i32 = 100;

/// One exported message: the JSON-serializable projection of a
/// `ParsedMessage`. Sender names are deliberately absent — Quill does not
/// plumb per-message sender identity anywhere (the UI shows the chat title
/// on rows); `outgoing` marks the viewer's own messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedMessage {
    pub id: i64,
    pub date: i32,
    pub outgoing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

/// In-progress export, held on `Session::chat_export`.
pub struct ChatExportState {
    pub chat_id: ChatId,
    pub chat_title: String,
    pub messages: Vec<ExportedMessage>,
    /// A `getChatHistory` page is in flight.
    pub in_flight: bool,
    /// A page added no older message — no more history to fetch.
    pub done_paging: bool,
    /// Set when a page send or the file write failed; the UI surfaces and
    /// clears the state.
    pub failed: Option<String>,
    /// Set when the file is written; the UI surfaces the path and clears
    /// the state.
    pub finished_path: Option<PathBuf>,
}

impl ChatExportState {
    pub fn new(chat_id: ChatId, chat_title: String) -> Self {
        Self {
            chat_id,
            chat_title,
            messages: Vec::new(),
            in_flight: false,
            done_paging: false,
            failed: None,
            finished_path: None,
        }
    }

    /// `from_message_id` for the next page: the oldest message fetched so
    /// far, or 0 for the first page (mirrors `fetch_history`).
    pub fn page_from(&self) -> MessageId {
        self.messages
            .last()
            .map(|m| MessageId(m.id))
            .unwrap_or(MessageId(0))
    }

    pub fn settled(&self) -> bool {
        self.failed.is_some() || self.finished_path.is_some()
    }
}

/// Project a parsed message into its export form: text or caption plus a
/// short media label (e.g. "photo", "document: report.pdf").
pub fn project_message(message: &ParsedMessage) -> ExportedMessage {
    let (text, media) = export_text(&message.content);
    ExportedMessage {
        id: message.id.0,
        date: message.date,
        outgoing: message.is_outgoing,
        text,
        media,
    }
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

/// (text-or-caption, media-label) for a message's content.
fn export_text(content: &MessageContent) -> (Option<String>, Option<String>) {
    match content {
        MessageContent::Text(c) => (non_empty(&c.text), None),
        MessageContent::Photo(c) => (non_empty(&c.caption), Some("photo".into())),
        MessageContent::Document(c) => (
            non_empty(&c.caption),
            Some(format!("document: {}", c.file_name)),
        ),
        MessageContent::Video(c) => (non_empty(&c.caption), Some("video".into())),
        MessageContent::Animation(c) => (non_empty(&c.caption), Some("animation".into())),
        MessageContent::VoiceNote(c) => (non_empty(&c.caption), Some("voice message".into())),
        MessageContent::VideoNote(_) => (None, Some("video message".into())),
        MessageContent::Audio(c) => (non_empty(&c.caption), Some("audio".into())),
        MessageContent::Sticker(_) => (None, Some("sticker".into())),
        MessageContent::Poll(_) => (None, Some("poll".into())),
        MessageContent::Location(_) => (None, Some("location".into())),
        MessageContent::Venue(_) => (None, Some("venue".into())),
        MessageContent::Contact(_) => (None, Some("contact".into())),
        MessageContent::Dice(_) => (None, Some("dice".into())),
        _ => (None, Some("message".into())),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Write the finished export as pretty JSON into `dir`. Returns the path
/// written. The file name is `quill-export-<sanitized-title>-<unix>.json`.
// ponytail: the whole history is buffered in RAM and written synchronously
// from the poll loop — stream the JSON and move the write off the loop if
// giant-chat exports ever freeze the app.
pub fn write_export(state: &ChatExportState, dir: &Path) -> io::Result<PathBuf> {
    let safe: String = state
        .chat_title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    let safe = if safe.is_empty() {
        "chat".to_string()
    } else {
        safe
    };
    let now = unix_now();
    let mut target = dir.join(format!("quill-export-{safe}-{now}.json"));
    // Same-second re-exports must not silently overwrite each other.
    for suffix in 1.. {
        if !target.exists() {
            break;
        }
        target = dir.join(format!("quill-export-{safe}-{now}-{suffix}.json"));
    }
    let payload = serde_json::json!({
        "app": "quill",
        "chat_id": state.chat_id.0,
        "chat_title": state.chat_title,
        "exported_at": now,
        "message_count": state.messages.len(),
        "messages": state.messages,
    });
    std::fs::create_dir_all(dir)?;
    std::fs::write(&target, serde_json::to_string_pretty(&payload)?)?;
    Ok(target)
}

/// Where exports land by default: the user's Downloads folder, falling back
/// to the temp dir when HOME is unset.
pub fn default_export_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|home| Path::new(&home).join("Downloads"))
        .unwrap_or_else(|_| std::env::temp_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MessageId;
    use crate::telegram::envelope::TextContent;

    fn text_message(id: i64, text: &str) -> ParsedMessage {
        // Minimal ParsedMessage: only the fields the projection reads.
        ParsedMessage {
            sender: None,
            id: MessageId(id),
            chat_id: ChatId(7),
            date: 1_700_000_000,
            is_outgoing: false,
            is_pinned: false,
            topic_id: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            content: MessageContent::Text(TextContent {
                text: text.to_string(),
                entities: Vec::new(),
                link_preview: None,
            }),
            ephemeral: None,
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            interaction_info: None,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
        }
    }

    #[test]
    fn projection_keeps_text_and_marks_outgoing() {
        let mut m = text_message(42, "hello");
        m.is_outgoing = true;
        let e = project_message(&m);
        assert_eq!(
            e,
            ExportedMessage {
                id: 42,
                date: 1_700_000_000,
                outgoing: true,
                text: Some("hello".into()),
                media: None,
            }
        );
    }

    #[test]
    fn projection_labels_sticker_without_text() {
        let mut m = text_message(1, "");
        m.content = MessageContent::ChatTtlChanged { secs: 60 };
        let e = project_message(&m);
        assert_eq!(e.text, None);
        // Non-text, non-media content degrades to the generic label.
        assert_eq!(e.media, Some("message".into()));
    }

    #[test]
    fn page_from_uses_oldest_fetched() {
        let mut state = ChatExportState::new(ChatId(7), "g".into());
        assert_eq!(state.page_from(), MessageId(0));
        state.messages.push(project_message(&text_message(90, "a")));
        state.messages.push(project_message(&text_message(80, "b")));
        assert_eq!(state.page_from(), MessageId(80));
    }

    #[test]
    fn write_export_produces_expected_json() {
        let dir = std::env::temp_dir().join(format!(
            "quill-export-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut state = ChatExportState::new(ChatId(7), "Test Chat!".into());
        state.messages.push(project_message(&text_message(3, "hi")));
        let path = write_export(&state, &dir).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("quill-export-Test_Chat_-"));
        let parsed: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed["app"], "quill");
        assert_eq!(parsed["chat_id"], 7);
        assert_eq!(parsed["message_count"], 1);
        assert_eq!(parsed["messages"][0]["text"], "hi");
        assert_eq!(parsed["messages"][0]["outgoing"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
