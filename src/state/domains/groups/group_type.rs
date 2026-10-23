//! The Type box's share of the session (tdesktop `edit_peer_type_box.cpp`,
//! `PublicLinksLimitBox`): the newest link check, whether the account is
//! at its public links limit, the public chats it may free a link from,
//! and the usernames list's last refusal.
use crate::premium_limits::PremiumLimitKind;
use crate::state::*;
use crate::telegram::envelope::UsernameCheckResult;

/// The Type box's requests; wrapped as `GroupsPurpose::GroupType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupTypePurpose {
    /// `checkChatUsername`; only the newest generation counts.
    CheckChatUsername { generation: u64 },
    /// `checkCreatedPublicChatsLimit`: an error means the limit is reached.
    CheckPublicChatsLimit,
    /// `getCreatedPublicChats` for the limit box's list.
    GetCreatedPublicChats,
    /// `disableAllSupergroupUsernames`.
    DisableAllUsernames,
}

/// The server's answer for one typed link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsernameVerdict {
    pub chat_id: ChatId,
    pub username: String,
    pub result: UsernameCheckResult,
}

/// The owned public chats of the limit box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PublicChatsFetch {
    #[default]
    None,
    Loading,
    Loaded(Vec<ChatId>),
    Failed,
}

/// A link being freed from the limit box: `setSupergroupUsername("")`
/// and `disableAllSupergroupUsernames` (tdesktop
/// `PublicsController::rowRightActionClicked`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicLinkRevoke {
    pub chat_id: ChatId,
    pub requests: Vec<RequestId>,
    pub failed: bool,
}

/// Why the usernames list could not show or hide a link (tdesktop
/// `Api::Usernames::Error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernameToggleError {
    /// `USERNAMES_ACTIVE_TOO_MUCH`.
    TooMany,
    Flood,
    Other,
}

/// The rest of an Edit box save that waits for a basic group to become a
/// supergroup (tdesktop `saveUsername` migrates first).
#[derive(Debug, Clone, PartialEq)]
pub struct EditAfterUpgrade {
    pub old: ChatId,
    pub info: Vec<crate::group_info_edit::GroupInfoEdit>,
    pub kind: Vec<crate::group_type::GroupTypeEdit>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GroupTypeState {
    /// Counts checks; an answer for an older one is dropped.
    pub check_generation: u64,
    /// The chat and text of the check in flight.
    pub check_pending: Option<(ChatId, String)>,
    /// The newest answer.
    pub check: Option<UsernameVerdict>,
    /// `checkCreatedPublicChatsLimit`: `Some(true)` at the limit.
    pub public_limit_reached: Option<bool>,
    /// Chats `checkChatUsername` said can't be public
    /// (`PublicGroupsUnavailable`).
    pub public_unavailable: HashSet<i64>,
    pub public_chats: PublicChatsFetch,
    pub revoke: Option<PublicLinkRevoke>,
    /// The usernames list's last refused toggle, by chat id.
    pub toggle_error: Option<(ChatId, UsernameToggleError)>,
    /// Saves waiting for `upgradeBasicGroupChatToSupergroupChat`, by its
    /// request id.
    pub after_upgrade: Vec<(RequestId, EditAfterUpgrade)>,
}

fn group_type_purpose(pending: Option<&PendingRequest>) -> Option<GroupTypePurpose> {
    match pending?.purpose {
        RequestPurpose::Groups(GroupsPurpose::GroupType(purpose)) => Some(purpose),
        _ => None,
    }
}

impl Session {
    /// The verdict for exactly `username` in `chat_id`, if one came.
    pub fn group_username_verdict(
        &self,
        chat_id: ChatId,
        username: &str,
    ) -> Option<UsernameCheckResult> {
        self.groups
            .group_type
            .check
            .as_ref()
            .filter(|v| v.chat_id == chat_id && v.username == username)
            .map(|v| v.result)
    }

    /// What the Type box opens with: the chat as TDLib last reported it
    /// (tdesktop `EditPeerTypeBox` fills from the peer).
    pub fn group_type_current(&self, chat_id: ChatId) -> crate::group_type::GroupTypeData {
        let lists = self.chat_usernames(chat_id);
        let username = self
            .chat_supergroup(chat_id)
            .and_then(|sg| self.supergroup_username(sg))
            .map(str::to_string)
            .or_else(|| lists.map(|l| l.editable.clone()))
            .unwrap_or_default();
        let order = lists.map(|l| l.active.clone()).unwrap_or_default();
        let request_to_join = self
            .chat_supergroup(chat_id)
            .is_some_and(|sg| self.groups.supergroup_join_by_request.get(&sg) == Some(&true));
        crate::group_type::GroupTypeData {
            public: !username.is_empty() || !order.is_empty(),
            username,
            order,
            no_forwards: self.chat_has_protected_content(chat_id),
            join_to_write: self.chat_join_to_send(chat_id),
            request_to_join,
            apply_to_invites: None,
        }
    }

    /// The chat's permanent link (tdesktop `CreateInviteLinkBox` primary).
    pub fn chat_primary_invite_link(
        &self,
        chat_id: ChatId,
    ) -> Option<&crate::telegram::envelope::ParsedChatInviteLink> {
        match self.groups.invite_links.get(&chat_id.0)? {
            InviteLinkFetch::Loaded(list) => list
                .links
                .iter()
                .find(|link| link.is_primary && !link.is_revoked),
            _ => None,
        }
    }

    /// Every admin's active invite links, summed; "Apply to existing
    /// invite links" is asked only when there is one.
    pub fn chat_active_invite_link_count(&self, chat_id: ChatId) -> i64 {
        match self.groups.invite_link_counts.get(&chat_id.0) {
            Some(InviteLinkCountsFetch::Loaded(counts)) => counts
                .iter()
                .map(|count| i64::from(count.invite_link_count))
                .sum(),
            _ => 0,
        }
    }

    /// A `checkChatUsernameResult*` answering the Type box's check. At
    /// the limit, the public links box opens (tdesktop
    /// `askUsernameRevoke`).
    pub(crate) fn apply_group_username_check(
        &mut self,
        result: UsernameCheckResult,
        pending: Option<&PendingRequest>,
    ) {
        let Some(GroupTypePurpose::CheckChatUsername { generation }) = group_type_purpose(pending)
        else {
            return;
        };
        let state = &mut self.groups.group_type;
        if generation != state.check_generation {
            return;
        }
        let Some((chat_id, username)) = state.check_pending.take() else {
            return;
        };
        match result {
            UsernameCheckResult::PublicChatsTooMany => {
                state.public_limit_reached = Some(true);
                self.payments.hub.limit_box = Some((PremiumLimitKind::PublicLinks, 0));
            }
            UsernameCheckResult::PublicGroupsUnavailable => {
                state.public_unavailable.insert(chat_id.0);
            }
            _ => {}
        }
        self.groups.group_type.check = Some(UsernameVerdict {
            chat_id,
            username,
            result,
        });
    }

    /// `getCreatedPublicChats` for the limit box; `true` when ours.
    pub(crate) fn apply_created_public_chats(
        &mut self,
        chat_ids: &[ChatId],
        pending: Option<&PendingRequest>,
    ) -> bool {
        if group_type_purpose(pending) != Some(GroupTypePurpose::GetCreatedPublicChats) {
            return false;
        }
        let public: Vec<ChatId> = chat_ids
            .iter()
            .copied()
            .filter(|id| {
                self.chat_supergroup(*id)
                    .and_then(|sg| self.supergroup_username(sg))
                    .is_some_and(|name| !name.is_empty())
            })
            .collect();
        self.groups.group_type.public_chats = PublicChatsFetch::Loaded(public);
        true
    }

    /// The `ok` answers of the Type box's requests and of a revoke.
    pub(crate) fn apply_group_type_ok(&mut self, pending: Option<&PendingRequest>) {
        if group_type_purpose(pending) == Some(GroupTypePurpose::CheckPublicChatsLimit) {
            self.groups.group_type.public_limit_reached = Some(false);
        }
        self.settle_public_link_revoke(pending, false);
    }

    /// A Type box request failed.
    pub(crate) fn apply_group_type_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
    ) {
        match group_type_purpose(pending) {
            Some(GroupTypePurpose::CheckChatUsername { generation })
                if generation == self.groups.group_type.check_generation =>
            {
                self.groups.group_type.check_pending = None;
            }
            Some(GroupTypePurpose::CheckPublicChatsLimit) => {
                self.groups.group_type.public_limit_reached =
                    Some(err.premium_limit == Some(PremiumLimitKind::PublicLinks));
            }
            Some(GroupTypePurpose::GetCreatedPublicChats) => {
                self.groups.group_type.public_chats = PublicChatsFetch::Failed;
            }
            _ => {}
        }
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::ToggleSupergroupUsername
            && let Some(chat_id) = pending.chat_id
        {
            let why = if err.server_type.as_deref() == Some("USERNAMES_ACTIVE_TOO_MUCH") {
                UsernameToggleError::TooMany
            } else if err.is_flood() {
                UsernameToggleError::Flood
            } else {
                UsernameToggleError::Other
            };
            self.groups.group_type.toggle_error = Some((chat_id, why));
        }
        self.settle_public_link_revoke(pending, true);
    }

    /// One answer of a revoke. Once both came without an error, the link
    /// is free: the limit box closes and the limit no longer holds.
    fn settle_public_link_revoke(&mut self, pending: Option<&PendingRequest>, failed: bool) {
        let Some(pending) = pending else {
            return;
        };
        let state = &mut self.groups.group_type;
        let Some(revoke) = state.revoke.as_mut() else {
            return;
        };
        if !revoke.requests.contains(&pending.id) {
            return;
        }
        revoke.requests.retain(|id| *id != pending.id);
        revoke.failed |= failed;
        if !revoke.requests.is_empty() {
            return;
        }
        if revoke.failed {
            return;
        }
        state.revoke = None;
        state.public_limit_reached = Some(false);
        state.public_chats = PublicChatsFetch::None;
        if self.payments.hub.limit_box.map(|(kind, _)| kind) == Some(PremiumLimitKind::PublicLinks)
        {
            self.payments.hub.limit_box = None;
        }
    }
}
