//! What the composer's reply and edit bars point at, read from the session
//! each time they are needed. The bars store ids; the text, the preview and
//! whether the message still exists all come from here (tdesktop
//! `FieldHeader`: the shown message is looked up, an edit refreshes it, and
//! a destroyed message cancels the reply or the edit).
use super::*;
use crate::composer::ComposerEdit;

/// Most fetched reply-bar targets kept; a new fetch past it clears the rest.
const COMPOSER_REPLY_FETCH_MAX: usize = 32;

/// A reply-bar target that was not in memory (a closed chat keeps only its
/// newest rows, a restored draft points anywhere), fetched with `getMessage`.
/// Kept beside the history: a lone old row must never enter the loaded
/// window, which has to stay one contiguous run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerReplyFetch {
    Loading,
    Loaded(Box<HistoryMessage>),
    /// TDLib answered with a 4xx: deleted, or no access.
    Gone,
    /// The request failed otherwise (offline, flood); not asked again until
    /// the entry is cleared.
    Failed,
}

impl Session {
    /// The message a reply points at, looked up where the composer is: the
    /// chat history, the open thread, the open topic's history, or the copy
    /// fetched for the bar.
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
            .or_else(|| {
                match self
                    .messages
                    .composer_reply_fetch
                    .get(&(chat_id.0, message_id.0))
                {
                    Some(ComposerReplyFetch::Loaded(found)) => Some(found.as_ref()),
                    _ => None,
                }
            })
    }

    /// Whether the reply bar's message must be fetched: not in memory, not
    /// known deleted and not asked for yet.
    pub fn composer_reply_fetch_wanted(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        message_id.0 > 0
            && self.reply_target(chat_id, message_id).is_none()
            && !self.message_deleted(chat_id, message_id)
            && !self
                .messages
                .composer_reply_fetch
                .contains_key(&(chat_id.0, message_id.0))
    }

    /// `getMessage` was sent for the reply bar.
    pub fn begin_composer_reply_fetch(&mut self, chat_id: ChatId, message_id: MessageId) {
        let fetch = &mut self.messages.composer_reply_fetch;
        if fetch.len() >= COMPOSER_REPLY_FETCH_MAX {
            fetch.retain(|_, state| *state == ComposerReplyFetch::Loading);
        }
        fetch.insert((chat_id.0, message_id.0), ComposerReplyFetch::Loading);
    }

    /// The `getMessage` answer for the reply bar.
    pub(crate) fn accept_composer_reply_target(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        message: ParsedMessage,
    ) {
        self.remember_files(&message.files);
        self.messages.composer_reply_fetch.insert(
            (chat_id.0, message_id.0),
            ComposerReplyFetch::Loaded(Box::new(history_message(message, false))),
        );
    }

    /// `getMessage` failed for the reply bar.
    pub(crate) fn reject_composer_reply_target(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        err: &TdError,
    ) {
        let state = if (400..500).contains(&err.code) {
            ComposerReplyFetch::Gone
        } else {
            ComposerReplyFetch::Failed
        };
        self.messages
            .composer_reply_fetch
            .insert((chat_id.0, message_id.0), state);
    }

    /// The message was deleted (or became inaccessible): its id is a
    /// tombstone in the chat history, or in the open thread or a topic
    /// history of the chat (the places `reply_target` looks in). A message
    /// that is merely not loaded is not gone.
    pub fn message_deleted(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.histories
            .get(&chat_id.0)
            .is_some_and(|history| history.is_tombstone(message_id))
            || self.history_pruning.dropped_tombstone(chat_id, message_id)
            || self
                .messages
                .composer_reply_fetch
                .get(&(chat_id.0, message_id.0))
                == Some(&ComposerReplyFetch::Gone)
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
