//! Connect driver: administrators, the admin event log and admin rights.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
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
            self.session.groups.admin_lists.get(&chat_id.0),
            Some(AdminListFetch::Loading | AdminListFetch::Loaded(_))
        ) || self
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::GetChatAdministrators, chat_id)
        {
            return Ok(None);
        }
        self.session
            .groups
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
            self.session.groups.admin_lists.remove(&chat_id.0);
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
        self.session.groups.admin_lists.remove(&chat_id.0);
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
        self.session.groups.event_logs.remove(&chat_id.0);
        self.fetch_chat_event_log(chat_id)
    }

    /// Slice G2: store the event-log search query for the chat (sent
    /// by the next `getChatEventLog`; empty clears it). Editing the
    /// query does not refetch by itself — the UI follows with
    /// `refresh_chat_event_log`.
    pub fn set_chat_event_log_query(&mut self, chat_id: ChatId, query: &str) {
        let query = query.trim().to_string();
        if query.is_empty() {
            self.session.groups.event_log_queries.remove(&chat_id.0);
        } else {
            self.session
                .groups
                .event_log_queries
                .insert(chat_id.0, query);
        }
    }

    /// Flip one admin in the chat's server-side event-log filter
    /// (`getChatEventLog.user_ids`). The UI follows with a refresh.
    pub fn toggle_chat_event_log_user(&mut self, chat_id: ChatId, user_id: i64) {
        let mut users = self
            .session
            .groups
            .event_log_users
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        crate::admin_extras::toggle_event_log_user(&mut users, user_id);
        if users.is_empty() {
            self.session.groups.event_log_users.remove(&chat_id.0);
        } else {
            self.session.groups.event_log_users.insert(chat_id.0, users);
        }
    }

    /// Show every admin again.
    pub fn clear_chat_event_log_users(&mut self, chat_id: ChatId) {
        self.session.groups.event_log_users.remove(&chat_id.0);
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
            .groups
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .unwrap_or_default();
        toggle(&mut set);
        if set.any_enabled() {
            self.session.groups.event_log_filters.insert(chat_id.0, set);
        } else {
            self.session.groups.event_log_filters.remove(&chat_id.0);
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
        let from_event_id = match self.session.groups.event_logs.get(&chat_id.0) {
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
                self.session.groups.event_logs.get(&chat_id.0),
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
                .groups
                .event_logs
                .insert(chat_id.0, ChatEventLogFetch::Loading);
        }
        let filters = self
            .session
            .groups
            .event_log_filters
            .get(&chat_id.0)
            .copied()
            .filter(|filters| filters.any_enabled());
        let query = self
            .session
            .groups
            .event_log_queries
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let user_ids = self
            .session
            .groups
            .event_log_users
            .get(&chat_id.0)
            .cloned()
            .unwrap_or_default();
        let extra = self.session.request(
            RequestPurpose::Groups(GroupsPurpose::GetChatEventLog { from_event_id }),
            Some(chat_id),
        );
        if let Err(err) = self.sender.send_json(&get_chat_event_log(
            extra,
            chat_id.0,
            &query,
            from_event_id,
            CHAT_EVENT_LOG_PAGE_SIZE,
            filters,
            &user_ids,
        )) {
            self.session.requests.take(extra);
            if from_event_id == 0 {
                self.session.groups.event_logs.remove(&chat_id.0);
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus { user_id, kind });
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
            self.session.groups.admin_lists.get(&chat_id.0),
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
            self.session.groups.admin_rights.get(&(chat_id.0, user_id)),
            Some(AdminRightsFetch::Loading | AdminRightsFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        self.session
            .groups
            .admin_rights
            .insert((chat_id.0, user_id), AdminRightsFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_member(extra, chat_id, user_id))
        {
            self.session.requests.take(extra);
            self.session
                .groups
                .admin_rights
                .remove(&(chat_id.0, user_id));
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
        self.session
            .groups
            .admin_rights
            .remove(&(chat_id.0, user_id));
        self.fetch_admin_rights(chat_id, user_id)
    }
}
