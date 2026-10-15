//! Who may start and manage video chats in a chat.
use crate::state::*;

impl Session {
    /// tdesktop `PeerData::canManageGroupCall`: whether the viewer may
    /// start and manage a video chat or live stream here. Channels and
    /// supergroups carry it on the own-member probe, basic groups on
    /// `updateBasicGroup`.
    pub fn chat_can_manage_video_chats(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        match chat.kind {
            ChatKind::Supergroup { .. } => chat.can_manage_video_chats(),
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
