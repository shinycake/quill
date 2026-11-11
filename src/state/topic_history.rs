//! Per-topic history of a forum supergroup.
use super::*;

/// Phase 5.1: per-topic history for a forum supergroup, fetched with
/// `searchChatMessages` (`topic_id = messageTopicForum`, empty query).
/// `next_from_message_id` pages older messages the same way `foundChatMessages`
/// does for in-chat search; `loaded_complete` once a page returns
/// `next_from_message_id` 0 (or an empty page). The window is capped at
/// both ends like the main history (`topic_window`).
#[derive(Debug)]
pub struct TopicHistory {
    pub messages: HistoryMessages,
    pub next_from_message_id: MessageId,
    /// Subsection tabs: `foundChatMessages.total_count` of the latest
    /// page — the topic header's "N messages".
    pub total_count: i32,
    /// Ids deleted while this history was loaded (or when it was not): a
    /// stale page cannot bring them back, and the composer reads them as
    /// "the message is gone" (`Session::message_deleted`).
    pub tombstones: HashSet<i64>,
    pub reader: ReaderPosition,
    /// The newer end: while it stops short of the newest message, newer
    /// pages load as the reader scrolls down and live messages wait
    /// outside the window; a failed page stops auto-paging until a jump.
    pub newer: NewerEnd,
    /// The older end: reached the start, or the last older page failed
    /// and auto-paging stops until a jump.
    pub older: OlderEnd,
    /// The newest server id known to lie past the window's newest end
    /// (trimmed away or held out while `has_newer`).
    pub latest_seen: i64,
    /// Bumped whenever the window is replaced, so the UI re-anchors its
    /// scroll.
    pub window_epoch: u64,
}

impl Default for TopicHistory {
    fn default() -> Self {
        Self {
            messages: HistoryMessages::default(),
            next_from_message_id: MessageId(0),
            total_count: 0,
            tombstones: HashSet::new(),
            reader: ReaderPosition::Tail,
            newer: NewerEnd::AtLatest,
            older: OlderEnd::Open,
            latest_seen: 0,
            window_epoch: 0,
        }
    }
}

impl TopicHistory {
    /// A page reached the start of the topic.
    pub fn loaded_complete(&self) -> bool {
        self.older == OlderEnd::Complete
    }

    /// The window stops short of the newest message.
    pub fn has_newer(&self) -> bool {
        self.newer != NewerEnd::AtLatest
    }

    /// The last newer page failed.
    pub fn newer_failed(&self) -> bool {
        self.newer == NewerEnd::Failed
    }

    /// A page reached the start of the topic: nothing older to ask for.
    pub fn mark_complete(&mut self) {
        self.older = OlderEnd::Complete;
    }

    /// The oldest end was trimmed away: scrolling up pages it again.
    pub fn reopen_oldest(&mut self) {
        if self.older == OlderEnd::Complete {
            self.older = OlderEnd::Open;
        }
    }

    /// The last older page failed (nothing to fail once the start is
    /// reached).
    pub fn fail_older(&mut self) {
        if self.older == OlderEnd::Open {
            self.older = OlderEnd::Failed;
        }
    }

    /// Reopen the older end after a failure (a retry, a landed page or
    /// re-selecting the topic).
    pub fn reopen_older(&mut self) {
        if self.older == OlderEnd::Failed {
            self.older = OlderEnd::Open;
        }
    }

    /// Whether a message past the window's newest end is known: the window
    /// stops short of it (`true`), or reaches the latest (`false`). A
    /// failed newer page stays failed while newer messages remain.
    pub fn newer_exists(&mut self, exists: bool) {
        self.newer = match (exists, self.newer) {
            (true, NewerEnd::Failed) => NewerEnd::Failed,
            (true, _) => NewerEnd::More,
            (false, _) => NewerEnd::AtLatest,
        };
    }

    /// The newer page failed: paging stops until a jump.
    pub fn fail_newer(&mut self) {
        self.newer = NewerEnd::Failed;
    }

    /// A jump asks for newer pages again.
    pub fn retry_newer(&mut self) {
        if self.newer == NewerEnd::Failed {
            self.newer = NewerEnd::More;
        }
    }

    /// Drop a deleted row and remember the id.
    pub(crate) fn remove(&mut self, id: MessageId, permanent: bool) {
        self.messages.remove(&id.0);
        if permanent {
            self.tombstones.insert(id.0);
        }
    }

    pub fn is_tombstone(&self, id: MessageId) -> bool {
        self.tombstones.contains(&id.0)
    }

    pub fn ordered(&self) -> Vec<&HistoryMessage> {
        self.messages.values().collect()
    }

    /// Parity slice 4: live topic messages (incoming updates and own
    /// sends) land here when the topic is loaded. Entries are only ever
    /// created by the `GetTopicHistory` fetch — upserting into a missing
    /// topic would corrupt the paging cursor.
    pub(crate) fn upsert(&mut self, message: HistoryMessage) {
        if self.tombstones.contains(&message.id.0) {
            return;
        }
        self.messages.insert(message.id.0, message);
    }

    /// Parity slice 4: `updateMessageSendSucceeded` / `Failed` replace the
    /// pending row, mirroring `HistoryState::replace_id`.
    pub(crate) fn replace_id(&mut self, old: MessageId, new_message: HistoryMessage) {
        self.messages.remove(&old.0);
        self.upsert(new_message);
    }
}
