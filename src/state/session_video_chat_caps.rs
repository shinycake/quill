//! Who may start and manage video chats in a chat.
use crate::state::*;

impl Session {
    /// tdesktop `PeerData::canManageGroupCall` (data/data_peer.cpp): the
    /// creator, or an administrator with `can_manage_video_chats`. Basic
    /// groups carry both on `updateBasicGroup`, supergroups and channels on
    /// `updateSupergroup` (kept current on every promotion and demotion).
    /// A channel's direct messages chat never manages one (`isMonoforum`).
    pub fn chat_can_manage_video_chats(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => {
                if self
                    .groups
                    .supergroup_direct_messages
                    .contains_key(&supergroup_id)
                {
                    return false;
                }
                self.supergroup_own_status(supergroup_id) == Some(ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .supergroup_manage_video_chats_right
                        .get(&supergroup_id)
                        .copied()
                        .unwrap_or(false)
            }
            ChatKind::BasicGroup { basic_group_id } => {
                self.groups.basic_group_status.get(&basic_group_id)
                    == Some(&ChannelMemberStatus::Creator)
                    || self
                        .groups
                        .basic_group_manage_video_chats_right
                        .get(&basic_group_id)
                        .copied()
                        .unwrap_or(false)
            }
            _ => false,
        }
    }
}
