//! `supergroup.boost_level` updates: when a channel's level moves, whatever
//! boost status Quill cached for it (level, boost count, next-level goal)
//! is out of date.

use crate::state::fetch::Fetch;
use crate::state::{GroupsState, Session};

impl GroupsState {
    /// `(level, boost_count)` of `chat`, the newest answer held (it stays
    /// while a refetch runs or after one fails).
    pub fn boost_level_count(&self, chat: i64) -> Option<(i32, i32)> {
        self.chat_boost_status
            .get(&chat)
            .and_then(|fetch| fetch.data().copied())
    }

    /// The status of `chat`: not asked, in flight, answered or failed.
    pub fn boost_status_fetch(&self, chat: i64) -> Option<&Fetch<(i32, i32)>> {
        self.chat_boost_status.get(&chat)
    }

    /// A boost or a level change made what is held out of date: it stays
    /// on screen until the refetch replaces it. A request in flight asked
    /// before the change, so its answer is dropped.
    pub fn outdate_boost_status(&mut self, chat: i64) {
        if let Some(fetch) = self.chat_boost_status.get_mut(&chat) {
            fetch.outdate();
        }
    }

    /// Record a status answer, as the answer to `request` of `chat`.
    /// `false` when the status no longer waits for that request.
    pub(crate) fn answer_boost_status(
        &mut self,
        chat: i64,
        request: crate::ids::RequestId,
        status: crate::telegram::envelope::ParsedBoostStatus,
    ) -> bool {
        let value = (status.level, status.boost_count);
        let taken = self
            .chat_boost_status
            .get_mut(&chat)
            .is_some_and(|fetch| fetch.answered(Some(request), value));
        if taken {
            self.chat_boost_details.insert(chat, status);
        }
        taken
    }
}

impl Session {
    /// Record the level an `updateSupergroup` carried. A level that differs
    /// from the last one seen queues the supergroup for a status refresh
    /// (see `ConnectDriver::refresh_moved_boost_statuses`); the first level
    /// seen is only remembered.
    pub(crate) fn note_supergroup_boost_level(&mut self, supergroup_id: i64, level: i32) {
        let previous = self
            .groups
            .supergroup_boost_levels
            .insert(supergroup_id, level);
        if previous.is_some_and(|previous| previous != level)
            && !self.groups.boost_level_moved.contains(&supergroup_id)
        {
            self.groups.boost_level_moved.push(supergroup_id);
        }
    }

    /// Drop the cached boost status of every chat whose supergroup's level
    /// moved, and return those chats so the caller can ask again. The
    /// detailed status stays on screen until the answer replaces it.
    pub(crate) fn take_moved_boost_chats(&mut self) -> Vec<crate::ids::ChatId> {
        let moved = std::mem::take(&mut self.groups.boost_level_moved);
        if moved.is_empty() {
            return Vec::new();
        }
        let chats: Vec<crate::ids::ChatId> = self
            .groups
            .chat_boost_status
            .keys()
            .map(|chat| crate::ids::ChatId(*chat))
            .filter(|chat| {
                self.chat_supergroup(*chat)
                    .is_some_and(|supergroup| moved.contains(&supergroup))
            })
            .collect();
        for chat in &chats {
            self.groups.outdate_boost_status(chat.0);
        }
        chats
    }
}
