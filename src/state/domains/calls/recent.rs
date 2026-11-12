//! The recent-calls list (`searchCallMessages` pages and
//! `deleteAllCallMessages`) as one value.
use crate::state::paged::{Page, Paged};
use crate::state::*;

/// The calls listed so far and where the next page starts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallPage {
    pub rows: Vec<ParsedMessage>,
    pub next_offset: String,
}

impl CallPage {
    pub fn new(rows: Vec<ParsedMessage>, next_offset: String) -> Self {
        Self { rows, next_offset }
    }
}

impl Page for CallPage {
    fn next_offset(&self) -> &str {
        &self.next_offset
    }

    fn append(&mut self, later: Self) {
        self.rows.extend(later.rows);
        self.next_offset = later.next_offset;
    }
}

/// The stage of the recent-calls list: its pages (the offset lives with
/// the rows, a reopen resets both together), or "Clear all" waiting for
/// TDLib. Only the answer or error of the request in the stage lands: a
/// reopen asks for the first page again at once, and "Clear all" forgets
/// the page in flight, so an older page never fills the list.
#[derive(Debug, Clone)]
pub enum RecentCalls {
    List(Paged<CallPage>),
    /// `deleteAllCallMessages` is out: the calls stay listed until `ok`;
    /// its error shows them again.
    Clearing {
        request: RequestId,
        list: CallPage,
    },
}

impl Default for RecentCalls {
    fn default() -> Self {
        Self::List(Paged::Idle)
    }
}

impl RecentCalls {
    /// The calls listed so far, newest first.
    pub fn rows(&self) -> &[ParsedMessage] {
        match self {
            Self::List(list) => list.data().map_or(&[], |page| page.rows.as_slice()),
            Self::Clearing { list, .. } => &list.rows,
        }
    }

    /// The offset of the next page; empty when the list is complete.
    pub fn next_offset(&self) -> &str {
        match self {
            Self::List(list) => list.next_offset(),
            Self::Clearing { list, .. } => &list.next_offset,
        }
    }

    pub fn loading(&self) -> bool {
        matches!(self, Self::List(list) if list.loading())
    }

    /// The last `searchCallMessages` request failed.
    pub fn failed(&self) -> bool {
        matches!(self, Self::List(list) if list.error().is_some())
    }

    pub fn clearing(&self) -> bool {
        matches!(self, Self::Clearing { .. })
    }

    /// The list starts over: nothing is kept, a page in flight is
    /// forgotten. A clear in flight keeps its stage.
    pub fn reset(&mut self) {
        if !self.clearing() {
            *self = Self::default();
        }
    }

    /// A page went out as `request`: a follow-up page when a next offset
    /// is held, else the first one. A clear in flight keeps its stage.
    pub fn page_sent(&mut self, request: RequestId) {
        if let Self::List(list) = self {
            let append = !list.next_offset().is_empty();
            list.begin(request, append);
        }
    }

    /// The page could not be sent: the list shows the failure.
    pub fn page_not_sent(&mut self) {
        if let Self::List(list) = self {
            list.send_failed("could not send");
        }
    }

    /// `request` answered with a page: taken only when it is the page the
    /// list waits for.
    pub fn page_answered(&mut self, request: RequestId, page: CallPage) -> bool {
        match self {
            Self::List(list) => list.answer(request, page),
            Self::Clearing { .. } => false,
        }
    }

    /// `request` failed: only the page the list waits for fails it.
    pub fn page_failed(&mut self, request: RequestId) -> bool {
        match self {
            Self::List(list) => list.fail(Some(request), "could not load the calls"),
            Self::Clearing { .. } => false,
        }
    }

    /// "Clear all" went out as `request`: the calls stay listed until it
    /// answers; a page in flight is dropped.
    pub fn clear_sent(&mut self, request: RequestId) {
        let list = match std::mem::take(self) {
            Self::List(list) => list.data().cloned().unwrap_or_default(),
            Self::Clearing { list, .. } => list,
        };
        *self = Self::Clearing { request, list };
    }

    /// `request` (the clear) answered `ok`: the list is empty and starts
    /// over. Another request changes nothing.
    pub fn clear_done(&mut self, request: RequestId) -> bool {
        if matches!(self, Self::Clearing { request: waiting, .. } if *waiting == request) {
            *self = Self::default();
            return true;
        }
        false
    }

    /// `request` (the clear) was refused: the calls show again.
    pub fn clear_failed(&mut self, request: RequestId) -> bool {
        let Self::Clearing {
            request: waiting,
            list,
        } = self
        else {
            return false;
        };
        if *waiting != request {
            return false;
        }
        *self = Self::List(Paged::Ready(std::mem::take(list)));
        true
    }
}
