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
    pub loading: bool,
    pub error: Option<String>,
    /// Newest page request; replies with another id are stale.
    pub request: Option<RequestId>,
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
    pub loading: bool,
    pub error: Option<String>,
    pub request: Option<RequestId>,
}

impl Session {
    /// The username lists of the supergroup behind `chat_id`, if known.
    pub fn chat_usernames(&self, chat_id: ChatId) -> Option<&SupergroupUsernames> {
        self.chat_supergroup(chat_id)
            .and_then(|id| self.supergroup_username_lists.get(&id))
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
            self.supergroup_username_lists.remove(&supergroup_id);
        } else {
            self.supergroup_username_lists
                .insert(supergroup_id, usernames);
        }
    }
}
