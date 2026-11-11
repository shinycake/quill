//! Answers of the groups panels' lookups: each is applied only to the stage
//! that waits for its request, so an answer of a request the panel dropped
//! (a refresh, a changed theme) lands nowhere.
use crate::state::*;
use crate::telegram::envelope::{ChatStatistics, ParsedChatInviteLinkCount, ParsedChatMember};

impl Session {
    /// `getChatStatistics`: the theme is the one the request in flight asked
    /// for.
    pub(super) fn accept_chat_statistics(
        &mut self,
        statistics: ChatStatistics,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetChatStatistics
            && let Some(chat_id) = pending.chat_id
            && let Some(ChatStatisticsFetch::Loading { is_dark, request }) =
                self.groups.chat_statistics.get(&chat_id.0)
            && *request == pending.id
        {
            let is_dark = *is_dark;
            self.groups.chat_statistics.insert(
                chat_id.0,
                ChatStatisticsFetch::Loaded {
                    stats: std::sync::Arc::new(statistics),
                    is_dark,
                },
            );
        }
    }

    /// B8: `getChatInviteLinkCounts`.
    pub(super) fn accept_invite_link_counts(
        &mut self,
        counts: Vec<ParsedChatInviteLinkCount>,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(p) = pending
            && p.purpose == RequestPurpose::GetChatInviteLinkCounts
            && let Some(chat_id) = p.chat_id
            && matches!(
                self.groups.invite_link_counts.get(&chat_id.0),
                Some(InviteLinkCountsFetch::Loading(asked)) if *asked == p.id
            )
        {
            self.groups
                .invite_link_counts
                .insert(chat_id.0, InviteLinkCountsFetch::Loaded(counts));
        }
    }

    /// Phase D3b: `getChatAdministrators` replaces the cached admin list.
    pub(super) fn accept_chat_administrators(
        &mut self,
        administrators: Vec<ChatAdministratorEntry>,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetChatAdministrators
            && let Some(chat_id) = pending.chat_id
            && matches!(
                self.groups.admin_lists.get(&chat_id.0),
                Some(AdminListFetch::Loading(asked)) if *asked == pending.id
            )
        {
            self.groups
                .admin_lists
                .insert(chat_id.0, AdminListFetch::Loaded(administrators));
        }
    }

    /// Phase D3b: `getChatMember` for one administrator's rights. A member
    /// who is no administrator (any more) has no rights block to edit: the
    /// editor says so instead of loading for good.
    pub(super) fn accept_admin_rights(
        &mut self,
        member: &ParsedChatMember,
        pending: Option<&PendingRequest>,
    ) {
        if let Some(pending) = pending
            && let RequestPurpose::Groups(GroupsPurpose::GetAdminRights { user_id }) =
                pending.purpose
            && let Some(chat_id) = pending.chat_id
            && matches!(
                self.groups.admin_rights.get(&(chat_id.0, user_id)),
                Some(AdminRightsFetch::Loading(asked)) if *asked == pending.id
            )
        {
            self.groups.admin_rights.insert(
                (chat_id.0, user_id),
                match member.admin_rights() {
                    Some(rights) => AdminRightsFetch::Loaded(rights),
                    None => AdminRightsFetch::Failed(
                        crate::lang::tr(crate::lang::keys::Q_PEOPLE_COULD_NOT_LOAD_ADMIN_RIGHTS)
                            .to_string(),
                    ),
                },
            );
        }
    }
}
