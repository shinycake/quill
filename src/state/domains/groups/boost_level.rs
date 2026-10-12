//! `supergroup.boost_level` updates: when a channel's level moves, whatever
//! boost status Quill cached for it (level, boost count, next-level goal)
//! is out of date.

use crate::state::Session;

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
            self.groups.chat_boost_status.remove(&chat.0);
        }
        chats
    }
}
