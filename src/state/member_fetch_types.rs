//! The fetch of a group's member list: its first page, the page kept
//! while a refetch is out, and the next page.
use super::*;

/// Phase D3b: fetch state for one chat's `getSupergroupMembers` result
/// (schema 1.8.67, line 15238) backing the promote member picker. Same
/// Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum SupergroupMembersFetch {
    /// Asked as `request` (`None` while a page a membership change dropped
    /// waits for its refetch to go out). `last` is the page it replaces,
    /// still drawn until the answer.
    Loading {
        request: Option<RequestId>,
        last: Option<MembersPage>,
    },
    Loaded {
        members: Vec<ParsedChatMember>,
        total_count: i32,
    },
    /// TDLib refused the request.
    Failed(String),
    /// The request never left: the path was inactive or the sender closed.
    /// Drawn like `Failed` with Retry, but it says nothing about whether the
    /// viewer may see the members (so no administrators fallback).
    NotSent(String),
}

/// A page of members and the group's total.
#[derive(Debug, Clone, PartialEq)]
pub struct MembersPage {
    pub members: Vec<ParsedChatMember>,
    pub total_count: i32,
}

impl SupergroupMembersFetch {
    /// A first page is asked as `request`, the page that was shown (loaded,
    /// or kept by an earlier stale mark) staying drawn meanwhile.
    pub fn asked(previous: Option<Self>, request: RequestId) -> Self {
        let last = match previous {
            Some(Self::Loaded {
                members,
                total_count,
            }) => Some(MembersPage {
                members,
                total_count,
            }),
            Some(Self::Loading { last, .. }) => last,
            Some(Self::Failed(_) | Self::NotSent(_)) | None => None,
        };
        Self::Loading {
            request: Some(request),
            last,
        }
    }

    /// Whether the fetch is out, or waits for its send.
    pub fn in_flight(&self) -> bool {
        matches!(
            self,
            Self::Loading {
                request: Some(_),
                ..
            }
        )
    }
}

/// Where the next page of a member list stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MoreMembers {
    #[default]
    Idle,
    /// The page at `offset` is out as `request`.
    Asking { request: RequestId, offset: i32 },
    /// The page at `offset` came back; asked again only when the list
    /// grows (an empty page leaves the list where it stopped).
    Settled { offset: i32 },
    /// The page at `offset` failed; the panel asks again at its next scroll.
    Failed { offset: i32 },
}

impl MoreMembers {
    /// The offset that is not asked again.
    pub fn blocked(self) -> Option<i32> {
        match self {
            MoreMembers::Idle => None,
            MoreMembers::Asking { offset, .. }
            | MoreMembers::Settled { offset }
            | MoreMembers::Failed { offset } => Some(offset),
        }
    }
}

/// How one member list was asked: the first-page request (`None` when the
/// list was filled without one), the query, and the next page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberAsk {
    pub first: Option<RequestId>,
    pub query: String,
    pub more: MoreMembers,
}
