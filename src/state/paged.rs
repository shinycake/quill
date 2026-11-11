//! One value for a paged list fetched from the server: not asked, a page in
//! flight, answered, out of date, or failed. It replaces a `loading` bool, a
//! `loaded` bool, an `error` and a request id kept beside the items.
//!
//! The items so far stay reachable under a page in flight, an out-of-date
//! mark and a failure, so a list does not blank out. A page in flight
//! carries its request id: only its answer or error lands, so a refetch and
//! an appended page never mix. Every stage has a way out: the answer, the
//! error, a failed send ([`Paged::send_failed`]), a change that makes the
//! list wrong ([`Paged::outdate`]) and a reset ([`Paged::reset`]).
use crate::ids::RequestId;

/// What a page of the list holds.
pub trait Page {
    /// The offset of the next page; empty when there is none.
    fn next_offset(&self) -> &str;
    /// Add the page that follows this one.
    fn append(&mut self, later: Self);
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Paged<P> {
    /// Never asked.
    #[default]
    Idle,
    /// A page is in flight. `append` is a follow-up page, else a first page
    /// that replaces the list.
    Loading {
        request: RequestId,
        append: bool,
        stale: Option<P>,
    },
    Ready(P),
    /// A change landed: the old list, if any, stays visible until the
    /// refetch replaces it (never optimistic).
    Outdated(Option<P>),
    Failed {
        error: String,
        stale: Option<P>,
    },
}

impl<P: Page> Paged<P> {
    /// The newest list held.
    pub fn data(&self) -> Option<&P> {
        match self {
            Self::Idle => None,
            Self::Ready(data) => Some(data),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held.as_ref(),
        }
    }

    pub fn data_mut(&mut self) -> Option<&mut P> {
        match self {
            Self::Idle => None,
            Self::Ready(data) => Some(data),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held.as_mut(),
        }
    }

    fn take_data(&mut self) -> Option<P> {
        match std::mem::take(self) {
            Self::Idle => None,
            Self::Ready(data) => Some(data),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held,
        }
    }

    /// A page is in flight.
    pub fn loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// The list answered and nothing has outdated it since.
    pub fn loaded(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    /// The request in flight, if it is `request`.
    pub fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!(self, Self::Loading { request: asked, .. } if Some(*asked) == request)
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The offset of the next page; empty when there is none.
    pub fn next_offset(&self) -> &str {
        self.data().map_or("", Page::next_offset)
    }

    /// Whether a first page may go out: never asked, failed, or outdated.
    /// A page in flight or an answer is not asked again.
    pub fn wants_first_page(&self) -> bool {
        matches!(self, Self::Idle | Self::Outdated(_) | Self::Failed { .. })
    }

    /// The list is out of date and a refetch is due.
    pub fn is_outdated(&self) -> bool {
        matches!(self, Self::Outdated(_))
    }

    /// A page goes out. A first page keeps the list visible under it.
    pub fn begin(&mut self, request: RequestId, append: bool) {
        let stale = self.take_data();
        *self = Self::Loading {
            request,
            append,
            stale,
        };
    }

    /// The page's send failed, so no answer will come.
    pub fn send_failed(&mut self, error: impl Into<String>) {
        let stale = self.take_data();
        *self = Self::Failed {
            error: error.into(),
            stale,
        };
    }

    /// A page answered. Only the page in flight lands.
    pub fn answer(&mut self, request: RequestId, page: P) -> bool {
        let Self::Loading {
            request: asked,
            append,
            stale,
        } = self
        else {
            return false;
        };
        if *asked != request {
            return false;
        }
        let list = match (*append, stale.take()) {
            (true, Some(mut existing)) => {
                existing.append(page);
                existing
            }
            _ => page,
        };
        *self = Self::Ready(list);
        true
    }

    /// A page failed. Only the page in flight fails the list.
    pub fn fail(&mut self, request: Option<RequestId>, error: impl Into<String>) -> bool {
        if !self.waits_for(request) {
            return false;
        }
        self.send_failed(error);
        true
    }

    /// The list is out of date, also when nothing was held (the change
    /// needs the answer). A page still in flight asked before the change,
    /// so its answer is dropped with its request; a first page in flight
    /// with nothing held keeps its place (there is nothing to refetch
    /// over).
    pub fn outdate(&mut self) {
        if matches!(self, Self::Loading { stale: None, .. }) {
            return;
        }
        *self = Self::Outdated(self.take_data());
    }

    /// The owner of the list changed (a filter, a chat): nothing is kept.
    pub fn reset(&mut self) {
        *self = Self::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Rows {
        rows: Vec<u8>,
        next: String,
    }

    impl Page for Rows {
        fn next_offset(&self) -> &str {
            &self.next
        }
        fn append(&mut self, later: Self) {
            self.rows.extend(later.rows);
            self.next = later.next;
        }
    }

    fn rows(rows: &[u8], next: &str) -> Rows {
        Rows {
            rows: rows.to_vec(),
            next: next.into(),
        }
    }

    #[test]
    fn a_next_page_appends_and_only_the_page_in_flight_lands() {
        let mut list = Paged::<Rows>::Idle;
        list.begin(RequestId(1), false);
        assert!(!list.answer(RequestId(9), rows(&[9], "")));
        assert!(list.answer(RequestId(1), rows(&[1], "50")));
        assert_eq!(list.next_offset(), "50");
        list.begin(RequestId(2), true);
        assert_eq!(list.data().map(|d| d.rows.len()), Some(1));
        assert!(list.answer(RequestId(2), rows(&[2], "")));
        assert_eq!(list.data().map(|d| d.rows.clone()), Some(vec![1, 2]));
        assert_eq!(list.next_offset(), "");
        assert!(list.loaded());
    }

    #[test]
    fn a_change_drops_the_page_in_flight_and_asks_again() {
        let mut list = Paged::<Rows>::Idle;
        list.begin(RequestId(1), false);
        list.answer(RequestId(1), rows(&[1], "50"));
        list.begin(RequestId(2), true);
        list.outdate();
        assert!(list.wants_first_page() && list.is_outdated());
        assert!(!list.answer(RequestId(2), rows(&[2], "")));
        assert_eq!(list.data().map(|d| d.rows.clone()), Some(vec![1]));
        list.begin(RequestId(3), false);
        assert!(list.answer(RequestId(3), rows(&[7], "")));
        assert_eq!(list.data().map(|d| d.rows.clone()), Some(vec![7]));
    }

    #[test]
    fn a_failure_keeps_the_list_and_leaves_loading() {
        let mut list = Paged::<Rows>::Idle;
        list.begin(RequestId(1), false);
        list.answer(RequestId(1), rows(&[1], "50"));
        list.begin(RequestId(2), true);
        assert!(!list.fail(Some(RequestId(5)), "late"));
        assert!(list.fail(Some(RequestId(2)), "boom"));
        assert_eq!(list.error(), Some("boom"));
        assert!(!list.loading());
        assert_eq!(list.data().map(|d| d.rows.len()), Some(1));
        list.begin(RequestId(3), false);
        list.send_failed("not sent");
        assert_eq!(list.error(), Some("not sent"));
        list.reset();
        assert!(list.data().is_none() && list.wants_first_page());
    }
}
