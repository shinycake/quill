//! What the group permissions box reads from the session (tdesktop
//! `ShowEditPeerPermissionsBox`): the chat's shape, the viewer's own
//! rights, the slow mode and booster values and the exception counts.
use super::*;
use crate::permissions_box::{AdminGrants, BoxContext, PermissionsSave};
use crate::telegram::envelope::SupergroupMemberCounts;

impl Session {
    pub(crate) fn set_supergroup_member_counts(
        &mut self,
        supergroup_id: i64,
        counts: SupergroupMemberCounts,
    ) {
        if let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id) {
            info.member_counts = counts;
        }
    }

    /// The chat as the box sees it; `None` for chats it does not open on
    /// (channels and private chats).
    pub fn permissions_box_context(&self, chat_id: ChatId) -> Option<BoxContext> {
        let chat = self.chats.get(&chat_id.0)?;
        let supergroup_id = match chat.kind {
            ChatKind::Supergroup {
                supergroup_id,
                is_channel: false,
            } => Some(supergroup_id),
            ChatKind::BasicGroup { .. } => None,
            _ => return None,
        };
        let grants = if self.chat_is_owner(chat_id) {
            AdminGrants::OWNER
        } else {
            AdminGrants {
                change_info: self.chat_can_change_info(chat_id),
                pin_messages: chat.can_pin_messages(),
                invite_users: self.chat_can_invite_users(chat_id),
                manage_topics: self.chat_can_manage_topics(chat_id),
            }
        };
        let is_public = supergroup_id
            .and_then(|id| self.supergroup_username(id))
            .is_some_and(|name| !name.is_empty());
        let is_discussion = supergroup_id
            .and_then(|id| self.supergroup_full_info(id))
            .is_some_and(|info| info.linked_chat_id != 0);
        Some(BoxContext {
            is_forum: chat.is_forum_chat(),
            is_public,
            is_discussion,
            grants,
        })
    }

    /// The supergroup's slow mode delay (0 in a basic group).
    pub fn chat_slow_mode_delay(&self, chat_id: ChatId) -> i32 {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.supergroup_full_info(id))
            .map_or(0, |info| info.slow_mode_delay)
    }

    /// Restricted and banned counts of a supergroup (zeros until known).
    pub fn chat_member_counts(&self, chat_id: ChatId) -> SupergroupMemberCounts {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.supergroup_full_info(id))
            .map(|info| info.member_counts)
            .unwrap_or_default()
    }

    /// tdesktop shows slow mode and the boosters setting to the owner of a
    /// basic group (saving them upgrades it) and in supergroups to anyone
    /// who may restrict members.
    pub fn permissions_box_has_slow_mode(&self, chat_id: ChatId) -> bool {
        match self.chats.get(&chat_id.0).map(|chat| &chat.kind) {
            Some(ChatKind::BasicGroup { .. }) => self.chat_is_owner(chat_id),
            Some(ChatKind::Supergroup {
                is_channel: false, ..
            }) => self.chat_can_restrict_members(chat_id),
            _ => false,
        }
    }

    /// The offline demo has no server to answer a save: take the values
    /// as the server would report them.
    pub fn apply_permissions_save_locally(&mut self, chat_id: ChatId, save: PermissionsSave) {
        if let Some(chat) = self.chats.get_mut(&chat_id.0) {
            chat.permissions = Some(save.permissions);
        }
        let Some(supergroup_id) = self.chat_supergroup(chat_id) else {
            return;
        };
        if let Some(info) = self.groups.supergroup_full_infos.get_mut(&supergroup_id) {
            info.slow_mode_delay = save.slow_mode_delay;
            info.unrestrict_boost_count = save.unrestrict_boost_count;
        }
    }
}
