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

impl Session {
    /// The call that a new video chat would take the place of (tdesktop
    /// `inCall()` / `inGroupCall()`): a one-to-one call that is not ending,
    /// a joined video chat or a scheduled one. A tracked call you have not
    /// joined is only being looked at and counts as none.
    pub fn current_call_for_create(&self) -> crate::calls::group_call_create::CurrentCall {
        use crate::calls::group_call_create::CurrentCall;
        use crate::telegram::envelope::CallState;
        let one_to_one = self.calls.active_call.as_ref().is_some_and(|call| {
            !matches!(
                call.state,
                CallState::HangingUp | CallState::Discarded { .. } | CallState::Error { .. }
            )
        });
        if one_to_one {
            return CurrentCall::OneToOne;
        }
        let Some(call) = self.calls.active_group_call.as_ref() else {
            return CurrentCall::None;
        };
        let scheduled = call.scheduled_start_date > 0;
        if !call.is_joined && !scheduled {
            return CurrentCall::None;
        }
        let chat = self.chats.values().find(|chat| {
            chat.video_chat
                .as_ref()
                .is_some_and(|video_chat| video_chat.group_call_id == call.id)
        });
        CurrentCall::Group {
            chat_id: chat.map(|chat| chat.id),
            scheduled,
            channel: chat.is_some_and(|chat| chat.kind.is_channel()),
        }
    }
}
