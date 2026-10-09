//! Connect driver: join-request search/paging/bulk, invite-link members,
//! counts, revoked-link deletion and Stars subscription links (batch B8).
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{
    INVITE_ADMIN_PAGE_SIZE, InviteLinkCountsFetch, InviteLinkFetch, InviteLinkMembersState,
    JoinRequestFetch, RequestPurpose,
};
use crate::telegram::requests::{
    create_chat_subscription_invite_link, delete_all_revoked_chat_invite_links,
    delete_revoked_chat_invite_link, edit_chat_subscription_invite_link,
    get_chat_invite_link_counts, get_chat_invite_link_members, get_chat_invite_links,
    get_chat_join_requests_page, process_chat_join_requests,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn invite_admin_ready(&self, chat_id: ChatId) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(self.session.chat_can_invite_users(chat_id))
    }

    /// Send `payload` for an already-registered request, unregistering it
    /// when the send fails.
    fn send_invite_admin(
        &mut self,
        extra: RequestId,
        payload: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if let Err(err) = self.sender.send_json(payload) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getChatJoinRequests` with a search `query` (empty = all); replaces
    /// the cached list. A newer search supersedes any in flight.
    pub fn search_chat_join_requests(
        &mut self,
        chat_id: ChatId,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        let query = query.trim().to_owned();
        let extra = self
            .session
            .request(RequestPurpose::GetChatJoinRequests, Some(chat_id));
        self.session.join_request_latest.insert(chat_id.0, extra);
        self.session
            .join_request_queries
            .insert(chat_id.0, query.clone());
        self.session
            .join_requests
            .insert(chat_id.0, JoinRequestFetch::Loading);
        let payload =
            get_chat_join_requests_page(extra, chat_id.0, "", &query, None, INVITE_ADMIN_PAGE_SIZE);
        let sent = self.send_invite_admin(extra, &payload);
        if sent.is_err() {
            self.session.join_request_latest.remove(&chat_id.0);
            self.session.join_requests.remove(&chat_id.0);
        }
        sent
    }

    /// Next page of the (possibly searched) join-request list, appended.
    pub fn load_more_chat_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        if self.session.join_request_latest.contains_key(&chat_id.0) {
            return Ok(None);
        }
        let Some(JoinRequestFetch::Loaded(list)) = self.session.join_requests.get(&chat_id.0)
        else {
            return Ok(None);
        };
        let Some(last) = list.requests.last() else {
            return Ok(None);
        };
        if list.requests.len() as i32 >= list.total_count {
            return Ok(None);
        }
        let offset = (last.user_id, last.date);
        let query = self
            .session
            .join_request_queries
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let extra = self
            .session
            .request(RequestPurpose::GetMoreChatJoinRequests, Some(chat_id));
        self.session.join_request_latest.insert(chat_id.0, extra);
        let payload = get_chat_join_requests_page(
            extra,
            chat_id.0,
            "",
            &query,
            Some(offset),
            INVITE_ADMIN_PAGE_SIZE,
        );
        let sent = self.send_invite_admin(extra, &payload);
        if sent.is_err() {
            self.session.join_request_latest.remove(&chat_id.0);
        }
        sent
    }

    /// `processChatJoinRequests`: approve or dismiss every pending request.
    pub fn process_all_chat_join_requests(
        &mut self,
        chat_id: ChatId,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        let purpose = RequestPurpose::ProcessAllChatJoinRequests { approve };
        // One bulk action at a time, whichever direction.
        if [true, false].into_iter().any(|other| {
            self.session.requests.has_purpose_for_chat(
                RequestPurpose::ProcessAllChatJoinRequests { approve: other },
                chat_id,
            )
        }) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let payload = process_chat_join_requests(extra, chat_id.0, "", approve);
        self.send_invite_admin(extra, &payload)
    }

    /// `getChatInviteLinks` with `is_revoked = true` for the viewer's own
    /// links. Idempotent like the active-list fetch.
    pub fn fetch_revoked_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        if matches!(
            self.session.revoked_invite_links.get(&chat_id.0),
            Some(InviteLinkFetch::Loading | InviteLinkFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetRevokedChatInviteLinks, Some(chat_id));
        self.session
            .revoked_invite_links
            .insert(chat_id.0, InviteLinkFetch::Loading);
        let creator = self.session.my_user_id.unwrap_or(0);
        let payload = get_chat_invite_links(extra, chat_id.0, creator, true, 0, "", 100);
        let sent = self.send_invite_admin(extra, &payload);
        if sent.is_err() {
            self.session.revoked_invite_links.remove(&chat_id.0);
        }
        sent
    }

    /// `getChatInviteLinkCounts`: owner only (TDLib rejects admins).
    pub fn fetch_chat_invite_link_counts(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? || !self.session.chat_is_owner(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.invite_link_counts.get(&chat_id.0),
            Some(InviteLinkCountsFetch::Loading | InviteLinkCountsFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatInviteLinkCounts, Some(chat_id));
        self.session
            .invite_link_counts
            .insert(chat_id.0, InviteLinkCountsFetch::Loading);
        let sent = self.send_invite_admin(extra, &get_chat_invite_link_counts(extra, chat_id.0));
        if sent.is_err() {
            self.session.invite_link_counts.remove(&chat_id.0);
        }
        sent
    }

    /// Open one link's member list: resets the cached state and loads the
    /// first page of `getChatInviteLinkMembers`.
    pub fn open_chat_invite_link_members(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        self.request_invite_link_members(chat_id, invite_link, false, None)
    }

    /// Next page of the open link's members, appended.
    pub fn load_more_chat_invite_link_members(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        let Some(state) = self.session.invite_link_members.get(&chat_id.0) else {
            return Ok(None);
        };
        if state.loading || state.members.len() as i32 >= state.total_count {
            return Ok(None);
        }
        let Some(last) = state.members.last() else {
            return Ok(None);
        };
        let offset = (last.user_id, last.joined_chat_date);
        let link = state.invite_link.clone();
        self.request_invite_link_members(chat_id, &link, true, Some(offset))
    }

    fn request_invite_link_members(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        append: bool,
        offset: Option<(i64, i32)>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let extra = self.session.request(
            RequestPurpose::GetChatInviteLinkMembers { append },
            Some(chat_id),
        );
        let fresh = InviteLinkMembersState {
            invite_link: invite_link.to_owned(),
            total_count: 0,
            members: Vec::new(),
            loading: true,
            error: None,
            request: Some(extra),
        };
        match self.session.invite_link_members.get_mut(&chat_id.0) {
            Some(state) if append => {
                state.loading = true;
                state.error = None;
                state.request = Some(extra);
            }
            _ => {
                self.session.invite_link_members.insert(chat_id.0, fresh);
            }
        }
        let payload = get_chat_invite_link_members(
            extra,
            chat_id.0,
            invite_link,
            false,
            offset,
            INVITE_ADMIN_PAGE_SIZE,
        );
        let sent = self.send_invite_admin(extra, &payload);
        if sent.is_err() {
            self.session.invite_link_members.remove(&chat_id.0);
        }
        sent
    }

    /// Drop the open link's member list (details closed).
    pub fn close_chat_invite_link_members(&mut self, chat_id: ChatId) {
        self.session.invite_link_members.remove(&chat_id.0);
    }

    /// `deleteRevokedChatInviteLink`; the link leaves the revoked list on
    /// the `ok` answer.
    pub fn delete_revoked_chat_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteRevokedChatInviteLink, Some(chat_id));
        self.session
            .revoked_link_deletions
            .insert(extra, (chat_id.0, invite_link.to_owned()));
        let sent = self.send_invite_admin(
            extra,
            &delete_revoked_chat_invite_link(extra, chat_id.0, invite_link),
        );
        if sent.is_err() {
            self.session.revoked_link_deletions.remove(&extra);
        }
        sent
    }

    /// `deleteAllRevokedChatInviteLinks` for the viewer's own links.
    pub fn delete_all_revoked_chat_invite_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteAllRevokedChatInviteLinks;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let creator = self.session.my_user_id.unwrap_or(0);
        self.send_invite_admin(
            extra,
            &delete_all_revoked_chat_invite_links(extra, chat_id.0, creator),
        )
    }

    /// Whether the chat is a channel the viewer may create Stars
    /// subscription links in (`createChatSubscriptionInviteLink` is for
    /// channels only).
    pub fn chat_supports_subscription_links(&self, chat_id: ChatId) -> bool {
        self.session.chat_can_invite_users(chat_id)
            && self
                .session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.kind.is_channel())
    }

    /// `createChatSubscriptionInviteLink`: 30-day period, `star_count`
    /// Stars. The new link arrives as the `chatInviteLink` answer.
    pub fn create_chat_subscription_invite_link(
        &mut self,
        chat_id: ChatId,
        name: &str,
        star_count: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)?
            || !self.chat_supports_subscription_links(chat_id)
            || star_count <= 0
        {
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
        let payload = create_chat_subscription_invite_link(extra, chat_id.0, name, star_count);
        self.send_invite_admin(extra, &payload)
    }

    /// `editChatSubscriptionInviteLink`: only the name changes.
    pub fn edit_chat_subscription_invite_link(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        name: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.invite_admin_ready(chat_id)? {
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
        let payload = edit_chat_subscription_invite_link(extra, chat_id.0, invite_link, name);
        self.send_invite_admin(extra, &payload)
    }
}
