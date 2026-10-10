//! Chat history export to file (`parity:platform-chat-export`).
//!
//! Telegram has no `exportHistory` constructor, so the export pages
//! `getChatHistory` itself (100/page, newest first) and writes the
//! projected messages as one JSON or HTML file, optionally limited to a
//! date range and to the viewer's own messages (tdesktop's export box
//! offers the same choices, `export/view/export_view_settings.cpp`). The paging state lives on
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
/// `ParsedMessage`. `sender` is the display name resolved from the session
/// when the page arrives (`None` for channel posts without a sender);
/// `outgoing` marks the viewer's own messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportedMessage {
    pub id: i64,
    pub date: i32,
    pub outgoing: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Html,
    Json,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Json => "json",
        }
    }
}

/// How far back the export reaches. tdesktop has two date pickers; Quill
/// offers the common spans (a calendar picker is a follow-up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportRange {
    #[default]
    AllTime,
    Last7Days,
    Last30Days,
    LastYear,
}

impl ExportRange {
    pub const ALL: [ExportRange; 4] = [
        Self::AllTime,
        Self::Last7Days,
        Self::Last30Days,
        Self::LastYear,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AllTime => "All time",
            Self::Last7Days => "Last 7 days",
            Self::Last30Days => "Last 30 days",
            Self::LastYear => "Last year",
        }
    }

    /// Oldest message date (unix seconds) the range keeps, as of `now`.
    pub fn since(self, now: i64) -> Option<i32> {
        let days: i64 = match self {
            Self::AllTime => return None,
            Self::Last7Days => 7,
            Self::Last30Days => 30,
            Self::LastYear => 365,
        };
        Some((now - days * 86_400).clamp(0, i64::from(i32::MAX)) as i32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatExportOptions {
    pub format: ExportFormat,
    pub range: ExportRange,
    /// "Only my messages".
    pub only_mine: bool,
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
    pub options: ChatExportOptions,
    /// Oldest date kept, resolved from `options.range` when the export
    /// starts.
    pub since: Option<i32>,
}

impl ChatExportState {
    pub fn new(chat_id: ChatId, chat_title: String) -> Self {
        Self::with_options(chat_id, chat_title, ChatExportOptions::default(), 0)
    }

    pub fn with_options(
        chat_id: ChatId,
        chat_title: String,
        options: ChatExportOptions,
        now: i64,
    ) -> Self {
        Self {
            options,
            since: options.range.since(now),
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

    /// The fetched messages that pass the date range and sender choices,
    /// oldest first (paging collects them newest first).
    pub fn selected(&self) -> Vec<&ExportedMessage> {
        let mut kept: Vec<&ExportedMessage> = self
            .messages
            .iter()
            .filter(|m| self.since.is_none_or(|since| m.date >= since))
            .filter(|m| !self.options.only_mine || m.outgoing)
            .collect();
        kept.reverse();
        kept
    }

    /// Paging can stop once a page reaches past the start of the range.
    pub fn past_range_start(&self) -> bool {
        match (self.since, self.messages.last()) {
            (Some(since), Some(oldest)) => oldest.date < since,
            _ => false,
        }
    }
}

/// Project a parsed message into its export form: text or caption plus a
/// short media label (e.g. "photo", "document: report.pdf").
pub fn project_message(message: &ParsedMessage) -> ExportedMessage {
    // Self-destructing media must disappear (Telegram API terms 1.4), so the
    // export keeps only a placeholder row, never its caption.
    let (text, media) = if message.self_destruct.is_some() {
        (None, Some("self-destructing message".into()))
    } else {
        export_text(&message.content)
    };
    ExportedMessage {
        id: message.id.0,
        date: message.date,
        outgoing: message.is_outgoing,
        sender: None,
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

/// Write the finished export into `dir` in the chosen format. Returns the
/// path written, `quill-export-<sanitized-title>-<unix>.<json|html>`.
// ponytail: the whole history is buffered in RAM and written synchronously
// from the poll loop — stream the file and move the write off the loop if
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
    let ext = state.options.format.extension();
    let mut target = dir.join(format!("quill-export-{safe}-{now}.{ext}"));
    // Same-second re-exports must not silently overwrite each other.
    for suffix in 1.. {
        if !target.exists() {
            break;
        }
        target = dir.join(format!("quill-export-{safe}-{now}-{suffix}.{ext}"));
    }
    let selected = state.selected();
    let body = match state.options.format {
        ExportFormat::Json => {
            let payload = serde_json::json!({
                "app": "quill",
                "chat_id": state.chat_id.0,
                "chat_title": state.chat_title,
                "exported_at": now,
                "message_count": selected.len(),
                "messages": selected,
            });
            serde_json::to_string_pretty(&payload)?
        }
        ExportFormat::Html => render_html(
            &state.chat_title,
            &selected,
            crate::local_time::utc_offset_at,
        ),
    };
    std::fs::create_dir_all(dir)?;
    // Write beside the target and rename at the end so a failed export never
    // leaves a half-written file under the final name.
    let tmp = dir.join(format!(".{now}-{}.export.tmp", std::process::id()));
    let written = std::fs::write(&tmp, body).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(err) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(target)
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// A self-contained HTML page: messages oldest first, a date heading per
/// day, the sender above each run of messages from the same sender. Media
/// appears as a label only; files are not copied.
pub fn render_html(
    title: &str,
    messages: &[&ExportedMessage],
    offset_at: impl Fn(i64) -> i32,
) -> String {
    use crate::local_time::{civil_at, hhmm, month_name};
    let title = escape_html(title);
    let mut out = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>\n{HTML_STYLE}</style>\n</head>\n<body>\n\
         <main>\n<h1>{title}</h1>\n<p class=\"count\">{} messages</p>\n",
        messages.len()
    );
    let mut last_day = None;
    let mut last_sender: Option<Option<String>> = None;
    for m in messages {
        let unix = i64::from(m.date);
        let time = civil_at(unix, offset_at(unix));
        let day = time.day_number();
        if last_day != Some(day) {
            out.push_str(&format!(
                "<h2>{} {} {}</h2>\n",
                time.day,
                month_name(time.month),
                time.year
            ));
            last_day = Some(day);
            last_sender = None;
        }
        let name = if m.outgoing && m.sender.is_none() {
            Some("You".to_string())
        } else {
            m.sender.clone()
        };
        let class = if m.outgoing { "message out" } else { "message" };
        out.push_str(&format!("<div class=\"{class}\">\n"));
        if last_sender.as_ref() != Some(&name)
            && let Some(name) = &name
        {
            out.push_str(&format!(
                "<div class=\"from\">{}</div>\n",
                escape_html(name)
            ));
        }
        last_sender = Some(name);
        if let Some(media) = &m.media {
            out.push_str(&format!(
                "<div class=\"media\">[{}]</div>\n",
                escape_html(media)
            ));
        }
        if let Some(text) = &m.text {
            out.push_str(&format!(
                "<div class=\"text\">{}</div>\n",
                escape_html(text).replace('\n', "<br>\n")
            ));
        }
        out.push_str(&format!(
            "<div class=\"time\">{}</div>\n</div>\n",
            hhmm(&time)
        ));
    }
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

const HTML_STYLE: &str = "\
body{margin:0;background:#f4f5f7;color:#1c1e21;font:15px/1.45 system-ui,sans-serif}\n\
main{max-width:720px;margin:0 auto;padding:24px 16px}\n\
h1{font-size:20px;margin:0}\n\
h2{font-size:13px;font-weight:600;text-align:center;color:#65676b;margin:24px 0 8px}\n\
.count{color:#65676b;margin:4px 0 0}\n\
.message{background:#fff;border-radius:10px;padding:8px 12px;margin:4px 0;max-width:85%}\n\
.message.out{background:#e3f0ff;margin-left:auto}\n\
.from{font-weight:600;font-size:13px;color:#2f6fed}\n\
.media{color:#65676b;font-style:italic}\n\
.text{overflow-wrap:anywhere}\n\
.time{font-size:11px;color:#65676b;text-align:right}\n\
@media (prefers-color-scheme:dark){body{background:#17212b;color:#e8eaed}\n\
.message{background:#182533}.message.out{background:#2b5278}\n\
h2,.count,.media,.time{color:#8fa0b3}.from{color:#6ab3f3}}\n";

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
            thread_id: None,
            media_album_id: 0,
            author_signature: None,
            scheduling_state: None,
            can_retry: false,
            send_state: Default::default(),
            content: MessageContent::Text(TextContent {
                text: text.to_string(),
                entities: Vec::new(),
                link_preview: None,
            }),
            ephemeral: None,
            files: Vec::new(),
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
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
                sender: None,
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
    fn projection_drops_self_destructing_content() {
        let mut m = text_message(5, "secret caption");
        m.self_destruct = Some(crate::telegram::envelope::MessageSelfDestruct {
            kind: crate::telegram::envelope::SelfDestructKind::Immediately,
            expires_in_ms: 0,
            fetched_at_ms: 0,
        });
        let e = project_message(&m);
        assert_eq!(e.text, None);
        assert_eq!(e.media, Some("self-destructing message".into()));
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
        state.options.format = ExportFormat::Json;
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

    fn exported(id: i64, date: i32, outgoing: bool, sender: &str, text: &str) -> ExportedMessage {
        ExportedMessage {
            id,
            date,
            outgoing,
            sender: Some(sender.to_string()),
            text: Some(text.to_string()),
            media: None,
        }
    }

    #[test]
    fn range_spans_resolve_against_now() {
        let now = 1_700_000_000;
        assert_eq!(ExportRange::AllTime.since(now), None);
        assert_eq!(
            ExportRange::Last7Days.since(now),
            Some(1_700_000_000 - 7 * 86_400)
        );
        assert_eq!(
            ExportRange::LastYear.since(now),
            Some(1_700_000_000 - 365 * 86_400)
        );
        // A clock before the epoch never produces a negative cutoff.
        assert_eq!(ExportRange::Last30Days.since(1), Some(0));
    }

    #[test]
    fn selection_applies_range_and_sender_choice_oldest_first() {
        let mut state = ChatExportState::with_options(
            ChatId(1),
            "g".into(),
            ChatExportOptions {
                format: ExportFormat::Json,
                range: ExportRange::Last7Days,
                only_mine: true,
            },
            1_700_000_000,
        );
        let week = 7 * 86_400;
        // Newest first, as paging delivers them.
        state.messages = vec![
            exported(4, 1_700_000_000 - 10, true, "Me", "d"),
            exported(3, 1_700_000_000 - 20, false, "Ann", "c"),
            exported(2, 1_700_000_000 - week + 5, true, "Me", "b"),
            exported(1, 1_700_000_000 - week - 5, true, "Me", "a"),
        ];
        let ids: Vec<i64> = state.selected().iter().map(|m| m.id).collect();
        assert_eq!(ids, [2, 4]);
        assert!(state.past_range_start());
        state.messages.pop();
        assert!(!state.past_range_start());
    }

    #[test]
    fn html_escapes_text_groups_by_day_and_sender() {
        let a = exported(1, 1_700_000_000, false, "Ann <b>", "hi & <script>\nline 2");
        let b = exported(2, 1_700_000_060, false, "Ann <b>", "again");
        let c = exported(3, 1_700_000_000 + 2 * 86_400, true, "Me", "later");
        let mut with_media = c.clone();
        with_media.id = 4;
        with_media.media = Some("photo".into());
        let html = render_html("Team & <Co>", &[&a, &b, &c, &with_media], |_| 0);
        assert!(html.contains("<title>Team &amp; &lt;Co&gt;</title>"));
        assert!(html.contains("<p class=\"count\">4 messages</p>"));
        assert!(html.contains("hi &amp; &lt;script&gt;<br>\nline 2"));
        assert!(!html.contains("<script>"));
        // The sender shows once for the two consecutive messages.
        assert_eq!(
            html.matches("<div class=\"from\">Ann &lt;b&gt;</div>")
                .count(),
            1
        );
        // Two different days.
        assert_eq!(html.matches("<h2>").count(), 2);
        assert!(html.contains("<h2>14 November 2023</h2>"));
        assert!(html.contains("<div class=\"media\">[photo]</div>"));
        assert!(html.contains("class=\"message out\""));
    }

    #[test]
    fn write_export_html_uses_the_html_extension() {
        let dir = std::env::temp_dir().join(format!(
            "quill-export-html-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut state = ChatExportState::new(ChatId(7), "Team".into());
        state.options.format = ExportFormat::Html;
        state.messages.push(project_message(&text_message(3, "hi")));
        let path = write_export(&state, &dir).unwrap();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("html"));
        let page = std::fs::read_to_string(&path).unwrap();
        assert!(page.starts_with("<!DOCTYPE html>"));
        assert!(page.contains(">hi<"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
