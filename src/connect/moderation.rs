//! Connect driver: members, admins, invites, join requests, event log.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{
    AdminListFetch, AdminRightsFetch, CHAT_EVENT_LOG_PAGE_SIZE, ChatEventLogFetch, InviteLinkFetch,
    JoinRequestFetch, MemberListFilter, MemberStatusChange, RequestPurpose, RequestRollback,
    SupergroupMembersFetch,
};
use crate::telegram::envelope::{ChatAdminRights, ChatKind, ChatPermissions};
use crate::telegram::requests::{
    ChatEventLogFilterSet, MessageSenderRef, add_chat_member, add_chat_members,
    chat_member_status_administrator_json, chat_member_status_banned_json,
    chat_member_status_member_json, chat_member_status_restricted_json, create_chat_invite_link,
    edit_chat_invite_link, get_basic_group_full_info, get_chat_administrators, get_chat_event_log,
    get_chat_invite_links, get_chat_join_requests, get_chat_member, get_supergroup_members,
    process_chat_join_request, replace_primary_chat_invite_link, revoke_chat_invite_link,
    set_chat_member_status, set_chat_permissions, set_supergroup_username,
    supergroup_members_filter_administrators_json, supergroup_members_filter_banned_json,
    supergroup_members_filter_recent_json, supergroup_members_filter_restricted_json,
    supergroup_members_filter_search_json, toggle_supergroup_join_by_request,
};

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
            self.session.invite_links.get(&chat_id.0),
            Some(InviteLinkFetch::Loading | InviteLinkFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatInviteLinks, chat_id)
        {
            return Ok(None);
        }
        self.session
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
            self.session.invite_links.remove(&chat_id.0);
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
        self.session.invite_links.remove(&chat_id.0);
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
            self.session.join_requests.get(&chat_id.0),
            Some(JoinRequestFetch::Loading | JoinRequestFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatJoinRequests, chat_id)
        {
            return Ok(None);
        }
        self.session
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
            self.session.join_requests.remove(&chat_id.0);
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
        self.session.join_requests.remove(&chat_id.0);
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
        let purpose = RequestPurpose::ProcessChatJoinRequest { user_id };
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

    /// Phase D3b: `getChatAdministrators` (TDLib 1.8.67,
    /// `schema/td_api.tl:13632`). Only the owner or admins with
    /// `can_promote_members` may call it (TDLib errors otherwise).
    /// Idempotent: a cached `Loaded` result is kept until an explicit
    /// refresh or a membership change clears it, and no second request
    /// goes out while one is in flight. Returns `Ok(None)` when nothing
    /// was sent.
    pub fn fetch_chat_administrators(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.admin_lists.get(&chat_id.0),
            Some(AdminListFetch::Loading | AdminListFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatAdministrators, chat_id)
        {
            return Ok(None);
        }
        self.session
            .admin_lists
            .insert(chat_id.0, AdminListFetch::Loading);
        let extra = self
            .session
            .request(RequestPurpose::GetChatAdministrators, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_administrators(extra, chat_id.0))
        {
            self.session.requests.take(extra);
            self.session.admin_lists.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: explicit refresh of `getChatAdministrators` — clears the
    /// cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_administrators(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.admin_lists.remove(&chat_id.0);
        self.fetch_chat_administrators(chat_id)
    }

    /// Phase D3c: `getChatEventLog` first page (TDLib 1.8.67,
    /// `schema/td_api.tl:15252`). Idempotent: a cached `Loaded` page is
    /// kept until an explicit refresh clears it, and no second request
    /// goes out while one is in flight. The driver no-ops unless the
    /// viewer may view the log (`chat_can_view_event_log` — TDLib
    /// "requires administrator rights"). Returns `Ok(None)` when nothing
    /// was sent.
    pub fn fetch_chat_event_log(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_chat_event_log_page(chat_id, 0)
    }

    /// Phase D3c: explicit refresh of `getChatEventLog` — clears the
    /// cached page and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_chat_event_log(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.event_logs.remove(&chat_id.0);
        self.fetch_chat_event_log(chat_id)
    }

    /// Slice G2: store the event-log search query for the chat (sent
    /// by the next `getChatEventLog`; empty clears it). Editing the
    /// query does not refetch by itself — the UI follows with
    /// `refresh_chat_event_log`.
    pub fn set_chat_event_log_query(&mut self, chat_id: ChatId, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.session.event_log_queries.remove(&chat_id.0);
        } else {
            self.session.event_log_queries.insert(chat_id.0, query);
        }
    }

    /// Slice G2: flip one event-log filter category for the chat
    /// (`toggle` flips one field of the set; clearing the last active
    /// category removes the set so the log shows all types again).
    pub fn toggle_chat_event_log_filter(
        &mut self,
        chat_id: ChatId,
        toggle: impl FnOnce(&mut ChatEventLogFilterSet),
    ) {
        let mut set = self
            .session
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default();
        toggle(&mut set);
        if set.any_enabled() {
            self.session.event_log_filters.insert(chat_id.0, set);
        } else {
            self.session.event_log_filters.remove(&chat_id.0);
        }
    }

    /// Phase D3c: the next older page of the event log. The paging cursor
    /// is the oldest cached event's id (results arrive in decreasing
    /// event-id order); the driver no-ops unless a `Loaded` page reports
    /// `has_more` — a short page means the log is exhausted.
    pub fn fetch_chat_event_log_more(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let from_event_id = match self.session.event_logs.get(&chat_id.0) {
            Some(ChatEventLogFetch::Loaded(page)) if page.has_more => {
                page.events.last().map(|event| event.id).unwrap_or(0)
            }
            _ => return Ok(None),
        };
        self.fetch_chat_event_log_page(chat_id, from_event_id)
    }

    /// Phase D3c: one `getChatEventLog` page (`from_event_id` 0 = latest).
    /// Deduped on any in-flight `GetChatEventLog` for the chat, whatever
    /// its cursor. Slice G2: sends the cached filter set / search query
    /// for the chat (both edited from the event-log dialog; changing them
    /// goes through `refresh_chat_event_log`, which clears the cache).
    fn fetch_chat_event_log_page(
        &mut self,
        chat_id: ChatId,
        from_event_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_view_event_log(chat_id) {
            return Ok(None);
        }
        if from_event_id == 0
            && matches!(
                self.session.event_logs.get(&chat_id.0),
                Some(ChatEventLogFetch::Loading | ChatEventLogFetch::Loaded(_))
            )
        {
            return Ok(None);
        }
        if self.session.requests.has_event_log_in_flight(chat_id) {
            return Ok(None);
        }
        if from_event_id == 0 {
            self.session
                .event_logs
                .insert(chat_id.0, ChatEventLogFetch::Loading);
        }
        let filters = self
            .session
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .filter(|filters| filters.any_enabled());
        let query = self
            .session
            .event_log_queries
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let extra = self.session.request(
            RequestPurpose::GetChatEventLog { from_event_id },
            Some(chat_id),
        );
        if let Err(err) = self.sender.send_json(&get_chat_event_log(
            extra,
            chat_id.0,
            &query,
            from_event_id,
            CHAT_EVENT_LOG_PAGE_SIZE,
            filters,
            &[],
        )) {
            self.session.requests.take(extra);
            if from_event_id == 0 {
                self.session.event_logs.remove(&chat_id.0);
            }
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: shared `setChatMemberStatus` sender (TDLib 1.8.67,
    /// `schema/td_api.tl:13592`). Gated on `can_promote_members` and
    /// deduped per (chat, user, kind). The `ok` response invalidates the
    /// cached admin list; the member change itself arrives as
    /// `updateChatMember`.
    fn send_set_chat_member_status(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        kind: MemberStatusChange,
        status: &serde_json::Value,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetChatMemberStatus { user_id, kind };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: promote a member to administrator with the given rights
    /// (`chatMemberStatusAdministrator`, schema 1.8.67, line 2500).
    pub fn promote_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        rights: &ChatAdminRights,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let status = chat_member_status_administrator_json(true, rights);
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::Promote, &status)
    }

    /// Phase D3b: edit an administrator's rights (same
    /// `chatMemberStatusAdministrator` shape as promote).
    pub fn edit_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        rights: &ChatAdminRights,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let status = chat_member_status_administrator_json(true, rights);
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::EditRights, &status)
    }

    /// Phase D3b: demote an administrator to a plain member
    /// (`chatMemberStatusMember`, schema 1.8.67, line 2504).
    pub fn demote_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        // Phase D3b: the owner can never be demoted — the UI hides the
        // action, but enforce it at the driver level too so no future
        // caller can bypass it. Unknown/unloaded list → allow and let
        // TDLib reject as the backstop.
        let is_owner = matches!(
            self.session.admin_lists.get(&chat_id.0),
            Some(AdminListFetch::Loaded(list))
                if list.iter().any(|e| e.user_id == user_id && e.is_owner)
        );
        if is_owner {
            return Ok(None);
        }
        let status = chat_member_status_member_json();
        self.send_set_chat_member_status(chat_id, user_id, MemberStatusChange::Demote, &status)
    }

    /// Phase D3b: `getChatMember` for one administrator's current rights
    /// (schema 1.8.67, line 13622), backing the edit-rights dialog.
    /// Deduped per (chat, user); the answer lands in
    /// `Session::admin_rights`.
    pub fn fetch_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_manage_admins(chat_id) {
            return Ok(None);
        }
        if matches!(
            self.session.admin_rights.get(&(chat_id.0, user_id)),
            Some(AdminRightsFetch::Loading | AdminRightsFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetAdminRights { user_id };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.session
            .admin_rights
            .insert((chat_id.0, user_id), AdminRightsFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, user_id))
        {
            self.session.requests.take(extra);
            self.session.admin_rights.remove(&(chat_id.0, user_id));
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Phase D3b: explicit refresh of one administrator's rights — clears
    /// the cached result and re-sends (the plain fetch keeps `Loaded`).
    pub fn refresh_admin_rights(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.admin_rights.remove(&(chat_id.0, user_id));
        self.fetch_admin_rights(chat_id, user_id)
    }

    /// Phase D3b / slice G1: `getSupergroupMembers` (TDLib 1.8.67,
    /// `schema/td_api.tl:15238`) for one member-list page. The page is
    /// cached per (chat, filter); `query` narrows the search-style
    /// filters (empty = no narrowing). The restricted/banned filters
    /// require the `can_restrict_members` administrator right (schema
    /// lines 2570/2574); the other filters are available to every
    /// member. Only supergroup chats (incl. channels) have members.
    /// Deduped like the other D3b fetches.
    pub fn fetch_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if filter.requires_restrict_right() && !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup { supergroup_id, .. } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        let key = (chat_id.0, filter);
        let purpose = RequestPurpose::GetSupergroupMembers { filter };
        if matches!(
            self.session.supergroup_members.get(&key),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        self.session
            .supergroup_members
            .insert(key, SupergroupMembersFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        let filter_json = match filter {
            MemberListFilter::Recent => supergroup_members_filter_recent_json(),
            MemberListFilter::Search => supergroup_members_filter_search_json(query),
            MemberListFilter::Administrators => supergroup_members_filter_administrators_json(),
            MemberListFilter::Restricted => supergroup_members_filter_restricted_json(query),
            MemberListFilter::Banned => supergroup_members_filter_banned_json(query),
        };
        if let Err(err) = self.sender.send_json(&get_supergroup_members(
            extra,
            supergroup_id,
            &filter_json,
            0,
            200,
        )) {
            self.session.requests.take(extra);
            self.session.supergroup_members.remove(&key);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: explicit refresh of one member-list page — clears the
    /// cached (chat, filter) page and re-sends.
    pub fn refresh_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.session.supergroup_members.remove(&(chat_id.0, filter));
        self.fetch_supergroup_members(chat_id, filter, query)
    }

    /// Slice G1: `getBasicGroupFullInfo` (schema 1.8.67, line 11507) —
    /// the member list for a basic group. Any member may view it.
    /// Deduped like the other fetches.
    pub fn fetch_basic_group_members(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let basic_group_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::BasicGroup { basic_group_id } => basic_group_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        let purpose = RequestPurpose::GetBasicGroupFullInfo;
        if matches!(
            self.session.basic_group_members.get(&chat_id.0),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        self.session
            .basic_group_members
            .insert(chat_id.0, SupergroupMembersFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_basic_group_full_info(extra, basic_group_id))
        {
            self.session.requests.take(extra);
            self.session.basic_group_members.remove(&chat_id.0);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: `addChatMembers` / `addChatMember` (schema 1.8.67, lines
    /// 13584/13578). Basic groups use the singular variant (the bulk one
    /// is supergroups and channels only, line 13580). Both require the
    /// `can_invite_users` member right. The response is
    /// `failedToAddMembers`; added members arrive as `updateChatMember`.
    pub fn add_chat_members(
        &mut self,
        chat_id: ChatId,
        user_ids: &[i64],
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if user_ids.is_empty() || !self.session.chat_can_add_members(chat_id) {
            return Ok(None);
        }
        let is_basic_group = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::BasicGroup { .. })
        );
        // Slice G1 fix-up: basic groups send one `addChatMember` per
        // user, each answering `failedToAddMembers` — a distinct purpose
        // so per-user responses accumulate instead of replacing the
        // single bulk count.
        let purpose = if is_basic_group {
            RequestPurpose::AddChatMember
        } else {
            RequestPurpose::AddChatMembers
        };
        if self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::AddChatMembers, chat_id)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::AddChatMember, chat_id)
        {
            return Ok(None);
        }
        // Slice G1: a new add attempt resets the failure count — errors
        // from a previous attempt must not linger into this one.
        self.session.add_members_failed.remove(&chat_id.0);
        // Basic groups need one `addChatMember` per user; supergroups and
        // channels take a single bulk `addChatMembers`.
        let mut first_extra = None;
        if is_basic_group {
            for user_id in user_ids {
                let req_extra = self.session.request(purpose, Some(chat_id));
                if let Err(err) = self
                    .sender
                    .send_json(&add_chat_member(req_extra, chat_id.0, *user_id))
                {
                    self.session.requests.take(req_extra);
                    if first_extra.is_none() {
                        return Err(err);
                    }
                    break;
                }
                if first_extra.is_none() {
                    first_extra = Some(req_extra);
                }
            }
        } else {
            let extra = self.session.request(purpose, Some(chat_id));
            if let Err(err) = self
                .sender
                .send_json(&add_chat_members(extra, chat_id.0, user_ids))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
            first_extra = Some(extra);
        }
        Ok(first_extra)
    }

    /// Slice G1: restrict a member (`chatMemberStatusRestricted`, schema
    /// 1.8.67, line 2510) via `setChatMemberStatus` (line 13592), which
    /// requires the `can_restrict_members` administrator right (lines
    /// 13586-13587). Not supported in basic groups and channels (line
    /// 2510) — non-channel supergroups only. The change itself arrives as
    /// `updateChatMember`.
    pub fn restrict_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        until_date: i32,
        permissions: &ChatPermissions,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_group = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(
                chat.kind,
                ChatKind::Supergroup {
                    is_channel: false,
                    ..
                }
            )
        );
        if !is_group || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_restricted_json(true, until_date, &permissions.to_json());
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Restrict,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: ban a member (`chatMemberStatusBanned`, schema 1.8.67,
    /// line 2517) via `setChatMemberStatus`. Works in supergroups and
    /// channels ("Chats can be only banned and unbanned in supergroups
    /// and channels", line 13587); requires `can_restrict_members`.
    pub fn ban_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
        until_date: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_bannable = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::Supergroup { .. })
        );
        if !is_bannable || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_banned_json(until_date);
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Ban,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: unban / unrestrict a member — `setChatMemberStatus` back
    /// to plain `chatMemberStatusMember` (schema 1.8.67, line 2504).
    pub fn unban_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_bannable = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::Supergroup { .. })
        );
        if !is_bannable || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let status = chat_member_status_member_json();
        let purpose = RequestPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Unban,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
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
