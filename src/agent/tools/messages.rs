//! TDLib messages turned into what agents read: a compact JSON object and a
//! one-line text form.

use super::Ctx;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// How much of a message the caller wants.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    /// One line, text cut to [`SUMMARY_CHARS`].
    Summary,
    /// Everything agents can use.
    Full,
}

const SUMMARY_CHARS: usize = 200;

/// Display names of users and chats, fetched once per tool call.
pub struct Names<'a> {
    ctx: &'a Ctx,
    users: HashMap<i64, String>,
    chats: HashMap<i64, String>,
}

impl<'a> Names<'a> {
    pub fn new(ctx: &'a Ctx) -> Self {
        Self {
            ctx,
            users: HashMap::new(),
            chats: HashMap::new(),
        }
    }

    fn user(&mut self, id: i64) -> String {
        if let Some(name) = self.users.get(&id) {
            return name.clone();
        }
        let name = self
            .ctx
            .call(json!({"@type": "getUser", "user_id": id}))
            .map(|user| user_name(&user))
            .unwrap_or_else(|_| format!("user {id}"));
        self.users.insert(id, name.clone());
        name
    }

    fn chat(&mut self, id: i64) -> String {
        if let Some(name) = self.chats.get(&id) {
            return name.clone();
        }
        let name = self
            .ctx
            .call(json!({"@type": "getChat", "chat_id": id}))
            .ok()
            .and_then(|chat| {
                chat.get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| format!("chat {id}"));
        self.chats.insert(id, name.clone());
        name
    }

    fn sender(&mut self, sender: &Value, me: i64) -> String {
        match sender.get("@type").and_then(Value::as_str) {
            Some("messageSenderUser") => {
                let id = sender.get("user_id").and_then(Value::as_i64).unwrap_or(0);
                if id == me {
                    "You".to_string()
                } else {
                    self.user(id)
                }
            }
            Some("messageSenderChat") => {
                self.chat(sender.get("chat_id").and_then(Value::as_i64).unwrap_or(0))
            }
            _ => "unknown".to_string(),
        }
    }

    fn origin(&mut self, origin: &Value) -> String {
        let str_field = |key: &str| {
            origin
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match origin.get("@type").and_then(Value::as_str) {
            Some("messageOriginUser") => self.user(
                origin
                    .get("sender_user_id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            ),
            Some("messageOriginHiddenUser") => str_field("sender_name"),
            Some("messageOriginChat") => self.chat(
                origin
                    .get("sender_chat_id")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            ),
            Some("messageOriginChannel") => {
                self.chat(origin.get("chat_id").and_then(Value::as_i64).unwrap_or(0))
            }
            _ => "unknown".to_string(),
        }
    }
}

pub fn user_name(user: &Value) -> String {
    let first = user
        .get("first_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let last = user
        .get("last_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let name = format!("{first} {last}").trim().to_string();
    if name.is_empty() {
        "Deleted Account".to_string()
    } else {
        name
    }
}

/// `messageVoiceNote` → `voice_note`.
fn kind_of(content_type: &str) -> String {
    let bare = content_type.strip_prefix("message").unwrap_or(content_type);
    let mut kind = String::new();
    for (i, c) in bare.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                kind.push('_');
            }
            kind.push(c.to_ascii_lowercase());
        } else {
            kind.push(c);
        }
    }
    kind
}

fn formatted(value: Option<&Value>) -> Option<&str> {
    value?.get("text")?.as_str().filter(|text| !text.is_empty())
}

/// The readable text of a message's content: its text or caption, or the
/// one thing that stands for it (a sticker's emoji, a poll's question).
fn content_text(content: &Value) -> String {
    if let Some(text) = formatted(content.get("text")).or_else(|| formatted(content.get("caption")))
    {
        return text.to_string();
    }
    let pick = |path: &[&str]| -> Option<String> {
        let mut node = content;
        for key in path {
            node = node.get(*key)?;
        }
        match node {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Object(_) => formatted(Some(node)).map(str::to_string),
            _ => None,
        }
    };
    pick(&["sticker", "emoji"])
        .or_else(|| pick(&["poll", "question"]))
        .or_else(|| pick(&["contact", "first_name"]))
        .or_else(|| pick(&["venue", "title"]))
        .or_else(|| pick(&["animated_emoji", "emoji"]))
        .or_else(|| pick(&["emoji"]))
        .unwrap_or_default()
}

/// The file a message carries: name, MIME type, size and TDLib file id.
fn file_of(content: &Value) -> Option<Value> {
    let media = [
        "document",
        "audio",
        "video",
        "animation",
        "voice_note",
        "video_note",
        "sticker",
    ]
    .iter()
    .find_map(|key| content.get(*key).map(|media| (*key, media)));
    let (key, media) = match media {
        Some(found) => found,
        None => {
            // A photo's largest size is its last.
            let sizes = content.get("photo")?.get("sizes")?.as_array()?;
            let file = sizes.last()?.get("photo")?;
            return Some(compact(json!({
                "mime_type": "image/jpeg",
                "size": file.get("size"),
                "file_id": file.get("id"),
            })));
        }
    };
    // The file field repeats the media's name, except for voice and video notes.
    let file = media
        .get(key)
        .or_else(|| media.get("voice"))
        .or_else(|| media.get("video"))?;
    Some(compact(json!({
        "name": media.get("file_name").and_then(Value::as_str).filter(|n| !n.is_empty()),
        "mime_type": media.get("mime_type").and_then(Value::as_str).filter(|n| !n.is_empty()),
        "size": file.get("size"),
        "file_id": file.get("id"),
        "duration": media.get("duration"),
    })))
}

/// `value` without its null fields.
fn compact(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.retain(|_, field| !field.is_null());
    }
    value
}

/// Links in a formatted text (URLs and text links).
fn links_of(content: &Value) -> Vec<String> {
    let Some(text) = content.get("text").or_else(|| content.get("caption")) else {
        return Vec::new();
    };
    let body = text.get("text").and_then(Value::as_str).unwrap_or_default();
    let utf16: Vec<u16> = body.encode_utf16().collect();
    let mut links = Vec::new();
    for entity in text
        .get("entities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = entity.get("type");
        match kind.and_then(|k| k.get("@type")).and_then(Value::as_str) {
            Some("textEntityTypeTextUrl") => {
                if let Some(url) = kind.and_then(|k| k.get("url")).and_then(Value::as_str) {
                    links.push(url.to_string());
                }
            }
            Some("textEntityTypeUrl") => {
                let offset = entity.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
                let length = entity.get("length").and_then(Value::as_u64).unwrap_or(0) as usize;
                if let Some(slice) = utf16.get(offset..offset.saturating_add(length)) {
                    links.push(String::from_utf16_lossy(slice));
                }
            }
            _ => {}
        }
    }
    links
}

/// Unix time → `2026-10-24T14:02:05Z`.
pub fn iso_time(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

fn truncate(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", text.get(..cut).unwrap_or(text)),
        None => text.to_string(),
    }
}

/// One TDLib `message` as agents see it.
pub fn shape(message: &Value, me: i64, detail: Detail, names: &mut Names) -> Value {
    let content = message.get("content").cloned().unwrap_or(Value::Null);
    let content_type = content
        .get("@type")
        .and_then(Value::as_str)
        .unwrap_or("messageUnsupported");
    let text = content_text(&content);
    let mut out = Map::new();
    out.insert(
        "id".into(),
        message.get("id").cloned().unwrap_or(Value::Null),
    );
    out.insert(
        "date".into(),
        Value::String(iso_time(
            message.get("date").and_then(Value::as_i64).unwrap_or(0),
        )),
    );
    out.insert(
        "from".into(),
        Value::String(names.sender(message.get("sender_id").unwrap_or(&Value::Null), me)),
    );
    out.insert("kind".into(), Value::String(kind_of(content_type)));
    let text = match detail {
        Detail::Summary => truncate(&text, SUMMARY_CHARS),
        Detail::Full => text,
    };
    out.insert("text".into(), Value::String(text));
    if let Some(origin) = message.get("forward_info").and_then(|f| f.get("origin")) {
        out.insert("forwarded_from".into(), Value::String(names.origin(origin)));
    }
    // Captionless media would read as empty without its file.
    if let Some(file) = file_of(&content) {
        out.insert("file".into(), file);
    }
    if detail == Detail::Full {
        let edit_date = message
            .get("edit_date")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if edit_date > 0 {
            out.insert("edited".into(), Value::String(iso_time(edit_date)));
        }
        if let Some(reply) = message.get("reply_to")
            && reply.get("@type").and_then(Value::as_str) == Some("messageReplyToMessage")
        {
            out.insert(
                "reply_to".into(),
                json!({"chat_id": reply.get("chat_id"), "message_id": reply.get("message_id")}),
            );
        }
        let links = links_of(&content);
        if !links.is_empty() {
            out.insert("links".into(), json!(links));
        }
        if message
            .get("media_album_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id != "0")
        {
            out.insert(
                "album_id".into(),
                message
                    .get("media_album_id")
                    .cloned()
                    .unwrap_or(Value::Null),
            );
        }
    }
    Value::Object(out)
}

/// The text form of one shaped message:
/// `[4194304] 2026-10-24T14:02:05Z You (forwarded from Ann): text`.
pub fn line(shaped: &Value) -> String {
    let s = |key: &str| shaped.get(key).and_then(Value::as_str).unwrap_or_default();
    let mut line = format!(
        "[{}] {} {}",
        shaped.get("id").unwrap_or(&Value::Null),
        s("date"),
        s("from")
    );
    if let Some(origin) = shaped.get("forwarded_from").and_then(Value::as_str) {
        line.push_str(&format!(" (forwarded from {origin})"));
    }
    line.push(':');
    if s("kind") != "text" {
        line.push_str(&format!(" [{}]", s("kind")));
    }
    if let Some(file) = shaped.get("file")
        && let Some(name) = file.get("name").and_then(Value::as_str)
    {
        line.push_str(&format!(" {name}"));
    }
    let text = s("text");
    if !text.is_empty() {
        line.push(' ');
        line.push_str(&text.replace('\n', "\n    "));
    }
    if let Some(reply) = shaped.get("reply_to") {
        line.push_str(&format!(
            " (reply to {})",
            reply.get("message_id").unwrap_or(&Value::Null)
        ));
    }
    if let Some(edited) = shaped.get("edited").and_then(Value::as_str) {
        line.push_str(&format!(" (edited {edited})"));
    }
    line
}

/// The `messages` array of a TDLib answer.
pub fn messages_of(answer: &Value) -> Vec<Value> {
    answer
        .get("messages")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter(|m| !m.is_null()).cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{iso_time, kind_of, line, links_of, truncate};
    use serde_json::json;

    #[test]
    fn times_are_utc_iso() {
        assert_eq!(iso_time(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_time(1_792_902_812), "2026-10-25T04:33:32Z");
        assert_eq!(iso_time(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn kinds_are_snake_case() {
        assert_eq!(kind_of("messageText"), "text");
        assert_eq!(kind_of("messageVoiceNote"), "voice_note");
    }

    #[test]
    fn long_text_is_cut_on_a_char_boundary() {
        assert_eq!(truncate("héllo", 2), "hé…");
        assert_eq!(truncate("hi", 5), "hi");
    }

    #[test]
    fn links_come_from_both_entity_kinds() {
        let content = json!({"@type": "messageText", "text": {"text": "see 🙂 x.org and here", "entities": [
            {"offset": 7, "length": 5, "type": {"@type": "textEntityTypeUrl"}},
            {"offset": 17, "length": 4, "type": {"@type": "textEntityTypeTextUrl", "url": "https://a.b"}}
        ]}});
        assert_eq!(
            links_of(&content),
            vec!["x.org".to_string(), "https://a.b".to_string()]
        );
    }

    #[test]
    fn lines_read_naturally() {
        let shaped = json!({"id": 7, "date": "2026-10-24T05:13:32Z", "from": "You", "kind": "document",
            "text": "two\nlines", "file": {"name": "a.pdf"}, "forwarded_from": "Ann"});
        assert_eq!(
            line(&shaped),
            "[7] 2026-10-24T05:13:32Z You (forwarded from Ann): [document] a.pdf two\n    lines"
        );
    }
}
