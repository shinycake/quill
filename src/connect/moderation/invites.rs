//! Connect driver: chat invite links and join requests.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase D3a: `getChatInviteLinks` (TDLib 1.8.67, `schema/td_api.tl:14138`).
    /// Lists active invite links from all creators; only admins with the
    /// `can_invite_users` right may call it (TDLib errors otherwise).
    /// Idempotent: a cached `Loaded` result is kept until an explicit
    /// refresh clears it, and no second request goes out while one is in
    /// flight. Returns `Ok(None)` when nothing was sent.
    pub fn fetch_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if matches!(
            self.session.groups.invite_links.get(&chat_id.0),
            Some(InviteLinkFetch::Loading | InviteLinkFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatInviteLinks, chat_id)
        {
            return Ok(None);
        }
        self.session
            .groups
            .invite_links
            .insert(chat_id.0, InviteLinkFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatInviteLinks, Some(chat_id));
        // `creator_user_id` must be an administrator — yourself unless you
        // own the chat (schema 1.8.67, line 14133); 0 is rejected (400).
        let creator = self.session.my_user_id.unwrap_or(0);
        if let Err(err) = self.sender.send_json(&get_chat_invite_links(
            extra, chat_id.0, creator, false, 0, "", 100,
        )) {
            self.session.requests.take(extra);
            self.session.groups.invite_links.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: explicit refresh of `getChatInviteLinks` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.groups.invite_links.remove(&chat_id.0);
        self.fetch_chat_invite_links(chat_id)
    }

    /// Phase D3a: `createChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14097`). Returns `Ok(None)` when the chat cannot
    /// be managed or another create request is already in flight. The new
    /// link arrives as the `chatInviteLink` response.
    pub fn create_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        name: &str,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::CreateChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateChatInviteLink, Some(chat_id));
        if let Err(err) = self.sender.send_json(&create_chat_invite_link(
            extra,
            chat_id.0,
            name,
            expiration_date,
            member_limit,
            creates_join_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `editChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14115`). Returns `Ok(None)` when the chat cannot
    /// be managed or another edit request is already in flight. The
    /// updated link arrives as the `chatInviteLink` response.
    pub fn edit_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        name: &str,
        expiration_date: i32,
        member_limit: i32,
        creates_join_request: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::EditChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::EditChatInviteLink, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_chat_invite_link(
            extra,
            chat_id.0,
            invite_link,
            name,
            expiration_date,
            member_limit,
            creates_join_request,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `revokeChatInviteLink` (TDLib 1.8.67,
    /// `schema/td_api.tl:14152`). Revocation is the only delete path (no
    /// `deleteChatInviteLink` in 1.8.67). Returns `Ok(None)` when the chat
    /// cannot be managed or another revoke request is already in flight.
    /// The updated list arrives as the `chatInviteLinks` response.
    pub fn revoke_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::RevokeChatInviteLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::RevokeChatInviteLink, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&revoke_chat_invite_link(extra, chat_id.0, invite_link))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: `getChatJoinRequests` (TDLib 1.8.67,
    /// `schema/td_api.tl:14174`). Lists pending requests across all invite
    /// links, unfiltered. Idempotent like `fetch_chat_invite_links`.
    /// Returns `Ok(None)` when nothing was sent.
    pub fn fetch_chat_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        if matches!(
            self.session.groups.join_requests.get(&chat_id.0),
            Some(JoinRequestFetch::Loading | JoinRequestFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatJoinRequests, chat_id)
        {
            return Ok(None);
        }
        self.session
            .groups
            .join_requests
            .insert(chat_id.0, JoinRequestFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatJoinRequests, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_join_requests(extra, chat_id.0, "", "", 50))
        {
            self.session.requests.take(extra);
            self.session.groups.join_requests.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3a: explicit refresh of `getChatJoinRequests` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.groups.join_requests.remove(&chat_id.0);
        self.fetch_chat_join_requests(chat_id)
    }

    /// Phase D3a: `processChatJoinRequest` (TDLib 1.8.67,
    /// `schema/td_api.tl:14177`). Approves or declines one pending
    /// request. Duplicate submissions for the same user and chat are
    /// suppressed while a request is in flight; the `ok` response drops
    /// the request from the cached list.
    pub fn process_chat_join_request(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_invite = self.session.chat_can_invite_users(chat_id);
        if !can_invite {
            return Ok(None);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::ProcessChatJoinRequest { user_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&process_chat_join_request(
            extra, chat_id.0, user_id, approve,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
