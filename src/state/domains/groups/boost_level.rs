//! `supergroup.boost_level` updates: when a channel's level moves, whatever
//! boost status Quill cached for it (level, boost count, next-level goal)
//! is out of date.

use crate::state::fetch::Fetch;
use crate::state::{GroupsState, Session};
use crate::telegram::envelope::ParsedBoostStatus;

impl GroupsState {
    /// The newest `getChatBoostStatus` answer held for `chat`.
    pub fn boost_status(&self, chat: i64) -> Option<&ParsedBoostStatus> {
        self.chat_boost_status.get(&chat).and_then(Fetch::data)
    }

    /// The status of `chat`: not asked, in flight, answered or failed.
    pub fn boost_status_fetch(&self, chat: i64) -> Option<&Fetch<ParsedBoostStatus>> {
        self.chat_boost_status.get(&chat)
    }

    /// The boost level and boost count of `chat`, once it answered.
    pub fn boost_level_and_count(&self, chat: i64) -> Option<(i32, i32)> {
        self.boost_status(chat)
            .map(|status| (status.level, status.boost_count))
    }

    /// The boost level of `chat`, once it answered.
    pub fn boost_level(&self, chat: i64) -> Option<i32> {
        self.boost_status(chat).map(|status| status.level)
    }

    /// Whether the status of `chat` should be asked for now: never asked,
    /// or out of date. A request in flight, an answer and a failure wait.
    pub fn boost_status_wanted(&self, chat: i64) -> bool {
        self.chat_boost_status
            .get(&chat)
            .is_none_or(Fetch::wants_fetch)
    }

    /// The held status of `chat` is out of date (a boost was made, a giveaway
    /// launched): it stays visible and is asked for again. A request in
    /// flight asked before the change, so its answer is dropped.
    pub fn outdate_boost_status(&mut self, chat: i64) {
        self.chat_boost_status.entry(chat).or_default().outdate();
    }

    /// An answer of `getChatBoostStatus` held as if it had arrived.
    pub fn set_boost_status(&mut self, chat: i64, status: ParsedBoostStatus) {
        self.chat_boost_status.insert(chat, Fetch::Ready(status));
    }
}

impl Session {
    /// Record the level an `updateSupergroup` carried. A level that differs
    /// from the last one seen marks the held boost status of the
    /// supergroup's chats out of date (see
    /// `ConnectDriver::refresh_moved_boost_statuses`); the first level seen
    /// is only remembered.
    pub(crate) fn note_supergroup_boost_level(&mut self, supergroup_id: i64, level: i32) {
        let previous = self
            .groups
            .supergroup_boost_levels
            .insert(supergroup_id, level);
        if previous.is_none_or(|previous| previous == level) {
            return;
        }
        let chats: Vec<i64> = self
            .groups
            .chat_boost_status
            .keys()
            .copied()
            .filter(|chat| {
                self.chat_supergroup(crate::ids::ChatId(*chat))
                    .is_some_and(|supergroup| supergroup == supergroup_id)
            })
            .collect();
        for chat in chats {
            if let Some(fetch) = self.groups.chat_boost_status.get_mut(&chat)
                && matches!(fetch, Fetch::Ready(_) | Fetch::Loading { .. })
            {
                fetch.outdate();
            }
        }
    }

    /// The chats whose held boost status is out of date and not asked for
    /// again yet.
    pub(crate) fn outdated_boost_chats(&self) -> Vec<crate::ids::ChatId> {
        self.groups
            .chat_boost_status
            .iter()
            .filter(|(_, fetch)| fetch.is_outdated())
            .map(|(chat, _)| crate::ids::ChatId(*chat))
            .collect()
    }
}
