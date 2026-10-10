//! Connect driver: members, admins, invites, join requests, event log.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::GroupsPurpose;
use crate::state::{
    AdminListFetch, AdminRightsFetch, CHAT_EVENT_LOG_PAGE_SIZE, ChatEventLogFetch, InviteLinkFetch,
    JoinRequestFetch, MemberListFilter, MemberStatusChange, OwnerLookup, RequestPurpose,
    RequestRollback, SupergroupMembersFetch,
};
use crate::telegram::envelope::{
    CanTransferOwnershipResult, ChatAdminRights, ChatKind, ChatPermissions,
};
use crate::telegram::requests::{
    ChatEventLogFilterSet, MessageSenderRef, add_chat_member, add_chat_members, ban_chat_member,
    can_transfer_ownership, chat_member_status_administrator_json, chat_member_status_banned_json,
    chat_member_status_member_json, chat_member_status_restricted_json, create_chat_invite_link,
    edit_chat_invite_link, get_basic_group_full_info, get_chat_administrators, get_chat_event_log,
    get_chat_invite_links, get_chat_join_requests, get_chat_member, get_chat_owner_after_leaving,
    get_supergroup_members, process_chat_join_request, replace_primary_chat_invite_link,
    revoke_chat_invite_link, set_chat_member_status, set_chat_permissions, set_supergroup_username,
    supergroup_members_filter_administrators_json, supergroup_members_filter_banned_json,
    supergroup_members_filter_recent_json, supergroup_members_filter_restricted_json,
    supergroup_members_filter_search_json, toggle_supergroup_join_by_request,
    transfer_chat_ownership,
};

mod admins;
mod invites;
mod members;

impl<S: JsonSender> ConnectDriver<S> {
    /// `canTransferOwnership`: the 2-step-verification and session-age
    /// gate. Sent when the transfer dialog opens, never on a timer.
    pub fn check_can_transfer_ownership(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.ownership.check_in_flight {
            return Ok(None);
        }
        self.session.begin_ownership_check();
        let extra = self
            .session
            .request(RequestPurpose::CanTransferOwnership, None);
        if let Err(err) = self.sender.send_json(&can_transfer_ownership(extra)) {
            self.session.requests.take(extra);
            self.session.ownership.check_in_flight = false;
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `transferChatOwnership` with the 2-step verification password. Only
    /// the owner may call it, only after `canTransferOwnership` said Ok,
    /// and not to a bot or yourself. The password goes to TDLib and
    /// nowhere else: it is not stored, logged or kept in the purpose.
    pub fn transfer_chat_ownership(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        password: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let allowed = matches!(
            self.session.chats.get(&chat_id.0).map(|c| &c.kind),
            Some(ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. })
        ) && self.session.chat_is_owner(chat_id)
            && self.session.ownership.can_transfer == Some(CanTransferOwnershipResult::Ok)
            && self.session.ownership.transfer_in_flight.is_none()
            && !password.is_empty()
            && self.session.my_user_id != Some(user_id)
            && !self.session.is_bot_user(user_id);
        if !allowed {
            return Ok(None);
        }
        self.session.begin_ownership_transfer(chat_id.0, user_id);
        let extra = self.session.request(
            RequestPurpose::Groups(GroupsPurpose::TransferChatOwnership { user_id }),
            Some(chat_id),
        );
        if let Err(err) = self.sender.send_json(&transfer_chat_ownership(
            extra, chat_id.0, user_id, password,
        )) {
            self.session.requests.take(extra);
            self.session.ownership.transfer_in_flight = None;
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getChatOwnerAfterLeaving`: who inherits the chat if the owner
    /// leaves. Owner-only; one lookup per chat until it is cleared.
    pub fn fetch_chat_owner_after_leaving(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = matches!(
            self.session.chats.get(&chat_id.0).map(|c| &c.kind),
            Some(ChatKind::BasicGroup { .. } | ChatKind::Supergroup { .. })
        );
        if !supported || !self.session.chat_is_owner(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.ownership.owner_after_leaving.get(&chat_id.0),
            Some(OwnerLookup::Loading | OwnerLookup::Loaded(_))
        ) {
            return Ok(None);
        }
        self.session.begin_owner_lookup(chat_id.0);
        let extra = self
            .session
            .request(RequestPurpose::GetChatOwnerAfterLeaving, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_owner_after_leaving(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            self.session
                .ownership
                .owner_after_leaving
                .remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setChatPermissions` (schema 1.8.67, line 13464) —
    /// changes the default chat member permissions. Supported only for
    /// basic groups and supergroups; requires the `can_restrict_members`
    /// administrator right (line 13461). Applied optimistically;
    /// `updateChatPermissions` confirms.
    pub fn set_chat_permissions(
        &mut self,
        chat_id: ChatId,
        permissions: &ChatPermissions,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supported = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(
                chat.kind,
                ChatKind::BasicGroup { .. }
                    | ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        );
        if !supported || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatPermissions;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_chat_permissions(
            extra,
            chat_id.0,
            &permissions.to_json(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: barring a TDLib error the block applies as sent.
        // The pre-request values ride on the pending entry so the error
        // arm can roll back.
        if let Some(chat) = self.session.chats.get_mut(&chat_id.0) {
            let rollback = RequestRollback::ChatPermissions {
                previous: chat.permissions,
                previous_can_send: chat.can_send_basic_messages,
            };
            chat.permissions = Some(*permissions);
            chat.can_send_basic_messages = permissions.can_send_basic_messages;
            if let Some(pending) = self.session.requests.pending_mut(extra) {
                pending.rollback = Some(rollback);
            }
        }
        Ok(Some(extra))
    }

    /// Slice G1: `replacePrimaryChatInviteLink` (schema 1.8.67, line
    /// 14089) — available for basic groups, supergroups, and channels;
    /// requires administrator privileges and the `can_invite_users`
    /// right. The new link arrives as `chatInviteLink`.
    pub fn replace_primary_chat_invite_link(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_invite_users(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ReplacePrimaryChatInviteLink;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&replace_primary_chat_invite_link(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `toggleSupergroupJoinByRequest` (schema 1.8.67, line
    /// 15188) — whether directly joining the supergroup needs admin
    /// approval. Requires the `can_restrict_members` administrator right
    /// (line 15182); not for broadcast groups or channels. Applied
    /// optimistically; `updateSupergroup` confirms.
    pub fn toggle_supergroup_join_by_request(
        &mut self,
        chat_id: ChatId,
        join_by_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupJoinByRequest;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_supergroup_join_by_request(
            extra,
            supergroup_id,
            join_by_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flag rides on the pending entry so the
        // error arm can roll back.
        let previous = self
            .session
            .supergroup_join_by_request
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_join_by_request
            .insert(supergroup_id, join_by_request);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::JoinByRequest {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G1: `setSupergroupUsername` (schema 1.8.67, line 15136) —
    /// changes the editable public username; requires owner privileges
    /// (line 15133). Empty string removes the username. Applied
    /// optimistically; `updateSupergroup` confirms.
    pub fn set_supergroup_username(
        &mut self,
        chat_id: ChatId,
        username: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_is_owner(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupUsername;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&set_supergroup_username(extra, supergroup_id, username))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous username rides on the pending entry so
        // the error arm can roll back.
        let previous = self
            .session
            .supergroup_usernames
            .get(&supergroup_id)
            .cloned();
        self.session
            .set_supergroup_username(supergroup_id, username.to_string());
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SupergroupUsername {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }
}
