//! The fetch of a group's member list: its first page, the page kept
//! while a refetch is out, and the next page.
use super::*;

/// Phase D3b: fetch state for one chat's `getSupergroupMembers` result
/// (schema 1.8.67, line 15238) backing the promote member picker. Same
/// Loading-guard convention.
#[derive(Debug, Clone, PartialEq)]
pub enum SupergroupMembersFetch {
    /// Asked as `request`: only that request's answer or error leaves the
    /// stage. `last` is the page it replaces, still drawn until the answer.
    Loading {
        request: RequestId,
        last: Option<MembersPage>,
    },
    /// A membership change made `last` out of date and nobody asked again
    /// yet: the page stays drawn, and whoever shows the list (or the
    /// poll, for the lists on screen) asks, which moves it to `Loading`.
    /// No request is out, so no answer can land on it.
    Stale { last: MembersPage },
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
            Some(Self::Stale { last }) => Some(last),
            Some(Self::Failed(_) | Self::NotSent(_)) | None => None,
        };
        Self::Loading { request, last }
    }

    /// Whether the fetch is out.
    pub fn in_flight(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// Whether the fetch is `Loading` as `request`.
    pub fn waits_for(&self, request: RequestId) -> bool {
        matches!(self, Self::Loading { request: asked, .. } if *asked == request)
    }

    /// A membership change made this out of date: a page stays drawn
    /// as `Stale` (its refetch owed); anything else goes (`None`).
    pub fn made_stale(self) -> Option<Self> {
        match self {
            Self::Loaded {
                members,
                total_count,
            } => Some(Self::Stale {
                last: MembersPage {
                    members,
                    total_count,
                },
            }),
            Self::Loading {
                last: Some(last), ..
            } => Some(Self::Stale { last }),
            stale @ Self::Stale { .. } => Some(stale),
            Self::Loading { last: None, .. } | Self::Failed(_) | Self::NotSent(_) => None,
        }
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

/// One member list of a chat and a filter: the query it was asked with,
/// where its first page stands (the request it waits for is inside the
/// stage), and the next page. A new first page replaces the whole value,
/// so a next page of an older query or an older answer finds itself gone.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberList {
    pub query: String,
    pub fetch: SupergroupMembersFetch,
    /// Only meaningful while `fetch` is `Loaded` (a page that goes takes
    /// its next page with it).
    pub more: MoreMembers,
}

impl MemberList {
    /// A first page asked as `request` for `query`, over the list `previous`.
    pub fn asked(previous: Option<Self>, request: RequestId, query: &str) -> Self {
        Self {
            query: query.to_string(),
            fetch: SupergroupMembersFetch::asked(previous.map(|list| list.fetch), request),
            more: MoreMembers::Idle,
        }
    }

    /// A list that holds `fetch` as it was filled, with no next page.
    pub fn of(query: &str, fetch: SupergroupMembersFetch) -> Self {
        Self {
            query: query.to_string(),
            fetch,
            more: MoreMembers::Idle,
        }
    }
}
