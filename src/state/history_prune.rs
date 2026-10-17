//! Bounded memory for the message histories of chats nobody is looking at.
//!
//! `Session::histories` and `ThreadsState::topic_histories` used to keep
//! every chat's loaded window for the whole session, trimmed only per chat.
//! Telegram Desktop keeps rows for the chats on screen and lets the rest go
//! (`Data::Histories`, `History::clear`, `HistoryBlock` unloading); a chat
//! opened again is shown from what is cached and then loads what is
//! missing. This module does the same from state transitions, never from
//! a timer:
//!
//! - a chat (or forum topic) that stops being shown keeps only its newest
//!   [`CLOSED_TAIL_KEEP`] messages, enough to show at once on reopen;
//! - such tails are kept for the [`CLOSED_CHATS_MAX`] most recently used
//!   chats ([`CLOSED_TOPICS_MAX`] topics); the least recently used one is
//!   dropped whole;
//! - a chat that was left or deleted is dropped as soon as nothing shows it;
//! - live messages landing in a chat nobody shows keep that tail bounded
//!   ([`CLOSED_TAIL_CAP`]) and count as use of the chat.
//!
//! A history is never pruned while any view shows it (the main window, a
//! chat window, a topic or comment thread of it) or while the UI holds it
//! ([`Session::set_history_holds`]: the source of a pending forward, the
//! chat of the playing track), and an entry with an unsent or failed local
//! message is never dropped whole.
use super::*;
use std::collections::{BTreeSet, VecDeque};

/// Newest messages a chat or topic keeps once it is no longer shown.
pub const CLOSED_TAIL_KEEP: usize = 60;
/// A closed chat's tail may grow to this many live messages before it is
/// trimmed back to [`CLOSED_TAIL_KEEP`].
pub const CLOSED_TAIL_CAP: usize = 100;
/// Closed chats that keep a tail; one more drops the least recently used.
pub const CLOSED_CHATS_MAX: usize = 16;
/// Closed forum topics that keep a tail.
pub const CLOSED_TOPICS_MAX: usize = 16;

/// Which closed histories are kept, most recently used first.
#[derive(Debug, Default)]
pub struct HistoryPruning {
    closed_chats: VecDeque<i64>,
    closed_topics: VecDeque<(i64, i32)>,
    holds: BTreeSet<ChatId>,
}

impl HistoryPruning {
    /// Closed chats with a kept tail, most recently used first.
    pub fn closed_chats(&self) -> impl Iterator<Item = ChatId> + '_ {
        self.closed_chats.iter().copied().map(ChatId)
    }

    /// Closed topics with a kept tail, most recently used first.
    pub fn closed_topics(&self) -> impl Iterator<Item = (ChatId, i32)> + '_ {
        self.closed_topics
            .iter()
            .map(|&(chat, topic)| (ChatId(chat), topic))
    }

    fn touch_chat(&mut self, chat_id: i64) {
        self.closed_chats.retain(|id| *id != chat_id);
        self.closed_chats.push_front(chat_id);
    }

    fn touch_topic(&mut self, key: (i64, i32)) {
        self.closed_topics.retain(|k| *k != key);
        self.closed_topics.push_front(key);
    }
}

/// The ids to drop to leave `keep` messages: the oldest ones, never a
/// pending or failed local send.
fn tail_excess(messages: &HistoryMessages, keep: usize) -> Vec<i64> {
    let excess = messages.len().saturating_sub(keep);
    messages
        .iter()
        .filter(|(_, message)| !message.pending && !message.failed)
        .map(|(id, _)| *id)
        .take(excess)
        .collect()
}

impl HistoryState {
    /// Keep the newest `keep` messages. The window no longer reaches the
    /// chat's start.
    pub(crate) fn trim_to_tail(&mut self, keep: usize) -> usize {
        let doomed = tail_excess(&self.messages, keep);
        for id in &doomed {
            self.messages.remove(id);
            self.visible.remove(id);
            self.viewed.remove(id);
        }
        if !doomed.is_empty() {
            self.loaded_complete = false;
        }
        doomed.len()
    }

    /// Whether dropping this entry would lose a local send in flight.
    fn has_unsent(&self) -> bool {
        self.messages
            .values()
            .any(|message| message.pending || message.failed)
    }
}

impl TopicHistory {
    /// [`HistoryState::trim_to_tail`] for a topic: the paging cursor moves
    /// to the oldest kept message so older pages continue from there.
    pub(crate) fn trim_to_tail(&mut self, keep: usize) -> usize {
        let doomed = tail_excess(&self.messages, keep);
        for id in &doomed {
            self.messages.remove(id);
        }
        if !doomed.is_empty() {
            self.loaded_complete = false;
            if let Some(oldest) = self.messages.keys().next() {
                self.next_from_message_id = MessageId(*oldest);
            }
        }
        doomed.len()
    }
}

impl Session {
    /// The UI's claim on histories that are read while their chat is not
    /// shown: the source chat of a pending forward, the chat of the playing
    /// track. Replaces the previous claim; held chats are never pruned.
    pub fn set_history_holds(&mut self, chats: impl IntoIterator<Item = ChatId>) {
        let holds: BTreeSet<ChatId> = chats.into_iter().collect();
        if holds != self.history_pruning.holds {
            self.history_pruning.holds = holds;
        }
    }

    /// Whether some view shows `chat_id` or the UI holds it: its history
    /// must stay whole.
    pub fn chat_history_in_use(&self, chat_id: ChatId) -> bool {
        self.open_chat == Some(chat_id)
            || self.window_chats.contains(&chat_id)
            || self.window_chats.shows_chat(chat_id)
            || self
                .threads
                .thread
                .as_ref()
                .is_some_and(|thread| thread.chat_id == chat_id)
            || self.history_pruning.holds.contains(&chat_id)
    }

    fn topic_history_in_use(&self, chat_id: ChatId, topic: i32) -> bool {
        self.history_pruning.holds.contains(&chat_id)
            || (self.open_chat == Some(chat_id) && self.open_topic == Some(topic))
            || self.window_chats.shows_topic(chat_id, topic)
    }

    /// The chat was left and is in no list: its history has no use.
    fn chat_is_gone(&self, chat_id: ChatId) -> bool {
        self.chats.get(&chat_id.0).is_some_and(|chat| {
            chat.my_member_status == Some(ChannelMemberStatus::Left)
                && !chat.in_main_list
                && !chat.in_archive
        })
    }

    /// A chat is shown again: it is no longer a closed tail.
    pub(crate) fn reclaim_chat_history(&mut self, chat_id: ChatId) {
        self.history_pruning
            .closed_chats
            .retain(|id| *id != chat_id.0);
    }

    /// A topic is shown again: it is no longer a closed tail.
    pub(crate) fn reclaim_topic_history(&mut self, chat_id: ChatId, topic: i32) {
        self.history_pruning
            .closed_topics
            .retain(|key| *key != (chat_id.0, topic));
    }

    /// `chat_id` stopped being shown by one view (the main window moved on
    /// or a chat window closed). When no other view shows it, its history
    /// and its topics shrink to a tail, or go whole for a chat that is gone.
    pub(crate) fn release_chat_history(&mut self, chat_id: ChatId) {
        if self.chat_history_in_use(chat_id) {
            return;
        }
        if self.chat_is_gone(chat_id) {
            self.forget_chat_history(chat_id);
            return;
        }
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.trim_to_tail(CLOSED_TAIL_KEEP);
            self.history_pruning.touch_chat(chat_id.0);
        }
        let topics: Vec<i32> = self
            .threads
            .topic_histories
            .keys()
            .filter(|(chat, _)| *chat == chat_id.0)
            .map(|(_, topic)| *topic)
            .collect();
        for topic in topics {
            self.release_topic_history(chat_id, topic);
        }
        self.enforce_closed_bounds();
    }

    /// `topic` of `chat_id` stopped being shown.
    pub(crate) fn release_topic_history(&mut self, chat_id: ChatId, topic: i32) {
        if self.topic_history_in_use(chat_id, topic) {
            return;
        }
        if let Some(history) = self.threads.topic_histories.get_mut(&(chat_id.0, topic)) {
            history.trim_to_tail(CLOSED_TAIL_KEEP);
            self.history_pruning.touch_topic((chat_id.0, topic));
        }
        self.enforce_closed_bounds();
    }

    /// A chat that was left or deleted: its histories go when no view
    /// shows it (a local send still in flight keeps its entry).
    pub(crate) fn forget_chat_history(&mut self, chat_id: ChatId) {
        if self.chat_history_in_use(chat_id) {
            return;
        }
        if self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| !history.has_unsent())
        {
            self.histories.remove(&chat_id.0);
            self.history_pruning
                .closed_chats
                .retain(|id| *id != chat_id.0);
        }
        self.threads
            .topic_histories
            .retain(|(chat, _), _| *chat != chat_id.0);
        self.history_pruning
            .closed_topics
            .retain(|(chat, _)| *chat != chat_id.0);
    }

    /// A live message landed in a chat no view shows: keep its tail
    /// bounded and mark the chat as recently used.
    pub(crate) fn note_closed_chat_update(&mut self, chat_id: ChatId) {
        if let Some(history) = self.histories.get_mut(&chat_id.0) {
            history.trim_window_end(WindowEnd::Oldest, CLOSED_TAIL_CAP, CLOSED_TAIL_KEEP);
            self.history_pruning.touch_chat(chat_id.0);
            self.enforce_closed_bounds();
        }
    }

    /// Drop the least recently used closed histories beyond the bounds.
    fn enforce_closed_bounds(&mut self) {
        while self.history_pruning.closed_chats.len() > CLOSED_CHATS_MAX {
            let victim = self
                .history_pruning
                .closed_chats
                .iter()
                .rposition(|id| self.chat_history_evictable(ChatId(*id)));
            let Some(id) = victim.and_then(|at| self.history_pruning.closed_chats.remove(at))
            else {
                break;
            };
            self.histories.remove(&id);
        }
        while self.history_pruning.closed_topics.len() > CLOSED_TOPICS_MAX {
            let victim = self
                .history_pruning
                .closed_topics
                .iter()
                .rposition(|&(chat, topic)| !self.topic_history_in_use(ChatId(chat), topic));
            let Some(key) = victim.and_then(|at| self.history_pruning.closed_topics.remove(at))
            else {
                break;
            };
            self.threads.topic_histories.remove(&key);
        }
    }

    fn chat_history_evictable(&self, chat_id: ChatId) -> bool {
        !self.chat_history_in_use(chat_id)
            && self
                .histories
                .get(&chat_id.0)
                .is_none_or(|history| !history.has_unsent())
    }
}
