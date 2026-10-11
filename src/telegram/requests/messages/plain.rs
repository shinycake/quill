//! Methods moved out of `messages.rs` to keep files under 1000 lines.

use super::*;

impl SendReply {
    pub fn plain(message_id: MessageId) -> Self {
        Self {
            message_id,
            quote: None,
            source_chat: None,
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
            source_chat: Some(source_chat),
        }
    }
}
