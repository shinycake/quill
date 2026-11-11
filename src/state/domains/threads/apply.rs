//! Applies TDLib updates and answers for forum topics, comment threads and Saved Messages.
use crate::lang::{keys, tr};
use crate::state::*;
use crate::telegram::envelope::ThreadsPayload;

impl Session {
    /// Applies one threads payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_threads_payload(
        &mut self,
        payload: ThreadsPayload,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            ThreadsPayload::UpdateChatViewAsTopics {
                chat_id,
                view_as_topics,
            } => self.set_chat_view_as_topics(chat_id.0, view_as_topics),
            ThreadsPayload::UpdateSavedMessagesTopic(topic) => self.apply_saved_topic(*topic),
            ThreadsPayload::UpdateSavedMessagesTopicCount { topic_count } => {
                self.threads.saved.topic_count = topic_count;
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
                    self.keep_dirty_topic_drafts(chat_id, &mut topics);
                    for topic in &mut topics {
                        crate::state::session_subsection_tabs::settle_topic_unread(topic);
                        crate::state::session_subsection_tabs::trace_topic_unread(
                            "getForumTopics",
                            chat_id.0,
                            topic,
                        );
                    }
                    self.threads.forum_topics_failed.remove(&chat_id.0);
                    self.threads.forum_topics.insert(chat_id.0, topics);
                }
            }
            // Slice G2: `createForumTopic` answers `forumTopicInfo` —
            // drop the cached topic list so the UI refetches it.
            ThreadsPayload::ForumTopic { chat_id, info } => match pending.map(|p| p.purpose) {
                Some(RequestPurpose::CreateForumTopic) => {
                    self.threads.forum_topics.remove(&chat_id);
                }
                // The new thread joins the tabs now, so the view can open
                // it before any refetch.
                Some(RequestPurpose::StartBotThread) => {
                    let Some(request) = extra else { return };
                    let outcome = match info {
                        Some(info) => {
                            let topic = info.forum_topic_id;
                            self.apply_update_forum_topic_info(*info);
                            // A thread made just now has nothing older to
                            // page in.
                            let key = (chat_id, topic);
                            if !self.threads.topic_histories.contains_key(&key) {
                                self.topic_history_entry(ChatId(chat_id), topic)
                                    .loaded_complete = true;
                            }
                            Ok(topic)
                        }
                        None => Err(tr(keys::Q_CHAT_COULDN_T_START_A_NEW_THREAD).to_string()),
                    };
                    self.bot_thread_answered(request, outcome);
                }
                _ => {}
            },
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
            ThreadsPayload::UpdateDirectMessagesChatTopic(topic) => {
                self.apply_direct_topic(*topic);
            }
            ThreadsPayload::DirectMessagesChatTopic(topic) => {
                if pending.is_some_and(|p| {
                    matches!(
                        p.purpose,
                        RequestPurpose::Threads(ThreadsPurpose::GetDirectMessagesChatTopic { .. })
                    )
                }) {
                    self.apply_direct_topic(*topic);
                }
            }
            ThreadsPayload::MessageThreadInfo(info) => {
                self.apply_message_thread_info(*info, pending);
            }
        }
    }
}
