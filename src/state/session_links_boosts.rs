//! State for the admin-links / boosts / usernames batch: the boosts list
//! and link, another admin's invite links, one link's pending join
//! requests, and the supergroup username lists.
use super::*;
use crate::ids::RequestId;
use crate::telegram::envelope::{ParsedChatBoost, ParsedChatJoinRequest, SupergroupUsernames};

/// Page size of `getChatBoosts` (tdesktop loads 25 and offers "Show more").
pub const BOOSTS_PAGE_SIZE: i32 = 25;

/// One chat's boosts list (`getChatBoosts`), for the tab that is open.
#[derive(Debug, Clone, PartialEq)]
pub struct BoostsListState {
    /// The "Gifts" tab asks for gift-code and giveaway boosts only.
    pub only_gifts: bool,
    pub total_count: i32,
    pub boosts: Vec<ParsedChatBoost>,
    /// Empty once the last page arrived.
    pub next_offset: String,
    /// The newest page request; replies with another id are stale.
    pub page: BoostsPage,
}

/// Where the boosts list's newest page is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BoostsPage {
    /// Nothing in flight: the list as it stands.
    #[default]
    Idle,
    /// A page was asked for; its answer is the only one that lands.
    Loading { request: RequestId },
    /// The page failed; "Show more" asks again.
    Failed(String),
}

impl BoostsListState {
    /// A page is being asked for.
    pub fn loading(&self) -> bool {
        matches!(self.page, BoostsPage::Loading { .. })
    }

    /// The line of a failed page.
    pub fn error(&self) -> Option<&str> {
        match &self.page {
            BoostsPage::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// Whether `request` is the page this list waits for.
    pub(crate) fn waits_for(&self, request: RequestId) -> bool {
        self.page == BoostsPage::Loading { request }
    }
}

/// Another admin's invite links, as the owner sees them
/// (`getChatInviteLinks` with `creator_user_id`).
#[derive(Debug, Clone, PartialEq)]
pub struct AdminLinksState {
    pub creator_user_id: i64,
    pub active: InviteLinkFetch,
    pub revoked: InviteLinkFetch,
    /// Request ids of the two lists; replies for another admin are stale.
    pub active_request: Option<RequestId>,
    pub revoked_request: Option<RequestId>,
}

/// The pending join requests of one invite link
/// (`getChatJoinRequests` with `invite_link`).
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRequestsState {
    pub invite_link: String,
    pub total_count: i32,
    pub requests: Vec<ParsedChatJoinRequest>,
    pub error: Option<String>,
    /// The page in flight, if any; its answer is the only one that lands.
    pub request: Option<RequestId>,
}

impl LinkRequestsState {
    /// A page is being asked for.
    pub fn loading(&self) -> bool {
        self.request.is_some()
    }
}

impl Session {
    /// The username lists of the supergroup behind `chat_id`, if known.
    pub fn chat_usernames(&self, chat_id: ChatId) -> Option<&SupergroupUsernames> {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.groups.supergroup_username_lists.get(&id))
    }

    /// Own admin status in a supergroup or channel; `getChatBoosts` and
    /// `getChatBoostLink` need it.
    pub fn chat_can_view_boosts(&self, chat_id: ChatId) -> bool {
        let Some(supergroup_id) = self.chat_supergroup(chat_id) else {
            return false;
        };
        self.chat_is_owner(chat_id)
            || self.supergroup_own_status(supergroup_id)
                == Some(crate::telegram::envelope::ChannelMemberStatus::Administrator)
    }

    /// Replace the cached username lists of a supergroup.
    pub(crate) fn set_supergroup_usernames(
        &mut self,
        supergroup_id: i64,
        usernames: SupergroupUsernames,
    ) {
        if usernames == SupergroupUsernames::default() {
            self.groups.supergroup_username_lists.remove(&supergroup_id);
        } else {
            self.groups
                .supergroup_username_lists
                .insert(supergroup_id, usernames);
        }
    }
}
