//! Connect driver: other admins' invite links, one link's pending join
//! requests, the boosts list and boost link, and supergroup usernames.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::GroupsPurpose;
use crate::state::{
    AdminLinksState, BOOSTS_PAGE_SIZE, BoostsListState, INVITE_ADMIN_PAGE_SIZE, InviteLinkFetch,
    LinkRequestsState, RequestPurpose,
};
use crate::telegram::requests::{
    delete_all_revoked_chat_invite_links, get_chat_boost_link, get_chat_boosts,
    get_chat_invite_links, get_chat_join_requests_page, process_chat_join_requests,
    reorder_supergroup_active_usernames, toggle_supergroup_username_is_active,
};

/// Links per page for another admin's list.
const ADMIN_LINKS_LIMIT: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    fn links_boosts_send(
        &mut self,
        extra: RequestId,
        payload: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.send_json_request(extra, payload).map(Some)
    }

    /// Open another admin's links (owner only): fetches their active and
    /// revoked lists with `getChatInviteLinks(creator_user_id)`.
    pub fn open_admin_invite_links(
        &mut self,
        chat_id: ChatId,
        creator_user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_invite_users(chat_id)
            || !self.session.chat_is_owner(chat_id)
            || Some(creator_user_id) == self.session.my_user_id
            || creator_user_id == 0
        {
            return Ok(None);
        }
        let active = self.session.request(
            RequestPurpose::GetAdminChatInviteLinks { revoked: false },
            Some(chat_id),
        );
        let revoked = self.session.request(
            RequestPurpose::GetAdminChatInviteLinks { revoked: true },
            Some(chat_id),
        );
        self.session.admin_invite_links.insert(
            chat_id.0,
            AdminLinksState {
                creator_user_id,
                active: InviteLinkFetch::Loading,
                revoked: InviteLinkFetch::Loading,
                active_request: Some(active),
                revoked_request: Some(revoked),
            },
        );
        let first = get_chat_invite_links(
            active,
            chat_id.0,
            creator_user_id,
            false,
            0,
            "",
            ADMIN_LINKS_LIMIT,
        );
        let second = get_chat_invite_links(
            revoked,
            chat_id.0,
            creator_user_id,
            true,
            0,
            "",
            ADMIN_LINKS_LIMIT,
        );
        let sent = self.links_boosts_send(active, &first);
        if sent.is_err() {
            self.session.requests.take(revoked);
            self.session.admin_invite_links.remove(&chat_id.0);
            return sent;
        }
        if let Err(err) = self.links_boosts_send(revoked, &second) {
            self.session.admin_invite_links.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(active))
    }

    /// Back from another admin's links.
    pub fn close_admin_invite_links(&mut self, chat_id: ChatId) {
        self.session.admin_invite_links.remove(&chat_id.0);
    }

    /// `deleteAllRevokedChatInviteLinks` for the admin that is open.
    pub fn delete_all_revoked_admin_links(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(creator) = self
            .session
            .admin_invite_links
            .get(&chat_id.0)
            .map(|state| state.creator_user_id)
        else {
            return Ok(None);
        };
        if !self.session.chat_can_invite_users(chat_id) || !self.session.chat_is_owner(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::DeleteAllRevokedChatInviteLinks;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.user_id = Some(creator);
        }
        let payload = delete_all_revoked_chat_invite_links(extra, chat_id.0, creator);
        self.links_boosts_send(extra, &payload)
    }

    /// First page of one link's pending requests
    /// (`getChatJoinRequests` with `invite_link`).
    pub fn open_link_join_requests(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.request_link_join_requests(chat_id, invite_link, false, None)
    }

    /// Next page of the open link's requests, appended.
    pub fn load_more_link_join_requests(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(state) = self.session.link_join_requests.get(&chat_id.0) else {
            return Ok(None);
        };
        if state.loading || state.requests.len() as i32 >= state.total_count {
            return Ok(None);
        }
        let Some(last) = state.requests.last() else {
            return Ok(None);
        };
        let offset = (last.user_id, last.date);
        let link = state.invite_link.clone();
        self.request_link_join_requests(chat_id, &link, true, Some(offset))
    }

    fn request_link_join_requests(
        &mut self,
        chat_id: ChatId,
        invite_link: &str,
        append: bool,
        offset: Option<(i64, i32)>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_invite_users(chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::GetLinkJoinRequests { append },
            Some(chat_id),
        );
        match self.session.link_join_requests.get_mut(&chat_id.0) {
            Some(state) if append => {
                state.loading = true;
                state.error = None;
                state.request = Some(extra);
            }
            _ => {
                self.session.link_join_requests.insert(
                    chat_id.0,
                    LinkRequestsState {
                        invite_link: invite_link.to_owned(),
                        total_count: 0,
                        requests: Vec::new(),
                        loading: true,
                        error: None,
                        request: Some(extra),
                    },
                );
            }
        }
        let payload = get_chat_join_requests_page(
            extra,
            chat_id.0,
            invite_link,
            "",
            offset,
            INVITE_ADMIN_PAGE_SIZE,
        );
        let sent = self.links_boosts_send(extra, &payload);
        if sent.is_err() {
            self.session.link_join_requests.remove(&chat_id.0);
        }
        sent
    }

    /// Drop the open link's request list.
    pub fn close_link_join_requests(&mut self, chat_id: ChatId) {
        self.session.link_join_requests.remove(&chat_id.0);
    }

    /// `processChatJoinRequests` for the open link only.
    pub fn process_link_join_requests(
        &mut self,
        chat_id: ChatId,
        approve: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_invite_users(chat_id) {
            return Ok(None);
        }
        let Some(link) = self
            .session
            .link_join_requests
            .get(&chat_id.0)
            .map(|state| state.invite_link.clone())
        else {
            return Ok(None);
        };
        if [true, false].into_iter().any(|other| {
            self.session.requests.has_purpose_for_chat(
                RequestPurpose::Groups(GroupsPurpose::ProcessLinkJoinRequests { approve: other }),
                chat_id,
            )
        }) {
            return Ok(None);
        }
        let extra = self.session.request(
            RequestPurpose::Groups(GroupsPurpose::ProcessLinkJoinRequests { approve }),
            Some(chat_id),
        );
        let payload = process_chat_join_requests(extra, chat_id.0, &link, approve);
        self.links_boosts_send(extra, &payload)
    }

    /// Open the boosts list on a tab (`only_gifts` = the "Gifts" tab):
    /// resets the list and loads the first page. Admins only.
    pub fn open_chat_boosts(
        &mut self,
        chat_id: ChatId,
        only_gifts: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.request_chat_boosts(chat_id, only_gifts, "", false)
    }

    /// Next page of the boosts list, appended.
    pub fn load_more_chat_boosts(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let Some(state) = self.session.chat_boost_lists.get(&chat_id.0) else {
            return Ok(None);
        };
        if state.loading || state.next_offset.is_empty() {
            return Ok(None);
        }
        let (only_gifts, offset) = (state.only_gifts, state.next_offset.clone());
        self.request_chat_boosts(chat_id, only_gifts, &offset, true)
    }

    fn request_chat_boosts(
        &mut self,
        chat_id: ChatId,
        only_gifts: bool,
        offset: &str,
        append: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_view_boosts(chat_id) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoosts { append }, Some(chat_id));
        match self.session.chat_boost_lists.get_mut(&chat_id.0) {
            Some(state) if append => {
                state.loading = true;
                state.error = None;
                state.request = Some(extra);
            }
            _ => {
                self.session.chat_boost_lists.insert(
                    chat_id.0,
                    BoostsListState {
                        only_gifts,
                        total_count: 0,
                        boosts: Vec::new(),
                        next_offset: String::new(),
                        loading: true,
                        error: None,
                        request: Some(extra),
                    },
                );
            }
        }
        let payload = get_chat_boosts(extra, chat_id.0, only_gifts, offset, BOOSTS_PAGE_SIZE);
        let sent = self.links_boosts_send(extra, &payload);
        if sent.is_err() {
            self.session.chat_boost_lists.remove(&chat_id.0);
        }
        sent
    }

    /// `getChatBoostLink`, once per chat (the answer is cached).
    pub fn fetch_chat_boost_link(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_view_boosts(chat_id)
            || self.session.chat_boost_links.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostLink, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostLink, Some(chat_id));
        let payload = get_chat_boost_link(extra, chat_id.0);
        self.links_boosts_send(extra, &payload)
    }

    fn owned_supergroup_usernames(&self, chat_id: ChatId) -> Option<(i64, Vec<String>, String)> {
        if !self.session.chat_is_owner(chat_id) {
            return None;
        }
        let supergroup_id = self.session.chat_supergroup(chat_id)?;
        let lists = self.session.supergroup_username_lists.get(&supergroup_id)?;
        Some((supergroup_id, lists.active.clone(), lists.editable.clone()))
    }

    /// `toggleSupergroupUsernameIsActive` for one username. The editable
    /// username can't be turned off; only the owner may change these.
    pub fn toggle_group_username(
        &mut self,
        chat_id: ChatId,
        username: &str,
        is_active: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some((supergroup_id, _, editable)) = self.owned_supergroup_usernames(chat_id) else {
            return Ok(None);
        };
        if !is_active && username == editable {
            return Ok(None);
        }
        let busy = [
            RequestPurpose::ToggleSupergroupUsername,
            RequestPurpose::ReorderSupergroupUsernames,
        ]
        .into_iter()
        .any(|purpose| self.session.requests.has_purpose_for_chat(purpose, chat_id));
        if busy {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleSupergroupUsername, Some(chat_id));
        let payload =
            toggle_supergroup_username_is_active(extra, supergroup_id, username, is_active);
        self.links_boosts_send(extra, &payload)
    }

    /// `reorderSupergroupActiveUsernames`: move one active username a slot
    /// up or down and send the whole new order.
    pub fn move_group_username(
        &mut self,
        chat_id: ChatId,
        username: &str,
        up: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some((supergroup_id, mut order, _)) = self.owned_supergroup_usernames(chat_id) else {
            return Ok(None);
        };
        let Some(pos) = order.iter().position(|name| name == username) else {
            return Ok(None);
        };
        let target = if up {
            pos.checked_sub(1)
        } else {
            pos.checked_add(1).filter(|&i| i < order.len())
        };
        let Some(target) = target else {
            return Ok(None);
        };
        let busy = [
            RequestPurpose::ToggleSupergroupUsername,
            RequestPurpose::ReorderSupergroupUsernames,
        ]
        .into_iter()
        .any(|purpose| self.session.requests.has_purpose_for_chat(purpose, chat_id));
        if busy {
            return Ok(None);
        }
        order.swap(pos, target);
        let extra = self
            .session
            .request(RequestPurpose::ReorderSupergroupUsernames, Some(chat_id));
        let payload = reorder_supergroup_active_usernames(extra, supergroup_id, &order);
        self.links_boosts_send(extra, &payload)
    }
}
