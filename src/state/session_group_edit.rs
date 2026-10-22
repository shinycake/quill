//! The group and channel Edit box gates (tdesktop
//! `boxes/peers/edit_peer_info_box.cpp`) and the description it edits.
use super::*;

impl Session {
    /// The viewer's status in a group or channel, wherever TDLib reported it.
    fn group_own_status(&self, chat_id: ChatId) -> Option<ChannelMemberStatus> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .supergroup_own_status(supergroup_id)
                .or(chat.my_member_status),
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_status
                .get(&basic_group_id)
                .copied()
                .or(chat.my_member_status),
            _ => None,
        }
    }

    /// tdesktop `canEditInformation`: the title, description and photo.
    /// Channels need the `can_change_info` admin right; groups also let a
    /// member change them when the default permissions allow it.
    pub fn chat_can_edit_information(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        let by_permission = || {
            chat.permissions.as_ref().is_some_and(|p| p.can_change_info)
                && matches!(
                    self.group_own_status(chat_id),
                    Some(
                        ChannelMemberStatus::Creator
                            | ChannelMemberStatus::Administrator
                            | ChannelMemberStatus::Member
                    )
                )
        };
        match chat.kind {
            ChatKind::Supergroup {
                is_channel: true, ..
            } => self.chat_can_change_info(chat_id),
            ChatKind::Supergroup { .. } | ChatKind::BasicGroup { .. } => {
                self.chat_can_change_info(chat_id) || by_permission()
            }
            _ => false,
        }
    }

    /// tdesktop `EditPeerInfoBox::Available`: whether the chat has an Edit
    /// box for the viewer (the "Manage group" / "Manage channel" entry).
    pub fn group_edit_box_available(&self, chat_id: ChatId) -> bool {
        let Some(chat) = self.chats.get(&chat_id.0) else {
            return false;
        };
        // A channel's direct messages group has no Edit box (`isMonoforum`).
        if self.direct_messages_role(chat_id).is_some() {
            return false;
        }
        match chat.kind {
            ChatKind::BasicGroup { basic_group_id } => {
                self.groups.basic_group_active.get(&basic_group_id) != Some(&false)
                    && (self.chat_can_edit_information(chat_id)
                        || self.chat_can_restrict_members(chat_id))
            }
            ChatKind::Supergroup { .. } => {
                matches!(
                    self.group_own_status(chat_id),
                    Some(ChannelMemberStatus::Creator | ChannelMemberStatus::Administrator)
                ) || self.chat_can_edit_information(chat_id)
                    || self.chat_can_restrict_members(chat_id)
            }
            _ => false,
        }
    }

    /// The group's or channel's description as TDLib last reported it
    /// (`supergroupFullInfo` / `basicGroupFullInfo`); `None` before the
    /// full info arrived.
    pub fn chat_description(&self, chat_id: ChatId) -> Option<&str> {
        let chat = self.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Supergroup { supergroup_id, .. } => self
                .supergroup_full_info(supergroup_id)
                .map(|info| info.description.as_str()),
            ChatKind::BasicGroup { basic_group_id } => self
                .groups
                .basic_group_descriptions
                .get(&basic_group_id)
                .map(String::as_str),
            _ => None,
        }
    }

    /// A refused Edit box request: the box shows it under its field.
    pub(crate) fn record_group_info_edit_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        let Some(pending) = pending else {
            return;
        };
        let field = match pending.purpose {
            RequestPurpose::SetChatTitle => GroupInfoField::Title,
            RequestPurpose::SetChatDescription => GroupInfoField::Description,
            RequestPurpose::SetChatPhoto => GroupInfoField::Photo,
            _ => return,
        };
        if let Some(chat_id) = pending.chat_id {
            self.groups.info_edit_errors.insert(
                chat_id.0,
                GroupInfoEditError {
                    field,
                    reason: super::request_errors::error_reason(err),
                },
            );
        }
    }
}
