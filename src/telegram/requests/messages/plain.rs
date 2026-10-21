//! Methods moved out of `messages.rs` to keep files under 1000 lines.

use super::*;

impl SendReply {
    pub fn plain(message_id: MessageId) -> Self {
        Self {
            message_id,
            quote: None,
            quote_rich: Vec::new(),
            source_chat: None,
            item: ReplyItem::Message,
        }
    }

    /// A reply to a message of another chat, optionally quoting part of it.
    pub fn external(
        source_chat: ChatId,
        message_id: MessageId,
        quote: Option<(String, i32)>,
    ) -> Self {
        Self {
            message_id,
            quote,
            quote_rich: Vec::new(),
            source_chat: Some(source_chat),
            item: ReplyItem::Message,
        }
    }
}
