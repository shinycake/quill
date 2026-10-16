//! What the composer's reply and edit bars point at, read from the session
//! each time they are needed. The bars store ids; the text, the preview and
//! whether the message still exists all come from here (tdesktop
//! `FieldHeader`: the shown message is looked up, an edit refreshes it, and
//! a destroyed message cancels the reply or the edit).
use super::*;
use crate::composer::ComposerEdit;

impl Session {
    /// The message a reply points at, looked up where the composer is: the
    /// chat history, the open thread, or the open topic's history.
    pub fn reply_target(&self, chat_id: ChatId, message_id: MessageId) -> Option<&HistoryMessage> {
        self.histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .or_else(|| {
                self.threads
                    .thread
                    .as_ref()
                    .filter(|thread| thread.chat_id == chat_id)
                    .and_then(|thread| thread.history.messages.get(&message_id.0))
            })
            .or_else(|| {
                let topic = self.open_topic?;
                self.threads
                    .topic_histories
                    .get(&(chat_id.0, topic))
                    .and_then(|history| history.messages.get(&message_id.0))
            })
    }

    /// The message was deleted (or became inaccessible): its id is a
    /// tombstone in the chat history, or in the open thread or a topic
    /// history of the chat (the places `reply_target` looks in). A message
    /// that is merely not loaded is not gone.
    pub fn message_deleted(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.histories
            .get(&chat_id.0)
            .is_some_and(|history| history.is_tombstone(message_id))
            || self
                .threads
                .thread
                .as_ref()
                .filter(|thread| thread.chat_id == chat_id)
                .is_some_and(|thread| thread.history.is_tombstone(message_id))
            || self
                .threads
                .topic_histories
                .iter()
                .any(|((topic_chat, _), history)| {
                    *topic_chat == chat_id.0 && history.is_tombstone(message_id)
                })
    }

    /// The text or caption an edit shows now, from the live message (the
    /// chat history, the open thread or topic). `None` while the message is
    /// not loaded: an edit must not begin on a blank field.
    pub fn edit_target_text(&self, edit: &ComposerEdit) -> Option<String> {
        let content = if edit.scheduled {
            &self
                .messages
                .scheduled_messages
                .iter()
                .find(|m| m.chat_id == edit.chat_id && m.id == edit.message_id)?
                .content
        } else {
            &self.reply_target(edit.chat_id, edit.message_id)?.content
        };
        ComposerEdit::editable_text(content).map(|(_, text)| text)
    }

    /// The edited message is gone: deleted from the history, or no longer
    /// scheduled (sent or deleted elsewhere).
    pub fn edit_target_gone(&self, edit: &ComposerEdit) -> bool {
        if edit.scheduled {
            return !self
                .messages
                .scheduled_messages
                .iter()
                .any(|m| m.chat_id == edit.chat_id && m.id == edit.message_id);
        }
        self.message_deleted(edit.chat_id, edit.message_id)
    }
}
