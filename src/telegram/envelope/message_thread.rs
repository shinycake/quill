use super::*;
use crate::ids::ChatId;
use serde_json::Value;

/// `messageThreadInfo` (TDLib 1.8.67, schema line 3897): the answer to
/// `getMessageThread`. For a channel post `chat_id` is the linked
/// discussion group and `messages` starts with the thread root there.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMessageThreadInfo {
    pub chat_id: ChatId,
    pub message_thread_id: i64,
    pub reply_info: Option<MessageReplyInfo>,
    pub unread_message_count: i32,
    pub messages: Vec<ParsedMessage>,
}

/// `message.topic_id` as a comment / reply thread (`messageTopicThread`).
pub(crate) fn parse_message_thread_topic(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("messageTopicThread") => int53(value.get("message_thread_id")).ok(),
        _ => None,
    }
}

pub(crate) fn parse_message_thread_info(
    value: &Value,
) -> Result<ParsedMessageThreadInfo, ParseError> {
    Ok(ParsedMessageThreadInfo {
        chat_id: ChatId(int53(value.get("chat_id"))?),
        message_thread_id: int53(value.get("message_thread_id"))?,
        reply_info: parse_reply_info(value.get("reply_info")),
        unread_message_count: value
            .get("unread_message_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        messages: value
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|message| parse_message(message).ok())
            .collect(),
    })
}
