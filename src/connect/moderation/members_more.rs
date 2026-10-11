//! Connect driver: the next page of a supergroup member list (the info
//! panel's members block loads more as it scrolls).
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Whether a next-page request for this list is in flight.
    pub fn supergroup_members_more_in_flight(
        &self,
        chat_id: ChatId,
        filter: MemberListFilter,
    ) -> bool {
        self.session.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore {
                    filter: f, ..
                }) if f == filter
            ) && pending.chat_id == Some(chat_id)
        })
    }

    /// `getSupergroupMembers` from `offset` (tdesktop asks 200 per page
    /// after the first). The answer joins the cached (chat, filter) page,
    /// so nothing is sent unless that page is loaded and the previous
    /// next-page request is done. `query` is the search text for the
    /// `Search` filter.
    pub fn fetch_supergroup_members_more(
        &mut self,
        chat_id: ChatId,
        filter: MemberListFilter,
        query: &str,
        offset: i32,
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
        let loaded = matches!(
            self.session.groups.supergroup_members.get(&(chat_id.0, filter)),
            Some(SupergroupMembersFetch::Loaded { members, .. }) if members.len() as i32 == offset
        );
        if !loaded || self.supergroup_members_more_in_flight(chat_id, filter) {
            return Ok(None);
        }
        let purpose =
            RequestPurpose::Groups(GroupsPurpose::GetSupergroupMembersMore { filter, offset });
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
            offset,
            crate::inline_members::SERVER_PAGE,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
