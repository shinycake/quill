//! Connect driver: group and channel admin toggles (B7) — topics, history
//! for new members, join-to-send, hidden members, protected content,
//! discussion group, allowed reactions and the basic group upgrade.
//!
//! Every method re-checks `Session::group_admin_controls`, so a control
//! the viewer's rights don't allow is never sent; a refused request is
//! `Ok(None)`.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{AdminFollowup, GroupToggle, ProfileChatsKind, RequestPurpose, RequestRollback};
use crate::telegram::envelope::{ChatAvailableReactions, ChatKind, EnvelopePayload};
use crate::telegram::requests::{
    set_chat_available_reactions, set_chat_discussion_group, toggle_chat_has_protected_content,
    toggle_supergroup_has_hidden_members, toggle_supergroup_is_all_history_available,
    toggle_supergroup_is_forum, toggle_supergroup_join_to_send_messages,
    upgrade_basic_group_chat_to_supergroup_chat,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn admin_supergroup(&self, chat_id: ChatId) -> Option<i64> {
        match self.session.chats.get(&chat_id.0)?.kind {
            ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
            _ => None,
        }
    }

    /// Refresh the group's full info (history, hidden members, linked
    /// chat) when the settings dialog opens.
    pub fn refresh_group_admin(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        match self.admin_supergroup(chat_id) {
            Some(supergroup_id) => self.refresh_supergroup_full_info(supergroup_id),
            None => Ok(None),
        }
    }

    /// Send one optimistic supergroup toggle.
    fn send_group_toggle(
        &mut self,
        chat_id: ChatId,
        toggle: GroupToggle,
        value: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(supergroup_id) = self.admin_supergroup(chat_id) else {
            return Ok(None);
        };
        let purpose = match toggle {
            GroupToggle::JoinToSend => RequestPurpose::ToggleSupergroupJoinToSendMessages,
            GroupToggle::HistoryVisible => RequestPurpose::ToggleSupergroupIsAllHistoryAvailable,
            GroupToggle::HiddenMembers => RequestPurpose::ToggleSupergroupHasHiddenMembers,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = match toggle {
            GroupToggle::JoinToSend => {
                toggle_supergroup_join_to_send_messages(extra, supergroup_id, value)
            }
            GroupToggle::HistoryVisible => {
                toggle_supergroup_is_all_history_available(extra, supergroup_id, value)
            }
            GroupToggle::HiddenMembers => {
                toggle_supergroup_has_hidden_members(extra, supergroup_id, value)
            }
        };
        self.send_json_request(extra, &json)?;
        let previous = self.session.set_group_toggle(supergroup_id, toggle, value);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::GroupToggle {
                supergroup_id,
                toggle,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// `toggleSupergroupJoinToSendMessages` for a discussion group.
    pub fn set_group_join_to_send(
        &mut self,
        chat_id: ChatId,
        value: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.group_admin_controls(chat_id).join_to_send {
            return Ok(None);
        }
        self.send_group_toggle(chat_id, GroupToggle::JoinToSend, value)
    }

    /// `toggleSupergroupHasHiddenMembers`.
    pub fn set_group_hidden_members(
        &mut self,
        chat_id: ChatId,
        value: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.group_admin_controls(chat_id).hide_members {
            return Ok(None);
        }
        self.send_group_toggle(chat_id, GroupToggle::HiddenMembers, value)
    }

    /// Chat history for new members. A basic group is upgraded first
    /// (tdesktop `saveHistoryVisibility` migrates), then the toggle runs.
    pub fn set_group_history_visible(
        &mut self,
        chat_id: ChatId,
        visible: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(control) = self.session.group_admin_controls(chat_id).history else {
            return Ok(None);
        };
        if control.needs_upgrade {
            // Basic groups start with hidden history; only "visible" has
            // anything to change.
            if !visible {
                return Ok(None);
            }
            return self.upgrade_with(
                chat_id,
                Some(AdminFollowup::HistoryAfterUpgrade {
                    old: chat_id,
                    visible,
                }),
            );
        }
        self.send_group_toggle(chat_id, GroupToggle::HistoryVisible, visible)
    }

    /// `toggleSupergroupIsForum` (owner only). Turning topics on is
    /// refused while the group is locked (too few members or a channel's
    /// discussion group); a basic group is upgraded first.
    pub fn set_group_topics(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
        tabs: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(control) = self.session.group_admin_controls(chat_id).topics else {
            return Ok(None);
        };
        if enabled && control.locked.is_some() {
            return Ok(None);
        }
        if control.needs_upgrade {
            if !enabled {
                return Ok(None);
            }
            return self.upgrade_with(
                chat_id,
                Some(AdminFollowup::EnableTopicsAfterUpgrade { old: chat_id, tabs }),
            );
        }
        self.send_forum_toggle(chat_id, enabled, tabs)
    }

    fn send_forum_toggle(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
        tabs: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(supergroup_id) = self.admin_supergroup(chat_id) else {
            return Ok(None);
        };
        let purpose = RequestPurpose::ToggleSupergroupIsForum;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &toggle_supergroup_is_forum(extra, supergroup_id, enabled, tabs),
        )
        .map(Some)
    }

    /// `toggleChatHasProtectedContent` (owner only).
    pub fn set_chat_protected_content(
        &mut self,
        chat_id: ChatId,
        value: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.group_admin_controls(chat_id).protected_content {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleChatHasProtectedContent;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &toggle_chat_has_protected_content(extra, chat_id.0, value),
        )?;
        let previous = self.session.chat_has_protected_content(chat_id);
        self.session.set_chat_protected(chat_id.0, value);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::ProtectedContent {
                chat_id: chat_id.0,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// `setChatAvailableReactions` (`can_change_info`). Applied
    /// optimistically.
    pub fn set_chat_reactions(
        &mut self,
        chat_id: ChatId,
        setting: ChatAvailableReactions,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.group_admin_controls(chat_id).reactions {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatAvailableReactions;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &set_chat_available_reactions(extra, chat_id.0, &setting),
        )?;
        let previous = self
            .session
            .chat_available_reactions
            .insert(chat_id.0, setting);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::AvailableReactions {
                chat_id: chat_id.0,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// `getSuitableDiscussionChats` for the link picker; always refetched.
    pub fn fetch_suitable_discussion_chats(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session
            .profile_chat_lists
            .remove(&(ProfileChatsKind::SuitableDiscussionChats, 0));
        self.fetch_profile_chats(ProfileChatsKind::SuitableDiscussionChats, 0)
    }

    /// Link `group` as the discussion group of `channel`. TDLib needs the
    /// group's history visible first, so unless the group is known to be
    /// visible the toggle goes out first and the link follows it.
    pub fn link_discussion_group(
        &mut self,
        channel: ChatId,
        group: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let allowed = matches!(
            self.session.group_admin_controls(channel).discussion,
            Some(crate::state::DiscussionControl::Channel { .. })
        );
        if !allowed || channel == group {
            return Ok(None);
        }
        let Some(group_supergroup) = self.admin_supergroup(group) else {
            return Ok(None);
        };
        let known_visible = self
            .session
            .supergroup_full_info(group_supergroup)
            .is_some_and(|info| info.admin.is_all_history_available);
        if known_visible {
            return self.send_discussion_link(channel, group);
        }
        let purpose = RequestPurpose::ToggleSupergroupIsAllHistoryAvailable;
        if self.session.requests.has_purpose_for_chat(purpose, group) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(group));
        self.send_json_request(
            extra,
            &toggle_supergroup_is_all_history_available(extra, group_supergroup, true),
        )?;
        self.session
            .queue_admin_followup(extra, AdminFollowup::LinkAfterHistory { channel, group });
        Ok(Some(extra))
    }

    fn send_discussion_link(
        &mut self,
        channel: ChatId,
        group: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let purpose = RequestPurpose::SetChatDiscussionGroup;
        if self.session.requests.has_purpose_for_chat(purpose, channel) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(channel));
        self.send_json_request(extra, &set_chat_discussion_group(extra, channel.0, group.0))
            .map(Some)
    }

    /// Unlink the discussion group: from the channel (`discussion_chat_id`
    /// 0) or, from the group side, with `chat_id` 0.
    pub fn unlink_discussion_group(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let json_ids = match self.session.group_admin_controls(chat_id).discussion {
            Some(crate::state::DiscussionControl::Channel { linked: Some(_) }) => (chat_id.0, 0),
            Some(crate::state::DiscussionControl::Group { .. }) => (0, chat_id.0),
            _ => return Ok(None),
        };
        let purpose = RequestPurpose::SetChatDiscussionGroup;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &set_chat_discussion_group(extra, json_ids.0, json_ids.1),
        )
        .map(Some)
    }

    /// `upgradeBasicGroupChatToSupergroupChat` (owner of a basic group).
    pub fn upgrade_basic_group(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.upgrade_with(chat_id, None)
    }

    fn upgrade_with(
        &mut self,
        chat_id: ChatId,
        followup: Option<AdminFollowup>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::BasicGroup { .. })
        ) || !self.session.chat_is_owner(chat_id)
        {
            return Ok(None);
        }
        let purpose = RequestPurpose::UpgradeBasicGroup;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &upgrade_basic_group_chat_to_supergroup_chat(extra, chat_id.0),
        )?;
        if let Some(followup) = followup {
            self.session.queue_admin_followup(extra, followup);
        }
        Ok(Some(extra))
    }

    /// Before `apply` takes the pending request: the step waiting for this
    /// answer, and whether the answer was a success. Always removes the
    /// step, so a failed request never leaves one behind.
    pub(crate) fn capture_admin_followup(
        &mut self,
        owned: &crate::telegram::client::OwnedEnvelope,
    ) -> Option<(AdminFollowup, bool)> {
        let extra = owned.envelope.extra?;
        let followup = self.session.take_admin_followup(extra)?;
        let ok = !matches!(owned.envelope.payload, EnvelopePayload::Error(_));
        Some((followup, ok))
    }

    /// After `apply`: run the step that was waiting. Steps whose chat is
    /// gone or no longer allowed leave a note instead of failing silently.
    pub(crate) fn run_admin_followup(&mut self, followup: AdminFollowup, ok: bool) {
        let linking = matches!(followup, AdminFollowup::LinkAfterHistory { .. });
        let result = match followup {
            // The history toggle may be a no-op or lack the right; the
            // link request reports the real error either way.
            AdminFollowup::LinkAfterHistory { channel, group } => {
                self.send_discussion_link(channel, group)
            }
            _ if !ok => return,
            AdminFollowup::EnableTopicsAfterUpgrade { old, tabs } => {
                match self.session.upgraded_chat_for(old) {
                    Some(new) if self.session.chat_is_owner(new) => {
                        self.send_forum_toggle(new, true, tabs)
                    }
                    _ => Ok(None),
                }
            }
            AdminFollowup::HistoryAfterUpgrade { old, visible } => {
                match self.session.upgraded_chat_for(old) {
                    Some(new) if self.session.chat_is_owner(new) => {
                        self.send_group_toggle(new, GroupToggle::HistoryVisible, visible)
                    }
                    _ => Ok(None),
                }
            }
        };
        if !matches!(result, Ok(Some(_))) {
            self.session.chat_action_error = Some(if linking {
                "could not link the discussion group".to_string()
            } else {
                "the group was upgraded, but the setting could not be applied; \
                 open the group settings to retry"
                    .to_string()
            });
        }
    }
}
