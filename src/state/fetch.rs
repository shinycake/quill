//! One value for a fetched resource: not asked, in flight, answered, or
//! failed. It replaces a `loading: bool` and an `error` stored beside the
//! data, which could be set in combinations that mean nothing (loading and
//! failed at once, an error with no request).
//!
//! The previous answer stays reachable while a refetch runs or after it
//! fails, so a list does not blank out under a spinner or an error line.

/// What a fetched list is doing besides loading: one change at a time under
/// its request, or waiting for a refetch after a change (or after something
/// else moved the list). Only that request's answer or error ends the
/// change; an older request's late answer leaves a newer one waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListOp {
    #[default]
    Idle,
    /// A change is in flight under this request; its controls stay disabled.
    Mutating(crate::ids::RequestId),
    /// A change is in flight and something else moved the list meanwhile:
    /// the list is fetched again whether the change lands or is refused.
    MutatingStale(crate::ids::RequestId),
    /// The list is kept for display and fetched again before it is trusted.
    Stale,
}

impl ListOp {
    pub fn is_mutating(self) -> bool {
        matches!(self, Self::Mutating(_) | Self::MutatingStale(_))
    }

    pub fn is_stale(self) -> bool {
        self == Self::Stale
    }

    /// A change goes out under `request`.
    pub fn begin(&mut self, request: crate::ids::RequestId) {
        *self = match *self {
            Self::Stale | Self::MutatingStale(_) => Self::MutatingStale(request),
            _ => Self::Mutating(request),
        };
    }

    /// `request` came back with a refusal, or never left: the change did
    /// not happen. Another request's late answer changes nothing; an answer
    /// that names no request (its entry is gone) frees the list, or it
    /// would stay locked for good.
    pub fn failed(&mut self, request: Option<crate::ids::RequestId>) {
        match *self {
            Self::Mutating(waiting) if request.is_none_or(|id| id == waiting) => *self = Self::Idle,
            Self::MutatingStale(waiting) if request.is_none_or(|id| id == waiting) => {
                *self = Self::Stale;
            }
            _ => {}
        }
    }

    /// `request` changed the list on the server: it is fetched again. A
    /// newer change in flight keeps its place and marks the list when it
    /// lands.
    pub fn changed(&mut self, request: Option<crate::ids::RequestId>) {
        match *self {
            Self::Mutating(waiting) | Self::MutatingStale(waiting)
                if request.is_some_and(|id| id != waiting) =>
            {
                *self = Self::MutatingStale(waiting);
            }
            _ => *self = Self::Stale,
        }
    }

    /// Something else moved the list (not a change of ours): fetch it
    /// again, unless a change is in flight (its landing marks the list).
    pub fn mark_stale(&mut self) {
        match *self {
            Self::Idle => *self = Self::Stale,
            Self::Mutating(request) => *self = Self::MutatingStale(request),
            Self::Stale | Self::MutatingStale(_) => {}
        }
    }

    /// The refetch went out, was answered or failed: no longer waiting.
    pub fn refetched(&mut self) {
        if *self == Self::Stale {
            *self = Self::Idle;
        }
    }
}

/// The refusal of a change to a fetched list (a block, a terminate, a mask
/// reorder), shown beside the list. It belongs to the changes' own
/// requests: the note holds every request it was given (several changes can
/// be out at once), and the refusal of any of them lands, matched by its
/// `RequestId`. The list's fetch is never touched by it, so a page or
/// refetch in flight still answers and the list is never turned into
/// `Failed` by a refused change. The next change adds a wait and takes the
/// shown line away; the screen's farewell drops everything to `Idle`, so a
/// refusal that arrives after the screen was left lands nowhere.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ChangeNote {
    /// No change is out and no refusal is shown.
    #[default]
    Idle,
    /// Changes went out as `requests`; the refusal of any will be shown.
    Waiting {
        requests: Vec<crate::ids::RequestId>,
    },
    /// A change was refused: `text` is shown; `waiting` are the changes
    /// still out.
    Refused {
        text: String,
        waiting: Vec<crate::ids::RequestId>,
    },
}

impl ChangeNote {
    fn held(&self) -> Vec<crate::ids::RequestId> {
        match self {
            Self::Idle => Vec::new(),
            Self::Waiting { requests } => requests.clone(),
            Self::Refused { waiting, .. } => waiting.clone(),
        }
    }

    /// A change goes out as `request`; any shown refusal goes, the changes
    /// already out stay waited for.
    pub fn begin(&mut self, request: crate::ids::RequestId) {
        let mut requests = self.held();
        requests.push(request);
        *self = Self::Waiting { requests };
    }

    /// `request` (a change) was refused; `None` when it never left. A
    /// refusal of a request the note does not hold lands nowhere.
    pub fn refused(&mut self, request: Option<crate::ids::RequestId>, line: impl Into<String>) {
        let mut waiting = self.held();
        if let Some(asked) = request {
            let before = waiting.len();
            waiting.retain(|id| *id != asked);
            if waiting.len() == before {
                return;
            }
        }
        *self = Self::Refused {
            text: line.into(),
            waiting,
        };
    }

    /// The line to show, if a change was refused.
    pub fn line(&self) -> Option<&str> {
        match self {
            Self::Refused { text, .. } => Some(text.as_str()),
            _ => None,
        }
    }

    /// The owner dismisses the line or its screen is left: every wait goes.
    pub fn clear(&mut self) {
        *self = Self::Idle;
    }
}

/// The state of a fetched `T`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Fetch<T> {
    /// Never asked.
    #[default]
    Idle,
    /// A request is in flight; `stale` is the last answer, if any.
    /// `request` is the one that asked, recorded when the stage begins
    /// ([`Fetch::begin_with`]): only its own answer or error acts through
    /// [`Fetch::answered`] and [`Fetch::failed`].
    Loading {
        stale: Option<T>,
        request: crate::ids::RequestId,
    },
    /// The latest request answered.
    Ready(T),
    /// A change landed after the answer: the old answer, if any, stays
    /// visible until the refetch replaces it (never optimistic).
    Outdated(Option<T>),
    /// The latest request failed; `stale` is the last answer, if any.
    Failed { error: String, stale: Option<T> },
}

impl<T> Fetch<T> {
    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Whether a once-per-session request may go out: never asked, or the
    /// last try failed. A request in flight or an answer is not asked again.
    pub fn can_ask(&self) -> bool {
        matches!(self, Self::Idle | Self::Failed { .. })
    }

    /// The answer is out of date and a refetch is due.
    pub fn is_outdated(&self) -> bool {
        matches!(self, Self::Outdated(_))
    }

    /// A change landed: what is held stays visible and is asked for
    /// again, also when nothing was held (the change needs the answer). A
    /// request in flight asked before the change, so it is dropped with its
    /// mark: a first ask with nothing held becomes `Outdated(None)` too,
    /// whose answer would otherwise land without the change. The owner that
    /// refetches asks again.
    pub fn outdate(&mut self) {
        *self = Self::Outdated(self.take_data());
    }

    /// Something is being waited for: a request in flight, or a refetch due
    /// with nothing held to show meanwhile. What draws a spinner.
    pub fn shows_loading(&self) -> bool {
        matches!(self, Self::Loading { .. } | Self::Outdated(None))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// The error line of a failed request.
    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    /// The newest answer held: the current one, or the one a refetch or a
    /// failure left in place.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held.as_ref(),
        }
    }

    pub fn data_mut(&mut self) -> Option<&mut T> {
        match self {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held.as_mut(),
        }
    }

    /// The held data, moving the state to `Idle`.
    pub fn take_data(&mut self) -> Option<T> {
        match std::mem::take(self) {
            Self::Idle => None,
            Self::Ready(value) => Some(value),
            Self::Outdated(held)
            | Self::Loading { stale: held, .. }
            | Self::Failed { stale: held, .. } => held,
        }
    }

    /// Create the data with `make` when there is none. The state otherwise
    /// stays as it is: an optimistic local edit does not end a request in
    /// flight.
    pub fn insert_data_if_missing(&mut self, make: impl FnOnce() -> T) {
        if self.data().is_some() {
            return;
        }
        *self = match std::mem::take(self) {
            Self::Loading { request, .. } => Self::Loading {
                stale: Some(make()),
                request,
            },
            Self::Failed { error, .. } => Self::Failed {
                error,
                stale: Some(make()),
            },
            Self::Outdated(_) => Self::Outdated(Some(make())),
            Self::Idle | Self::Ready(_) => Self::Ready(make()),
        };
    }

    /// Whether a request should go out now: never asked, or a refetch is
    /// owed. A request in flight, an answer and a failure wait.
    pub fn wants_fetch(&self) -> bool {
        matches!(self, Self::Idle | Self::Outdated(_))
    }

    /// A request goes out as `request` (registered before this call, so the
    /// stage never reads Loading with nothing to wait for). The last answer
    /// stays visible, the last error is gone.
    pub fn begin_with(&mut self, request: crate::ids::RequestId) {
        let stale = self.take_data();
        *self = Self::Loading { stale, request };
    }

    /// The fetch waits for the answer of `request`.
    pub fn waits_for(&self, request: crate::ids::RequestId) -> bool {
        matches!(self, Self::Loading { request: asked, .. } if *asked == request)
    }

    /// `request` answered: taken only by the fetch that waits for it
    /// (`false` and no change for an older request, or after the fetch was
    /// dropped for a new one).
    pub fn answered(&mut self, request: Option<crate::ids::RequestId>, value: T) -> bool {
        if request.is_some_and(|request| self.waits_for(request)) {
            *self = Self::Ready(value);
            true
        } else {
            false
        }
    }

    /// `request` failed: only the fetch that waits for it fails. The held
    /// data goes when `drop_data`.
    pub fn failed(
        &mut self,
        request: Option<crate::ids::RequestId>,
        error: impl Into<String>,
        drop_data: bool,
    ) -> bool {
        if !request.is_some_and(|request| self.waits_for(request)) {
            return false;
        }
        if drop_data {
            self.fail_dropping_data(error);
        } else {
            self.fail(error);
        }
        true
    }

    /// The request answered, unless a change outdated the list meanwhile:
    /// that answer predates the change, so it is dropped and the refetch
    /// that follows asks again. `false` when it was dropped.
    pub fn answer(&mut self, value: T) -> bool {
        if self.is_outdated() {
            return false;
        }
        *self = Self::Ready(value);
        true
    }

    /// The request failed, unless a change outdated the list meanwhile (the
    /// refetch that follows asks again). `false` when it was dropped.
    pub fn answer_failed(&mut self, error: impl Into<String>) -> bool {
        if self.is_outdated() {
            return false;
        }
        self.fail(error);
        true
    }

    /// An answer that names no request: a pushed update, or a fetch whose
    /// sender does not record its request ([`Fetch::begin_with`]). An older request's
    /// late answer lands on a newer ask through this, so a sender prefers
    /// [`Fetch::answered`]; `scripts/check-request-match.py` counts the callers
    /// that remain in its baseline and fails a new one.
    pub fn succeed(&mut self, value: T) {
        *self = Self::Ready(value);
    }

    /// The server pushed the value (an update, or the truth after a change
    /// that needs no fetch): it is not the answer to any request, so it
    /// replaces whatever stage the fetch was in. A request still in flight
    /// finds `Ready` and is dropped by [`Fetch::answered`].
    pub fn pushed(&mut self, value: T) {
        *self = Self::Ready(value);
    }

    /// A failure that names no request (see [`Fetch::succeed`]); a sender
    /// prefers [`Fetch::failed`]. The last answer stays visible.
    pub fn fail(&mut self, error: impl Into<String>) {
        let stale = self.take_data();
        *self = Self::Failed {
            error: error.into(),
            stale,
        };
    }

    /// The request failed and what was held is wrong now.
    pub fn fail_dropping_data(&mut self, error: impl Into<String>) {
        *self = Self::Failed {
            error: error.into(),
            stale: None,
        };
    }

    /// The request never left (a send error that is not a result): back to
    /// what was held before it.
    pub fn cancel(&mut self) {
        if self.is_loading() {
            *self = match self.take_data() {
                Some(value) => Self::Ready(value),
                None => Self::Idle,
            };
        }
    }

    /// Forget the last error, keeping the data. A request in flight or an
    /// answer is left alone.
    pub fn clear_error(&mut self) {
        if matches!(self, Self::Failed { .. }) {
            *self = match self.take_data() {
                Some(value) => Self::Ready(value),
                None => Self::Idle,
            };
        }
    }
}

impl<T> Fetch<Vec<T>> {
    /// The newest list held, empty when there is none.
    pub fn items(&self) -> &[T] {
        self.data().map(Vec::as_slice).unwrap_or(&[])
    }
}

/// Several requests that answer one question (a peer's TON and Stars
/// balances, the gift limit options): asked together, settled when every
/// one has answered, failed when any send or answer fails. A failed batch
/// is never asked again by itself; its owner forgets it at a transition
/// (an open) so the next one asks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BatchAsk {
    #[default]
    Unasked,
    /// These requests are out. Each answer takes its own off.
    Asking {
        waiting: Vec<crate::ids::RequestId>,
    },
    Answered,
    Failed,
}

impl BatchAsk {
    /// The batch after its sends: `Failed` when one was refused, else
    /// `Asking` the requests that went out (`Answered` when none were
    /// needed).
    pub fn sent(waiting: Vec<crate::ids::RequestId>, refused: bool) -> Self {
        if refused {
            Self::Failed
        } else if waiting.is_empty() {
            Self::Answered
        } else {
            Self::Asking { waiting }
        }
    }

    /// Nothing is out, so the owner may ask: never asked, or failed since.
    pub fn may_ask(&self) -> bool {
        matches!(self, Self::Unasked | Self::Failed)
    }

    /// `request` answered: the batch is `Answered` once none is left.
    pub fn settle(&mut self, request: Option<crate::ids::RequestId>) {
        let Self::Asking { waiting } = self else {
            return;
        };
        waiting.retain(|id| Some(*id) != request);
        if waiting.is_empty() {
            *self = Self::Answered;
        }
    }

    /// `request` failed: the batch fails when it was one of its own.
    pub fn fail(&mut self, request: Option<crate::ids::RequestId>) {
        if let Self::Asking { waiting } = self
            && request.is_some_and(|id| waiting.contains(&id))
        {
            *self = Self::Failed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BatchAsk, Fetch};
    use crate::ids::RequestId;

    #[test]
    fn a_batch_settles_when_every_request_answered_and_fails_on_one() {
        let mut batch = BatchAsk::sent(vec![RequestId(1), RequestId(2)], false);
        assert!(!batch.may_ask());
        batch.settle(Some(RequestId(1)));
        assert!(matches!(batch, BatchAsk::Asking { .. }));
        batch.settle(Some(RequestId(2)));
        assert_eq!(batch, BatchAsk::Answered);

        let mut batch = BatchAsk::sent(vec![RequestId(3), RequestId(4)], false);
        batch.fail(Some(RequestId(9)));
        assert!(matches!(batch, BatchAsk::Asking { .. }), "not its request");
        batch.fail(Some(RequestId(4)));
        assert_eq!(batch, BatchAsk::Failed);
        assert!(batch.may_ask());
        assert_eq!(BatchAsk::sent(Vec::new(), true), BatchAsk::Failed);
    }

    #[test]
    fn a_fresh_fetch_is_idle_with_nothing() {
        let fetch: Fetch<Vec<i32>> = Fetch::default();
        assert!(fetch.is_idle());
        assert!(!fetch.is_loading());
        assert_eq!(fetch.data(), None);
        assert_eq!(fetch.error(), None);
    }

    #[test]
    fn loading_keeps_the_last_answer_and_drops_the_error() {
        let mut fetch = Fetch::Failed {
            error: "no network".to_string(),
            stale: Some(vec![1]),
        };
        fetch.begin_with(crate::ids::RequestId(7004));
        assert!(fetch.is_loading());
        assert_eq!(fetch.data(), Some(&vec![1]));
        assert_eq!(fetch.error(), None);
        fetch.succeed(vec![2]);
        assert_eq!(fetch, Fetch::Ready(vec![2]));
    }

    #[test]
    fn failing_keeps_the_last_answer() {
        let mut fetch = Fetch::Ready(vec![1]);
        fetch.begin_with(crate::ids::RequestId(7005));
        fetch.fail("timeout");
        assert_eq!(fetch.error(), Some("timeout"));
        assert_eq!(fetch.data(), Some(&vec![1]));
        assert!(!fetch.is_loading());
    }

    #[test]
    fn failing_can_drop_the_data() {
        let mut fetch = Fetch::Ready(vec![1]);
        fetch.fail_dropping_data("bad");
        assert_eq!(fetch.error(), Some("bad"));
        assert_eq!(fetch.data(), None);
    }

    #[test]
    fn cancel_returns_to_what_was_held() {
        let mut empty: Fetch<i32> = Fetch::Idle;
        empty.begin_with(crate::ids::RequestId(7006));
        empty.cancel();
        assert!(empty.is_idle());
        let mut held = Fetch::Ready(5);
        held.begin_with(crate::ids::RequestId(7007));
        held.cancel();
        assert_eq!(held, Fetch::Ready(5));
        // Not loading: nothing to cancel.
        let mut failed: Fetch<i32> = Fetch::Failed {
            error: "x".into(),
            stale: None,
        };
        failed.cancel();
        assert_eq!(failed.error(), Some("x"));
    }

    #[test]
    fn clearing_the_error_keeps_data_and_in_flight_requests() {
        let mut failed = Fetch::Failed {
            error: "x".to_string(),
            stale: Some(3),
        };
        failed.clear_error();
        assert_eq!(failed, Fetch::Ready(3));
        let mut bare: Fetch<i32> = Fetch::Failed {
            error: "x".into(),
            stale: None,
        };
        bare.clear_error();
        assert!(bare.is_idle());
        let mut loading: Fetch<i32> = Fetch::Loading {
            stale: None,
            request: RequestId(1),
        };
        loading.clear_error();
        assert!(loading.is_loading());
    }

    #[test]
    fn inserting_data_keeps_the_request_state() {
        let mut idle: Fetch<Vec<i32>> = Fetch::Idle;
        idle.insert_data_if_missing(Vec::new);
        idle.data_mut().into_iter().for_each(|list| list.push(1));
        assert_eq!(idle, Fetch::Ready(vec![1]));

        let mut loading: Fetch<Vec<i32>> = Fetch::Loading {
            stale: None,
            request: RequestId(1),
        };
        loading.insert_data_if_missing(Vec::new);
        loading.data_mut().into_iter().for_each(|list| list.push(2));
        assert_eq!(
            loading,
            Fetch::Loading {
                stale: Some(vec![2]),
                request: RequestId(1),
            }
        );

        let mut failed: Fetch<Vec<i32>> = Fetch::Failed {
            error: "e".into(),
            stale: None,
        };
        failed.insert_data_if_missing(Vec::new);
        failed.data_mut().into_iter().for_each(|list| list.push(3));
        assert_eq!(failed.error(), Some("e"));
        assert_eq!(failed.data(), Some(&vec![3]));

        let mut ready = Fetch::Ready(vec![4]);
        ready.insert_data_if_missing(Vec::new);
        ready.data_mut().into_iter().for_each(|list| list.push(5));
        assert_eq!(ready, Fetch::Ready(vec![4, 5]));
    }

    #[test]
    fn take_data_leaves_idle() {
        let mut fetch = Fetch::Ready(7);
        assert_eq!(fetch.take_data(), Some(7));
        assert!(fetch.is_idle());
    }
}
