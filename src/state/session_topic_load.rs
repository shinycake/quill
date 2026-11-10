//! Load state of a forum topic's first history page, derived from the topic
//! store, the older-end stage and the in-flight requests (the topic twin of
//! `history_loading` / `history_load_failed`).
use crate::state::*;

impl Session {
    /// The topic's first page failed or timed out and nothing is loaded:
    /// the pane offers "Couldn't load messages · Retry".
    pub fn topic_history_load_failed(&self, chat_id: ChatId, forum_topic_id: i32) -> bool {
        self.threads
            .topic_histories
            .get(&(chat_id.0, forum_topic_id))
            .is_some_and(|h| h.older == OlderEnd::Failed && h.messages.is_empty())
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

    /// Reopen the topic's older end after a failure (a retry, a landed
    /// page or re-selecting the topic): the one stage the first-page Retry
    /// and the older-page Retry strip share.
    pub(crate) fn clear_topic_load_failed(&mut self, chat_id: ChatId, forum_topic_id: i32) {
        if let Some(history) = self
            .threads
            .topic_histories
            .get_mut(&(chat_id.0, forum_topic_id))
        {
            history.older = OlderEnd::Open;
        }
    }
}
