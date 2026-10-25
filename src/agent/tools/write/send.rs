//! Tools that put new messages in a chat: text, files, polls, stickers and
//! forwards.

use super::super::messages::{self, Detail, Names};
use super::super::target::{Target, chat_schema, require};
use super::super::{Ctx, ToolError, ToolOutput};
use super::common::{
    self, CAPTION_LIMIT, TEXT_LIMIT, confirm, formatted, message_ids, reply_to, required_text,
    send_options,
};
use crate::agent::grants::Scope;
use serde_json::{Value, json};
use std::path::Path;

/// The sent messages as the agent sees them.
fn sent_output(
    ctx: &Ctx,
    target: &Target,
    verb: &str,
    sent: &[Value],
    scheduled: bool,
) -> ToolOutput {
    let mut names = Names::new(ctx);
    let shaped: Vec<Value> = sent
        .iter()
        .map(|m| messages::shape(m, target.me, Detail::Full, &mut names))
        .collect();
    let when = if scheduled { " (scheduled)" } else { "" };
    let mut text = format!("{verb} in \"{}\"{when}:\n", target.title);
    for message in &shaped {
        text.push_str(&messages::line(message));
        text.push('\n');
    }
    ToolOutput {
        text,
        data: json!({"chat": target.summary(), "messages": shaped, "scheduled": scheduled}),
    }
}

/// Whether `file` lives in Quill's data folder, resolving links first.
fn in_quill_data(file: &Path) -> bool {
    let Some(root) = crate::settings::safe_app_root() else {
        return false;
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    file.starts_with(root)
}

fn clip(text: &str) -> String {
    let one_line = text.replace('\n', " ");
    if one_line.chars().count() > 60 {
        format!("{}…", one_line.chars().take(60).collect::<String>())
    } else {
        one_line
    }
}

fn is_scheduled(args: &Value) -> bool {
    args.get("schedule_at").is_some_and(|v| !v.is_null())
}

pub fn send_message_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "text": {"type": "string", "description": "The message, up to 4096 characters."},
            "format": common::format_schema(),
            "reply_to": common::reply_schema(),
            "link_preview": {"type": "boolean", "default": true, "description": "Show a preview of the first link."},
            "silent": common::silent_schema(),
            "schedule_at": common::schedule_schema(),
        },
        "required": ["chat", "text"],
        "additionalProperties": false,
    })
}

pub fn send_message(args: &Value) -> Result<ToolOutput, ToolError> {
    let text = required_text(args, "text")?;
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let body = formatted(&ctx, &text, args, TEXT_LIMIT)?;
    let link_preview = args
        .get("link_preview")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let request = json!({
        "@type": "sendMessage",
        "chat_id": target.chat_id,
        "topic_id": Value::Null,
        "reply_to": reply_to(args)?,
        "options": send_options(args)?,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageText",
            "text": body,
            "link_preview_options": if link_preview { Value::Null } else {
                json!({"@type": "linkPreviewOptions", "is_disabled": true, "url": "", "force_small_media": false, "force_large_media": false, "show_above_text": false})
            },
            "clear_draft": false,
        },
    });
    confirm(&ctx, &target, "send_message", &text)?;
    let summary = format!("\"{}\"", clip(&text));
    let sent = common::send(&ctx, &target, "send_message", &summary, request, 1)?;
    Ok(sent_output(
        &ctx,
        &target,
        "Sent",
        &sent,
        is_scheduled(args),
    ))
}

/// How to send one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Photo,
    Video,
    Audio,
    Voice,
    Document,
}

impl FileKind {
    fn parse(word: &str) -> Option<Option<Self>> {
        Some(match word {
            "auto" => None,
            "photo" => Some(Self::Photo),
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            "voice" => Some(Self::Voice),
            "document" | "file" => Some(Self::Document),
            _ => return None,
        })
    }

    /// The kind a file's extension suggests.
    fn guess(path: &Path) -> Self {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "heic" => Self::Photo,
            "mp4" | "mov" | "m4v" | "webm" => Self::Video,
            "mp3" | "m4a" | "flac" | "wav" | "aac" => Self::Audio,
            "ogg" | "oga" | "opus" => Self::Voice,
            _ => Self::Document,
        }
    }
}

/// `inputMessage*` for one local file, with `caption`.
fn file_content(path: &str, kind: FileKind, caption: &Value) -> Value {
    use crate::telegram::requests as td;
    let mut content = match kind {
        FileKind::Photo => td::input_message_photo(path, "", false, None, false),
        FileKind::Video => td::input_message_video(
            path,
            &td::VideoSend {
                duration: 0,
                width: 0,
                height: 0,
                supports_streaming: true,
                self_destruct: None,
            },
            "",
            false,
            false,
        ),
        FileKind::Audio => td::input_message_audio(path, "", false),
        FileKind::Voice => json!({
            "@type": "inputMessageVoiceNote",
            "voice_note": {
                "@type": "inputVoiceNote",
                "voice_note": {"@type": "inputFileLocal", "path": path},
                "duration": 0,
                "waveform": "",
            },
            "caption": Value::Null,
            "self_destruct_type": Value::Null,
        }),
        FileKind::Document => td::input_message_document(path, None, "", false),
    };
    if let Some(fields) = content.as_object_mut() {
        fields.insert("caption".into(), caption.clone());
    }
    content
}

pub fn send_file_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "paths": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 10,
                "description": "Absolute paths of files on this computer. 2 to 10 photos and videos (or 2 to 10 documents, or audio files) go as one album."},
            "caption": {"type": "string", "description": "Text under the file (the album's first item), up to 1024 characters."},
            "format": common::format_schema(),
            "as": {"type": "string", "enum": ["auto", "photo", "video", "audio", "voice", "document"], "default": "auto",
                "description": "How to send: auto picks by extension (jpg/png → photo, mp4/mov → video, mp3/m4a → audio, ogg/opus → voice, anything else → document)."},
            "reply_to": common::reply_schema(),
            "silent": common::silent_schema(),
            "schedule_at": common::schedule_schema(),
        },
        "required": ["chat", "paths"],
        "additionalProperties": false,
    })
}

pub fn send_file(args: &Value) -> Result<ToolOutput, ToolError> {
    let paths: Vec<String> = match args.get("paths") {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(Value::String(one)) => vec![one.clone()],
        _ => Vec::new(),
    };
    if paths.is_empty() || paths.len() > 10 {
        return Err(ToolError::new(
            "paths must list 1 to 10 absolute file paths.",
        ));
    }
    for path in &paths {
        let file = Path::new(path);
        if !file.is_absolute() {
            return Err(ToolError::new(format!("{path} is not an absolute path.")));
        }
        if !file.is_file() {
            return Err(ToolError::new(format!("There is no file at {path}.")));
        }
        if in_quill_data(file) {
            return Err(ToolError::new(format!(
                "{path} is inside Quill's own data folder (sign-in keys, the agent token, the message cache); agents cannot send files from there."
            )));
        }
    }
    let forced = match args.get("as").and_then(Value::as_str) {
        None => None,
        Some(word) => FileKind::parse(word).ok_or_else(|| {
            ToolError::new(
                "as is \"auto\", \"photo\", \"video\", \"audio\", \"voice\" or \"document\".",
            )
        })?,
    };
    let kinds: Vec<FileKind> = paths
        .iter()
        .map(|p| forced.unwrap_or_else(|| FileKind::guess(Path::new(p))))
        .collect();
    if paths.len() > 1 {
        let visual = kinds
            .iter()
            .all(|k| matches!(k, FileKind::Photo | FileKind::Video));
        let same = kinds.windows(2).all(|w| w.first() == w.get(1));
        if !(visual || same) || kinds.contains(&FileKind::Voice) {
            return Err(ToolError::new(
                "An album holds photos and videos, or only documents, or only audio files; voice notes go one at a time. Send these in separate calls.",
            ));
        }
    }
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let caption_text = args
        .get("caption")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let caption = if caption_text.trim().is_empty() {
        Value::Null
    } else {
        formatted(&ctx, caption_text, args, CAPTION_LIMIT)?
    };
    let contents: Vec<Value> = paths
        .iter()
        .zip(&kinds)
        .enumerate()
        .map(|(i, (path, kind))| {
            file_content(path, *kind, if i == 0 { &caption } else { &Value::Null })
        })
        .collect();
    let names: Vec<String> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    let request = if contents.len() == 1 {
        json!({
            "@type": "sendMessage",
            "chat_id": target.chat_id,
            "topic_id": Value::Null,
            "reply_to": reply_to(args)?,
            "options": send_options(args)?,
            "reply_markup": Value::Null,
            "input_message_content": contents.first().cloned().unwrap_or(Value::Null),
        })
    } else {
        json!({
            "@type": "sendMessageAlbum",
            "chat_id": target.chat_id,
            "topic_id": Value::Null,
            "reply_to": reply_to(args)?,
            "options": send_options(args)?,
            "input_message_contents": contents,
        })
    };
    let preview = format!(
        "{}{}",
        names.join(", "),
        if caption_text.is_empty() {
            String::new()
        } else {
            format!("\n{caption_text}")
        }
    );
    confirm(&ctx, &target, "send_file", &preview)?;
    let summary = clip(&names.join(", "));
    let count = u32::try_from(paths.len()).unwrap_or(10);
    let sent = common::send(&ctx, &target, "send_file", &summary, request, count)?;
    Ok(sent_output(
        &ctx,
        &target,
        "Sent",
        &sent,
        is_scheduled(args),
    ))
}

pub fn send_poll_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "question": {"type": "string", "description": "Up to 300 characters."},
            "options": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 12, "description": "2 to 12 answers, up to 100 characters each."},
            "anonymous": {"type": "boolean", "default": true},
            "multiple_answers": {"type": "boolean", "default": false},
            "correct_option": {"type": "integer", "description": "Makes it a quiz: the 0-based index of the right answer."},
            "silent": common::silent_schema(),
        },
        "required": ["chat", "question", "options"],
        "additionalProperties": false,
    })
}

pub fn send_poll(args: &Value) -> Result<ToolOutput, ToolError> {
    let question = required_text(args, "question")?;
    let options: Vec<String> = args
        .get("options")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !(2..=12).contains(&options.len()) {
        return Err(ToolError::new("options must list 2 to 12 answers."));
    }
    let last = i64::try_from(options.len()).unwrap_or(12) - 1;
    let correct = match args.get("correct_option") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|c| (0..=last).contains(c))
                .ok_or_else(|| {
                    ToolError::new("correct_option is the 0-based index of one of the options.")
                })?,
        ),
    };
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let text = |t: &str| json!({"@type": "formattedText", "text": t, "entities": []});
    let poll_type = match correct {
        Some(index) => json!({
            "@type": "inputPollTypeQuiz",
            "correct_option_ids": [index],
            "explanation": text(""),
            "explanation_media": Value::Null,
        }),
        None => json!({"@type": "inputPollTypeRegular", "allow_adding_options": false}),
    };
    let request = json!({
        "@type": "sendMessage",
        "chat_id": target.chat_id,
        "topic_id": Value::Null,
        "reply_to": Value::Null,
        "options": send_options(args)?,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessagePoll",
            "question": text(&question),
            "options": options.iter().map(|o| json!({"@type": "inputPollOption", "text": text(o), "media": Value::Null})).collect::<Vec<_>>(),
            "description": Value::Null,
            "media": Value::Null,
            "is_anonymous": args.get("anonymous").and_then(Value::as_bool).unwrap_or(true),
            "allows_multiple_answers": correct.is_none() && args.get("multiple_answers").and_then(Value::as_bool).unwrap_or(false),
            "allows_revoting": true,
            "members_only": false,
            "country_codes": [],
            "shuffle_options": false,
            "hide_results_until_closes": false,
            "type": poll_type,
            "open_period": 0,
            "close_date": 0,
            "is_closed": false,
        },
    });
    let preview = format!("{question}\n• {}", options.join("\n• "));
    confirm(&ctx, &target, "send_poll", &preview)?;
    let summary = format!("\"{}\"", clip(&question));
    let sent = common::send(&ctx, &target, "send_poll", &summary, request, 1)?;
    Ok(sent_output(&ctx, &target, "Sent", &sent, false))
}

pub fn send_sticker_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chat": chat_schema(),
            "emoji": {"type": "string", "description": "An emoji; Quill sends the user's first installed sticker for it."},
            "reply_to": common::reply_schema(),
        },
        "required": ["chat", "emoji"],
        "additionalProperties": false,
    })
}

pub fn send_sticker(args: &Value) -> Result<ToolOutput, ToolError> {
    let emoji = required_text(args, "emoji")?;
    let ctx = Ctx::active()?;
    let target = require(&ctx, args, Scope::ReadWrite)?;
    let found = ctx.call(json!({
        "@type": "getStickers",
        "sticker_type": {"@type": "stickerTypeRegular"},
        "query": emoji.trim(),
        "limit": 1,
        "chat_id": target.chat_id,
    }))?;
    let sticker = found
        .get("stickers")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .cloned()
        .ok_or_else(|| {
            ToolError::new(format!(
                "The user has no sticker for {emoji}. Try another emoji."
            ))
        })?;
    let file_id = sticker
        .get("sticker")
        .and_then(|f| f.get("id"))
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            ToolError::new("Telegram returned a sticker without a file. Try another emoji.")
        })?;
    let request = json!({
        "@type": "sendMessage",
        "chat_id": target.chat_id,
        "topic_id": Value::Null,
        "reply_to": reply_to(args)?,
        "options": send_options(&json!({}))?,
        "reply_markup": Value::Null,
        "input_message_content": {
            "@type": "inputMessageSticker",
            "sticker": {
                "@type": "inputSticker",
                "sticker": {"@type": "inputFileId", "id": file_id},
                "thumbnail": Value::Null,
                "width": sticker.get("width").and_then(Value::as_i64).unwrap_or(0),
                "height": sticker.get("height").and_then(Value::as_i64).unwrap_or(0),
            },
            "emoji": emoji.trim(),
        },
    });
    confirm(&ctx, &target, "send_sticker", &emoji)?;
    let summary = emoji.trim().to_string();
    let sent = common::send(&ctx, &target, "send_sticker", &summary, request, 1)?;
    Ok(sent_output(&ctx, &target, "Sent", &sent, false))
}

pub fn forward_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "from_chat": chat_schema(),
            "message_ids": common::ids_schema("Messages in from_chat to forward, oldest first."),
            "to_chat": chat_schema(),
            "hide_sender": {"type": "boolean", "default": false, "description": "Send as copies, without \"Forwarded from\"."},
            "silent": common::silent_schema(),
        },
        "required": ["from_chat", "message_ids", "to_chat"],
        "additionalProperties": false,
    })
}

pub fn forward_messages(args: &Value) -> Result<ToolOutput, ToolError> {
    let ids = message_ids(args, "message_ids")?;
    let ctx = Ctx::active()?;
    let from = require(&ctx, &json!({"chat": args.get("from_chat")}), Scope::Read)?;
    let to = require(
        &ctx,
        &json!({"chat": args.get("to_chat")}),
        Scope::ReadWrite,
    )?;
    let hide_sender = args
        .get("hide_sender")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let request = json!({
        "@type": "forwardMessages",
        "chat_id": to.chat_id,
        "topic_id": Value::Null,
        "from_chat_id": from.chat_id,
        "message_ids": ids,
        "options": send_options(args)?,
        "send_copy": hide_sender,
        "remove_caption": false,
    });
    let preview = format!("{} × \"{}\"", ids.len(), from.title);
    confirm(&ctx, &to, "forward_messages", &preview)?;
    let summary = preview.clone();
    let count = u32::try_from(ids.len()).unwrap_or(100);
    let sent = common::send(&ctx, &to, "forward_messages", &summary, request, count)?;
    Ok(sent_output(&ctx, &to, "Forwarded", &sent, false))
}
