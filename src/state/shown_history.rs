//! The messages a chat's view draws, in the view's own order: an open
//! comment thread, then an open forum topic, then the chat's main history.
//! Anything that picks rows (a drag selection, "Select up to this") reads
//! them from here. Saved Messages sublists and tag filters live in the UI
//! (`QuillApp::shown_history`), which asks here for every other view.
use super::*;

impl Session {
    /// The loaded messages `chat_id`'s view draws: the open thread's
    /// replies (a failed thread falls through, as the view does), else the
    /// open topic's page, else the chat's main history.
    pub fn shown_history(&self, chat_id: ChatId) -> Option<&HistoryMessages> {
        if self.open.chat() == Some(chat_id)
            && let Some(thread) = self
                .thread_for_chat(chat_id)
                .filter(|thread| !matches!(thread.status, ThreadStatus::Failed(_)))
        {
            return Some(&thread.history.messages);
        }
        if self.open.chat() == Some(chat_id)
            && let Some(topic) = self.open.topic()
        {
            return self
                .threads
                .topic_histories
                .get(&(chat_id.0, topic))
                .map(|history| &history.messages);
        }
        self.histories.get(&chat_id.0).map(|h| &h.messages)
    }

    /// One message of `chat_id` as the view draws it.
    pub fn shown_message(&self, chat_id: ChatId, id: MessageId) -> Option<&HistoryMessage> {
        self.shown_history(chat_id)?.get(&id.0)
    }
}
