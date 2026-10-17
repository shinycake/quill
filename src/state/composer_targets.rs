//! What the composer's reply and edit bars point at, read from the session
//! each time they are needed. The bars store ids; the text, the preview and
//! whether the message still exists all come from here (tdesktop
//! `FieldHeader`: the shown message is looked up, an edit refreshes it, and
//! a destroyed message cancels the reply or the edit).
use super::*;
use crate::composer::ComposerEdit;
use std::time::{Duration, Instant};

/// Most fetched reply-bar targets kept; a new fetch past it clears the rest.
const COMPOSER_REPLY_FETCH_MAX: usize = 32;

/// Failed fetches are asked again this many times in all.
const COMPOSER_REPLY_ATTEMPTS_MAX: u8 = 3;
/// Wait before the first retry; each further one waits twice as long.
const COMPOSER_REPLY_RETRY_BASE: Duration = Duration::from_secs(2);

/// A reply-bar target that was not in memory (a closed chat keeps only its
/// newest rows, a restored draft points anywhere), fetched with `getMessage`.
/// Kept beside the history: a lone old row must never enter the loaded
/// window, which has to stay one contiguous run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerReplyFetch {
    Loading {
        /// Requests sent before this one (retries of a failed fetch).
        attempts: u8,
    },
    Loaded(Box<HistoryMessage>),
    /// TDLib answered 400, 403 or 404: deleted, or no access.
    Gone,
    /// The request failed otherwise (lost, offline, flood). Asked again with
    /// a growing wait while the connection is up, a few times in all.
    Failed {
        attempts: u8,
        at: Instant,
    },
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
    /// known deleted and not asked for yet, or asked and failed, with the
    /// connection back and the retry wait over.
    pub fn composer_reply_fetch_wanted(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        if message_id.0 <= 0
            || self.reply_target(chat_id, message_id).is_some()
            || self.message_deleted(chat_id, message_id)
        {
            return false;
        }
        match self
            .messages
            .composer_reply_fetch
            .get(&(chat_id.0, message_id.0))
        {
            None => true,
            Some(ComposerReplyFetch::Failed { attempts, at }) => {
                *attempts < COMPOSER_REPLY_ATTEMPTS_MAX
                    && self.connection == ConnectionState::Ready
                    && at.elapsed() >= retry_wait(*attempts)
            }
            Some(_) => false,
        }
    }

    /// `getMessage` was sent for the reply bar. Past the entry cap, other
    /// settled entries make room (never one in flight, never this one).
    pub fn begin_composer_reply_fetch(&mut self, chat_id: ChatId, message_id: MessageId) {
        let key = (chat_id.0, message_id.0);
        let fetch = &mut self.messages.composer_reply_fetch;
        let attempts = match fetch.get(&key) {
            Some(ComposerReplyFetch::Failed { attempts, .. }) => *attempts,
            _ => 0,
        };
        if fetch.len() >= COMPOSER_REPLY_FETCH_MAX && !fetch.contains_key(&key) {
            let mut settled: Vec<(i64, i64)> = fetch
                .iter()
                .filter(|(_, state)| !matches!(state, ComposerReplyFetch::Loading { .. }))
                .map(|(key, _)| *key)
                .collect();
            settled.sort_unstable();
            let excess = (fetch.len() + 1).saturating_sub(COMPOSER_REPLY_FETCH_MAX);
            for doomed in settled.into_iter().take(excess) {
                fetch.remove(&doomed);
            }
        }
        fetch.insert(key, ComposerReplyFetch::Loading { attempts });
    }

    /// The request never left (the sender is closed): counts as a failed
    /// attempt, so the bar does not retry every frame.
    pub fn fail_composer_reply_fetch(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.settle_composer_reply_failure(chat_id, message_id);
    }

    fn settle_composer_reply_failure(&mut self, chat_id: ChatId, message_id: MessageId) {
        let key = (chat_id.0, message_id.0);
        let attempts = match self.messages.composer_reply_fetch.get(&key) {
            Some(ComposerReplyFetch::Loading { attempts }) => attempts.saturating_add(1),
            Some(ComposerReplyFetch::Failed { attempts, .. }) => attempts.saturating_add(1),
            _ => 1,
        };
        self.messages.composer_reply_fetch.insert(
            key,
            ComposerReplyFetch::Failed {
                attempts,
                at: Instant::now(),
            },
        );
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
        // Only a definite answer means the message is gone. A lost request
        // (408), a flood wait (429) or a server fault says nothing about it.
        if matches!(err.code, 400 | 403 | 404) {
            self.messages
                .composer_reply_fetch
                .insert((chat_id.0, message_id.0), ComposerReplyFetch::Gone);
        } else {
            self.settle_composer_reply_failure(chat_id, message_id);
        }
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

/// How long a failed fetch waits before attempt `attempts + 1`.
fn retry_wait(attempts: u8) -> Duration {
    COMPOSER_REPLY_RETRY_BASE.saturating_mul(1u32 << u32::from(attempts.min(6)))
}
