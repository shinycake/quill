//! The request stage of a paged list.
#![deny(clippy::indexing_slicing)]

use crate::ids::RequestId;

/// Which page of a list a request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    /// The first page of a tab; the list is empty until it lands.
    First,
    /// A follow-up page; the rows already loaded stay.
    Next,
}

/// Where a paged list's request stands (the Stars history, the received
/// gifts, the Grams history): one value instead of loaded + loading +
/// error + request id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PageStage {
    /// Not asked yet (or the tab was reset).
    #[default]
    Idle,
    /// A page is out; `request` is its `@extra`, so only its answer (or
    /// its error) counts and an older one is dropped.
    Loading { request: RequestId, kind: PageKind },
    /// The newest page landed.
    Ready,
    /// The newest page failed; the screen shows the line.
    Failed { error: String, kind: PageKind },
}

impl PageStage {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// Whether this tab's first page has landed (a failed or running
    /// follow-up page does not undo it).
    pub fn is_loaded(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Ready => true,
            Self::Loading { kind, .. } | Self::Failed { kind, .. } => *kind == PageKind::Next,
        }
    }

    /// Whether `id` is the page request in flight.
    pub fn waits_for(&self, id: RequestId) -> bool {
        matches!(self, Self::Loading { request, .. } if *request == id)
    }

    /// The first page is neither landed nor out: ask for it.
    pub fn first_page_due(&self) -> bool {
        !self.is_loaded() && !self.is_loading()
    }

    /// The loaded list is out of date (a conversion changed the balance):
    /// the next first-page ask goes out again. A page in flight stays.
    pub fn invalidate(&mut self) {
        if !self.is_loading() {
            *self = Self::Idle;
        }
    }

    /// The request `id` failed. Only the page in flight counts, so an older
    /// page answering late cannot stop the newer one. Whether it counted.
    pub fn fail(&mut self, id: RequestId, error: String) -> bool {
        match self {
            Self::Loading { request, kind } if *request == id => {
                *self = Self::Failed { error, kind: *kind };
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(id: u64, kind: PageKind) -> PageStage {
        PageStage::Loading {
            request: RequestId(id),
            kind,
        }
    }

    #[test]
    fn only_the_newest_request_fails_the_list() {
        let mut stage = out(2, PageKind::First);
        assert!(!stage.fail(RequestId(1), "old".into()));
        assert!(stage.is_loading());
        assert!(stage.fail(RequestId(2), "new".into()));
        assert_eq!(stage.error(), Some("new"));
        assert!(!stage.is_loading());
    }

    #[test]
    fn a_failed_follow_up_page_keeps_the_first_page_loaded() {
        let mut stage = out(3, PageKind::Next);
        assert!(stage.is_loaded(), "the list under the spinner is there");
        stage.fail(RequestId(3), "x".into());
        assert!(stage.is_loaded());
        assert!(!stage.first_page_due());
        let mut first = out(4, PageKind::First);
        first.fail(RequestId(4), "x".into());
        assert!(!first.is_loaded());
        assert!(first.first_page_due(), "a failed first page is asked again");
    }

    #[test]
    fn invalidate_asks_again_but_leaves_a_request_in_flight() {
        let mut ready = PageStage::Ready;
        ready.invalidate();
        assert!(ready.first_page_due());
        let mut loading = out(5, PageKind::First);
        loading.invalidate();
        assert!(loading.waits_for(RequestId(5)));
    }
}
