//! "Boosts needed to bypass restrictions" (tdesktop
//! `edit_peer_permissions_box.cpp` `AddBoostsUnrestrictSlider`,
//! `SaveBoostsUnrestrict`): the supergroup setting behind
//! `setSupergroupUnrestrictBoostCount`.
use super::*;

/// tdesktop offers 1 to 5 boosts on its slider (`kBoostsUnrestrictValues`).
pub const UNRESTRICT_BOOST_CHOICES: [i32; 5] = [1, 2, 3, 4, 5];

/// TDLib accepts 0 to 8 (`setSupergroupUnrestrictBoostCount`).
pub const UNRESTRICT_BOOST_MAX: i32 = 8;

impl Session {
    /// Whether the viewer can edit the setting: supergroups only (not
    /// channels), with the right to restrict members.
    pub fn chat_can_edit_unrestrict_boosts(&self, chat_id: ChatId) -> bool {
        matches!(
            self.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup {
                is_channel: false,
                ..
            })
        ) && self.chat_can_restrict_members(chat_id)
    }

    /// Boosts a member needs to bypass restrictions: the value being saved,
    /// else the one from the full info (0 = off).
    pub fn chat_unrestrict_boost_count(&self, chat_id: ChatId) -> i32 {
        if let Some(pending) = self.groups.unrestrict_boost_pending.get(&chat_id.0) {
            return *pending;
        }
        self.chat_supergroup(chat_id)
            .and_then(|id| self.groups.supergroup_full_infos.get(&id))
            .map_or(0, |info| info.unrestrict_boost_count)
    }

    /// The acknowledged value becomes the stored one.
    pub(crate) fn apply_boost_settings_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(pending) = pending else {
            return;
        };
        if pending.purpose != RequestPurpose::SetSupergroupUnrestrictBoostCount {
            return;
        }
        let Some(chat_id) = pending.chat_id else {
            return;
        };
        let Some(value) = self.groups.unrestrict_boost_pending.remove(&chat_id.0) else {
            return;
        };
        if let Some(supergroup_id) = self.chat_supergroup(chat_id)
            && let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id)
        {
            info.unrestrict_boost_count = value;
        }
    }
}
