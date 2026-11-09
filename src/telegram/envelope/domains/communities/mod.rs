//! TDLib answers for communities added in TDLib 1.8.70: the joined
//! communities, the add-chat requests and a member's chats. The
//! `community` object and its updates stay in the groups domain.
mod parse;

use crate::telegram::envelope::*;
pub(crate) use parse::parse_communities_payload;

/// Payloads for communities; wrapped as
/// [`EnvelopePayload::Communities`].
#[derive(Debug, Clone, PartialEq)]
pub enum CommunitiesPayload {
    /// `communities` — the `getJoinedCommunities` answer.
    Communities { community_ids: Vec<i64> },
    /// `communityAddChatRequests` — one page of
    /// `getCommunityAddChatRequests`.
    AddChatRequests(CommunityAddChatRequests),
    /// `communityMemberChats` — the `getCommunityMemberChats` answer.
    MemberChats(CommunityMemberChats),
}
