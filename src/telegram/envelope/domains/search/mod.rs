//! TDLib updates and answers for global and in-chat search, top chats and date jumps.
mod parse;

use crate::ids::MessageId;
use crate::telegram::envelope::*;
pub(crate) use parse::parse_search_payload;

/// Payloads for global and in-chat search, top chats and date jumps; wrapped as
/// [`EnvelopePayload::Search`].
#[derive(Debug, Clone, PartialEq)]
pub enum SearchPayload {
    /// `foundMessages` — `searchMessages` (and secret-chat search).
    FoundMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_offset: String,
    },
    /// `foundPublicPosts` — `searchPublicPosts` (schema 1.8.67, line 3182).
    FoundPublicPosts {
        messages: Vec<ParsedMessage>,
        next_offset: String,
        are_limits_exceeded: bool,
    },
    /// `foundChatMessages` — `searchChatMessages`.
    FoundChatMessages {
        total_count: i32,
        messages: Vec<ParsedMessage>,
        next_from_message_id: MessageId,
    },
    /// `messageCalendar` — `getChatMessageCalendar` (schema line 3194):
    /// per-day counts, newest day first.
    MessageCalendar {
        total_count: i32,
        days: Vec<CalendarDay>,
    },
}
