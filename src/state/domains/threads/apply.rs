//! Applies TDLib updates and answers for forum topics, comment threads and Saved Messages.
use crate::state::*;
use crate::telegram::envelope::ThreadsPayload;

impl Session {
    /// Applies one threads payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_threads_payload(
        &mut self,
        payload: ThreadsPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            ThreadsPayload::UpdateChatViewAsTopics {
                chat_id,
                view_as_topics,
            } => self.set_chat_view_as_topics(chat_id.0, view_as_topics),
            ThreadsPayload::UpdateSavedMessagesTopic(topic) => self.apply_saved_topic(*topic),
            ThreadsPayload::UpdateSavedMessagesTopicCount { topic_count } => {
                self.saved.topic_count = topic_count;
            }
            ThreadsPayload::UpdateSavedMessagesTags {
                saved_messages_topic_id,
                tags,
            } => self.apply_saved_tags(saved_messages_topic_id, tags),
            ThreadsPayload::SavedMessagesTags { tags } => {
                if let Some(RequestPurpose::GetSavedMessagesTags { topic_id }) =
                    pending.map(|p| p.purpose)
                {
                    self.apply_saved_tags(topic_id, tags);
                }
            }
            // Phase 5.1: `getForumTopics` response — cache the first page
            // against the requesting chat.
            ThreadsPayload::ForumTopics {
                total_count: _,
                topics,
            } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetForumTopics)
                    && let Some(chat_id) = pending.and_then(|p| p.chat_id)
                {
                    let mut topics = topics;
                    for topic in &mut topics {
                        crate::state::session_subsection_tabs::settle_topic_unread(topic);
                        crate::state::session_subsection_tabs::trace_topic_unread(
                            "getForumTopics",
                            chat_id.0,
                            topic,
                        );
                    }
                    self.forum_topics.insert(chat_id.0, topics);
                }
            }
            // Slice G2: `createForumTopic` answers `forumTopicInfo` —
            // drop the cached topic list so the UI refetches it.
            ThreadsPayload::ForumTopic { chat_id } => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::CreateForumTopic) {
                    self.forum_topics.remove(&chat_id);
                }
            }
            ThreadsPayload::UpdateForumTopicInfo(info) => {
                self.apply_update_forum_topic_info(info);
            }
            ThreadsPayload::UpdateForumTopic(update) => {
                self.apply_update_forum_topic(update);
            }
            ThreadsPayload::ForumTopicAnswer(topic) => {
                if let Some(pending) = pending
                    && matches!(
                        pending.purpose,
                        RequestPurpose::Threads(ThreadsPurpose::GetForumTopic { .. })
                    )
                    && let Some(chat_id) = pending.chat_id
                {
                    self.replace_forum_topic(chat_id, topic);
                }
            }
            ThreadsPayload::MessageThreadInfo(info) => {
                self.apply_message_thread_info(*info, pending);
            }
        }
    }
}
