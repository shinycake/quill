//! `supergroup.boost_level` updates: when a channel's level moves, whatever
//! boost status Quill cached for it (level, boost count, next-level goal)
//! is out of date.

use crate::state::{PendingRequest, RequestPurpose, Session};

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

    /// `chatBoostSlots`: the answer to `getAvailableChatBoostSlots` (kept
    /// for the box that waits for this very request) or to `boostChat`
    /// (the cached status is dropped and the box goes back to its
    /// counters). A box that closed, or waits for another request, has
    /// nothing to take it.
    pub(crate) fn accept_chat_boost_slots(
        &mut self,
        slots: Vec<crate::telegram::envelope::ParsedBoostSlot>,
        pending: Option<&PendingRequest>,
    ) {
        use crate::boost_box::{BoostPhase, BoostWork};
        let Some(pending) = pending else { return };
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let waiting = self
            .groups
            .boost_box
            .as_mut()
            .filter(|state| state.chat_id == chat_id.0 && state.waits_for(pending.id));
        match pending.purpose {
            RequestPurpose::GetBoostSlotsForBoost => {
                if let Some(state) = waiting
                    && matches!(state.phase, BoostPhase::Working(BoostWork::Slots { .. }))
                {
                    state.phase = BoostPhase::Working(BoostWork::SlotsReady(slots));
                }
            }
            // `boostChat` answers `chatBoostSlots` as well (schema 1.8.67,
            // line 13922): drop the cached status so the dialog refetches.
            RequestPurpose::BoostChat => {
                self.groups.chat_boost_status.remove(&chat_id.0);
                self.groups.chat_boost_details.remove(&chat_id.0);
                // A line about boosts that moved here (`lng_boost_reassign_done`).
                let Some(state) = self
                    .groups
                    .boost_box
                    .as_mut()
                    .filter(|state| state.chat_id == chat_id.0 && state.waits_for(pending.id))
                else {
                    return;
                };
                let moved = match std::mem::replace(&mut state.phase, BoostPhase::Main) {
                    BoostPhase::Working(BoostWork::Boosting { reassign, .. }) => reassign,
                    _ => None,
                };
                state.note = moved.map(|moved| {
                    crate::boost_box::reassign_done_text(moved.count, moved.groups, moved.channels)
                });
            }
            _ => {}
        }
    }
}
