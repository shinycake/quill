//! Connect driver: member lists, adding, restricting, banning and removing members.
use super::*;
use crate::state::send_failed_line;

impl<S: JsonSender> ConnectDriver<S> {
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
        self.send_supergroup_members(chat_id, filter, query, false)
    }

    /// [`Self::fetch_supergroup_members`]; `refresh` sends even when the
    /// same list for the same query is loaded or loading. A send that is
    /// refused leaves what was there (a refresh keeps showing its page),
    /// or, when there was nothing, records `Failed` so the list draws its
    /// Retry instead of a spinner nobody answers.
    fn send_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
        refresh: bool,
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembers { filter });
        // The same list for the same query is asked once; a search for
        // another query is sent even while the last one is in flight, and
        // only its answer fills the list.
        let same_query = self.session.groups.member_query(key) == query;
        if same_query
            && !refresh
            && (matches!(
                self.session.groups.supergroup_members.get(&key),
                Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
            ) || self.session.requests.has_purpose_for_chat(purpose, chat_id))
        {
            return Ok(None);
        }
        let previous = self
            .session
            .groups
            .supergroup_members
            .insert(key, SupergroupMembersFetch::Loading);
        let previous_ask = self.session.groups.member_asks.get(&key).cloned();
        let extra = self.session.request(purpose, Some(chat_id));
        self.session.groups.note_member_first(key, extra, query);
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
            // What was there comes back, with how it was asked: a page
            // stays shown, a list still loading keeps waiting for its own
            // request, a failed one keeps its Retry. Only a list that had
            // nothing becomes `Failed`, under the query that was refused,
            // so the box that asked draws Retry instead of "Loading".
            let groups = &mut self.session.groups;
            match previous {
                Some(page) => {
                    groups.supergroup_members.insert(key, page);
                    match previous_ask {
                        Some(ask) => groups.member_asks.insert(key, ask),
                        None => groups.member_asks.remove(&key),
                    };
                }
                None => {
                    groups.supergroup_members.insert(
                        key,
                        SupergroupMembersFetch::Failed(send_failed_line("Could not load members")),
                    );
                    groups.note_member_refused(key, query);
                }
            }
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G1: explicit refresh of one member-list page — re-sends it
    /// even when the same query is loaded.
    pub fn refresh_supergroup_members(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.send_supergroup_members(chat_id, filter, query, true)
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
            self.session.groups.basic_group_members.get(&chat_id.0),
            Some(SupergroupMembersFetch::Loading | SupergroupMembersFetch::Loaded { .. })
        ) || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        self.session
            .groups
            .basic_group_members
            .insert(chat_id.0, SupergroupMembersFetch::Loading);
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_basic_group_full_info(extra, basic_group_id))
        {
            self.session.requests.take(extra);
            self.session.groups.basic_group_members.remove(&chat_id.0);
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
        self.session.groups.add_members_failed.remove(&chat_id.0);
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Restrict,
        });
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Ban,
        });
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
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Unban,
        });
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

    /// "Delete" on the Removed users list (tdesktop `removeKicked`): the
    /// user leaves the list and may join again, but is not added back.
    pub fn remove_from_banned(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_supergroup = matches!(
            self.session.chats.get(&chat_id.0),
            Some(chat) if matches!(chat.kind, ChatKind::Supergroup { .. })
        );
        if !is_supergroup || !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus {
            user_id,
            kind: MemberStatusChange::Unban,
        });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let member_id = MessageSenderRef::User(user_id).to_value();
        let status = chat_member_status_left_json();
        if let Err(err) = self.sender.send_json(&set_chat_member_status(
            extra, chat_id.0, &member_id, &status,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// "Remove from group" (tdesktop `kickParticipant`). A basic group uses
    /// `banChatMember`; a supergroup or channel member is banned for good
    /// (they show under Banned and cannot rejoin until unbanned), with no
    /// automatic lift. Needs `can_restrict_members`.
    pub fn remove_chat_member(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.chat_can_restrict_members(chat_id) {
            return Ok(None);
        }
        let basic = match self.session.chats.get(&chat_id.0).map(|c| &c.kind) {
            Some(ChatKind::BasicGroup { .. }) => true,
            Some(ChatKind::Supergroup { .. }) => false,
            _ => return Ok(None),
        };
        let kind = if basic {
            MemberStatusChange::Remove
        } else {
            MemberStatusChange::Ban
        };
        let purpose = RequestPurpose::Groups(GroupsPurpose::SetChatMemberStatus { user_id, kind });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        let json = if basic {
            ban_chat_member(extra, chat_id.0, user_id, 0, false)
        } else {
            let member_id = MessageSenderRef::User(user_id).to_value();
            set_chat_member_status(
                extra,
                chat_id.0,
                &member_id,
                &chat_member_status_banned_json(0),
            )
        };
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
