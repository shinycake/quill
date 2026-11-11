//! How one cached member list (`GroupsState::supergroup_members`) was
//! asked for: the first page and its query, and the next page.
use crate::ids::RequestId;

/// The next page of a member list.
///
/// `Loading` leaves on its own answer (`Idle`), its error (`Failed`), or a
/// new first page for the list (`Idle`: an older query's page never joins
/// the new results). `Failed` leaves when the panel takes the mark to ask
/// again ([`MemberPaging::take_failed`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MemberMore {
    #[default]
    Idle,
    /// The page in flight, and the first page whose list it extends
    /// (`None`: a list filled without one).
    Loading {
        request: RequestId,
        base: Option<RequestId>,
    },
    /// The last next-page request failed.
    Failed,
}

/// The first-page request of a member list with the query it asked with,
/// and its next page. A search for another query sends a new first page;
/// only the latest one's answer or error fills the list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberPaging {
    pub first: Option<(RequestId, String)>,
    pub more: MemberMore,
}

impl MemberPaging {
    /// A first page was sent: the newest, and any next page of the older
    /// query is dropped.
    pub fn first_sent(&mut self, request: RequestId, query: &str) {
        self.first = Some((request, query.to_string()));
        self.more = MemberMore::Idle;
    }

    pub fn first_id(&self) -> Option<RequestId> {
        self.first.as_ref().map(|(id, _)| *id)
    }

    /// A next page was sent: it extends the list the latest first page
    /// filled.
    pub fn more_sent(&mut self, request: RequestId) {
        self.more = MemberMore::Loading {
            request,
            base: self.first_id(),
        };
    }

    /// Whether `request` is the next page the list waits for and extends
    /// the list shown now. The page is settled either way it is the one
    /// waited for; an older query's page is dropped.
    pub fn take_more(&mut self, request: RequestId) -> bool {
        match self.more {
            MemberMore::Loading {
                request: asked,
                base,
            } if asked == request => {
                self.more = MemberMore::Idle;
                base == self.first_id()
            }
            _ => false,
        }
    }

    /// The next page the list waited for failed.
    pub fn more_failed(&mut self) {
        self.more = MemberMore::Failed;
    }

    /// Takes the failed mark: whether the last next page failed.
    pub fn take_failed(&mut self) -> bool {
        let failed = self.more == MemberMore::Failed;
        if failed {
            self.more = MemberMore::Idle;
        }
        failed
    }
}
