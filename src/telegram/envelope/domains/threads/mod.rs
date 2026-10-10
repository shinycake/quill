//! TDLib updates and answers for forum topics, comment threads and Saved Messages.
mod parse;

use crate::ids::ChatId;
use crate::telegram::envelope::*;
pub(crate) use parse::parse_threads_payload;

/// Payloads for forum topics, comment threads and Saved Messages; wrapped as
/// [`EnvelopePayload::Threads`].
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadsPayload {
    /// `updateChatViewAsTopics` (schema 1.8.67, line 10591).
    UpdateChatViewAsTopics {
        chat_id: ChatId,
        view_as_topics: bool,
    },
    /// `updateSavedMessagesTopic` (schema 1.8.67, line 10616): a Saved
    /// Messages sublist appeared or changed.
    UpdateSavedMessagesTopic(Box<SavedMessagesTopic>),
    /// `updateSavedMessagesTopicCount` (schema line 10619).
    UpdateSavedMessagesTopicCount { topic_count: i32 },
    /// `updateSavedMessagesTags` (schema line 11015): the tags of all Saved
    /// Messages (`saved_messages_topic_id` 0) or of one sublist.
    UpdateSavedMessagesTags {
        saved_messages_topic_id: i64,
        tags: Vec<SavedMessagesTag>,
    },
    /// `savedMessagesTags` — the `getSavedMessagesTags` answer.
    SavedMessagesTags { tags: Vec<SavedMessagesTag> },
    /// `messageThreadInfo` — the answer to `getMessageThread`.
    MessageThreadInfo(Box<ParsedMessageThreadInfo>),
    /// `forumTopics` — `getForumTopics` response. Only the first page is
    /// fetched; `next_offset_*` are dropped (see Phase 5.1 DECISIONS).
    ForumTopics {
        total_count: i32,
        topics: Vec<ForumTopic>,
    },
    /// `forumTopicInfo` — `createForumTopic` answer. Only the chat id is
    /// kept; the topic list is refetched on success.
    ForumTopic { chat_id: i64 },
    /// Subsection tabs: `updateForumTopicInfo` (schema 1.8.67, line 10652).
    UpdateForumTopicInfo(ForumTopicInfoUpdate),
    /// Subsection tabs: `updateForumTopic` (schema 1.8.67, line 10665).
    UpdateForumTopic(ForumTopicUpdate),
    /// Subsection tabs: `forumTopic` — the `getForumTopic` answer.
    ForumTopicAnswer(ForumTopic),
}
