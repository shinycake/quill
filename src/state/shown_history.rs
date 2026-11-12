//! The messages the open chat's view draws: a forum topic shows its own
//! history, not the chat's main one, so anything that picks rows (a drag
//! selection, "Select up to this") reads them from here.
use super::*;

impl Session {
    /// The loaded messages `chat_id`'s view draws: the open topic's page
    /// when that topic is open in this chat, else the chat's main history.
    pub fn shown_history(&self, chat_id: ChatId) -> Option<&HistoryMessages> {
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
