//! Load state of a forum topic's first history page, derived from the topic
//! store, the failure marks and the in-flight requests (the topic twin of
//! `history_loading` / `history_load_failed`).
use crate::state::*;

impl Session {
    /// The topic's first page failed or timed out and nothing is loaded:
    /// the pane offers "Couldn't load messages · Retry".
    pub fn topic_history_load_failed(&self, chat_id: ChatId, forum_topic_id: i32) -> bool {
        let key = (chat_id.0, forum_topic_id);
        self.threads.topic_load_failed.contains(&key)
            && self
                .threads
                .topic_histories
                .get(&key)
                .is_none_or(|h| h.messages.is_empty())
    }

    /// The topic has nothing to show yet because its first page is on the
    /// way (no entry yet, or an empty window with a page in flight): the
    /// pane shows a skeleton rather than "No messages in this topic".
    pub fn topic_history_loading(&self, chat_id: ChatId, forum_topic_id: i32) -> bool {
        if self.topic_history_load_failed(chat_id, forum_topic_id) {
            return false;
        }
        match self
            .threads
            .topic_histories
            .get(&(chat_id.0, forum_topic_id))
        {
            None => true,
            Some(history) => {
                history.messages.is_empty()
                    && !history.loaded_complete
                    && self.topic_page_in_flight(chat_id, forum_topic_id, WindowEnd::Oldest)
            }
        }
    }

    pub(crate) fn mark_topic_load_failed(&mut self, pending: &PendingRequest) {
        if let Some(chat_id) = pending.chat_id
            && let Some(forum_topic_id) = pending.forum_topic_id
        {
            self.threads
                .topic_load_failed
                .insert((chat_id.0, forum_topic_id));
        }
    }

    pub(crate) fn clear_topic_load_failed(&mut self, chat_id: ChatId, forum_topic_id: i32) {
        self.threads
            .topic_load_failed
            .remove(&(chat_id.0, forum_topic_id));
    }
}
