use super::*;
use crate::ids::MessageId;
use serde_json::Value;

/// `draftMessage` text this slice restores. Voice/video/rich drafts are ignored.
/// `reply_to` is same-chat `inputMessageReplyToMessage` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDraft {
    pub text: String,
    pub reply_to_message_id: Option<MessageId>,
    /// Slice G1: partial-message quote carried by the draft's `reply_to`
    /// (`inputTextQuote`, schema 1.8.67 line 3056) — `(text, position)`
    /// with `position` in UTF-16 code units.
    pub quote: Option<(String, i32)>,
}

/// `ChatAction` values this slice acts on. Other constructors stay `Other`
/// so a replacement action clears typing without inventing labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// `chatActionTyping`
    Typing,
    /// `chatActionChoosingSticker` (TDLib 1.8.67, line 6380).
    ChoosingSticker,
    /// `chatActionCancel`, or a null action (schema: null cancels).
    Cancel,
    Other,
}

/// `draftMessage` / `draftMessageContentText`. Other content constructors are
/// not restored into the text field (Unigram only fills the field from text).
pub(crate) fn parse_chat_draft(value: Option<&Value>) -> Option<ChatDraft> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("draftMessage") {
        return None;
    }
    let content = value.get("content")?;
    if content.get("@type").and_then(Value::as_str) != Some("draftMessageContentText") {
        return None;
    }
    let text = content
        .get("text")
        .and_then(|formatted| formatted.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let (reply_to_message_id, quote) = match value.get("reply_to") {
        Some(reply)
            if reply.get("@type").and_then(Value::as_str) == Some("inputMessageReplyToMessage") =>
        {
            let id = int53(reply.get("message_id")).ok().map(MessageId);
            let quote = reply.get("quote").and_then(|quote| {
                if quote.get("@type").and_then(Value::as_str) != Some("inputTextQuote") {
                    return None;
                }
                let text = quote
                    .get("text")
                    .and_then(|formatted| formatted.get("text"))
                    .and_then(Value::as_str)?;
                let position = quote.get("position").and_then(Value::as_i64)? as i32;
                Some((text.to_string(), position))
            });
            (id, quote)
        }
        _ => (None, None),
    };
    if text.trim().is_empty() && reply_to_message_id.is_none() {
        return None;
    }
    Some(ChatDraft {
        text,
        reply_to_message_id,
        quote,
    })
}

pub(crate) fn parse_chat_action(value: Option<&Value>) -> ChatAction {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatActionTyping") => ChatAction::Typing,
        Some("chatActionChoosingSticker") => ChatAction::ChoosingSticker,
        None | Some("chatActionCancel") => ChatAction::Cancel,
        Some(_) => ChatAction::Other,
    }
}
