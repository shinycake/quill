//! The proxy list's requests: the `getProxies` its list waits for and the
//! one change in flight, as one value (split out of [`crate::proxy`]).
use crate::ids::RequestId;

/// Where the proxy list's `getProxies` is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProxyFetch {
    /// No `getProxies` has been sent yet: the first one goes out.
    #[default]
    NotAsked,
    /// The last one answered or failed (a failed one is not retried in a
    /// tight loop: the box's error line says so).
    Idle,
    /// The list changed under us (an add / edit / enable / remove
    /// succeeded, dropping any fetch out, whose answer may predate the
    /// change): the authoritative list is asked for again.
    Stale,
    /// This request is the one whose answer replaces the list.
    Loading(RequestId),
    /// The request could not be sent (or its answer was an error): nothing
    /// asks again by itself, only the dialog's open / Refresh (`Retry`).
    Failed,
    /// The refetch of a stale list could not be sent. Like `Failed`, nothing
    /// asks again by itself, but the held list is still out of date, so the
    /// state stays busy and nothing plans on it.
    FailedStale,
}

impl ProxyFetch {
    pub fn loading(self) -> bool {
        matches!(self, Self::Loading(_))
    }

    /// The list is out of date and no request is on its way for it.
    pub fn stale(self) -> bool {
        self == Self::Stale
    }

    /// A `getProxies` should go out: never asked, or invalidated.
    pub fn wanted(self) -> bool {
        matches!(self, Self::NotAsked | Self::Stale)
    }

    /// `request` is the fetch the list waits for.
    pub fn answers(self, request: RequestId) -> bool {
        self == Self::Loading(request)
    }
}

/// The proxy list's two requests: the `getProxies` its list waits for and
/// the one change (`addProxy`, `editProxy`, `enableProxy`, ...) in flight.
/// They are one value because a change's success outdates any fetch still
/// out (its answer may predate the change) and owes a new one: every
/// transition is a method here, so no reducer writes one half alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProxyListOps {
    fetch: ProxyFetch,
    change: Option<RequestId>,
}

impl ProxyListOps {
    pub fn fetch(&self) -> ProxyFetch {
        self.fetch
    }

    /// The list is changing or about to: a change is out, or a fetch is
    /// owed or on its way.
    pub fn busy(&self) -> bool {
        self.changing()
            || self.fetch.loading()
            || self.fetch.stale()
            || self.fetch == ProxyFetch::FailedStale
    }

    /// A change is in flight.
    pub fn changing(&self) -> bool {
        self.change.is_some()
    }

    /// A `getProxies` should go out: never asked, or invalidated.
    pub fn wanted(&self) -> bool {
        self.fetch.wanted()
    }

    /// The list is out of date and no request is on its way for it.
    pub fn stale(&self) -> bool {
        self.fetch.stale()
    }

    pub fn loading(&self) -> bool {
        self.fetch.loading()
    }

    /// The fetch in flight, if one is.
    pub fn fetch_out(&self) -> Option<RequestId> {
        match self.fetch {
            ProxyFetch::Loading(request) => Some(request),
            _ => None,
        }
    }

    /// Mark the list out of date: the next tick fetches it again. A fetch
    /// out is dropped (its answer may predate whatever outdated the list).
    pub fn invalidate(&mut self) {
        self.fetch = ProxyFetch::Stale;
    }

    /// `getProxies` went out as `request`.
    pub fn fetch_sent(&mut self, request: RequestId) {
        self.fetch = ProxyFetch::Loading(request);
    }

    /// `getProxies` could not be sent: nothing asks again by itself, only
    /// the dialog's open / Refresh. A list that was owed a refetch stays
    /// busy (`FailedStale`).
    pub fn fetch_unsent(&mut self) {
        self.fetch = match self.fetch {
            ProxyFetch::Stale | ProxyFetch::FailedStale => ProxyFetch::FailedStale,
            _ => ProxyFetch::Failed,
        };
    }

    /// The answer of `request`: whether it is the fetch the list waits for
    /// (an older one says nothing about the list as it is now).
    pub fn fetch_answered(&mut self, request: RequestId) -> bool {
        let is_it = self.fetch == ProxyFetch::Loading(request);
        if is_it {
            self.fetch = ProxyFetch::Idle;
        }
        is_it
    }

    /// The error of `request`: whether it is the fetch the list waits for.
    pub fn fetch_failed(&mut self, request: RequestId) -> bool {
        let is_it = self.fetch == ProxyFetch::Loading(request);
        if is_it {
            self.fetch = ProxyFetch::Failed;
        }
        is_it
    }

    /// A change went out as `request`.
    pub fn change_sent(&mut self, request: RequestId) {
        self.change = Some(request);
    }

    /// The change `request` succeeded (`None`: an answer with no request
    /// record): the list is out of date. Only the change in flight unlocks
    /// the list; an older one's answer leaves the lock to its own.
    pub fn change_succeeded(&mut self, request: Option<RequestId>) {
        self.change_ended(request);
        self.fetch = ProxyFetch::Stale;
    }

    /// The change `request` failed: the list stays as it was.
    pub fn change_failed(&mut self, request: Option<RequestId>) {
        self.change_ended(request);
    }

    fn change_ended(&mut self, request: Option<RequestId>) {
        if request.is_none() || self.change == request {
            self.change = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_ops_move_together() {
        let mut ops = ProxyListOps::default();
        assert!(ops.wanted() && !ops.busy());
        ops.fetch_sent(RequestId(1));
        ops.change_sent(RequestId(2));
        assert!(ops.busy() && ops.changing() && ops.loading());
        // An older change's success leaves the lock to the change in flight,
        // but still outdates the fetch.
        ops.change_succeeded(Some(RequestId(9)));
        assert!(ops.changing() && ops.stale() && !ops.loading());
        assert!(!ops.fetch_answered(RequestId(1)), "dropped with the change");
        ops.change_failed(Some(RequestId(2)));
        assert!(!ops.changing());
        // A refetch that cannot be sent keeps the list busy, not owed.
        ops.fetch_unsent();
        assert_eq!(ops.fetch(), ProxyFetch::FailedStale);
        assert!(ops.busy() && !ops.wanted());
        ops.fetch_sent(RequestId(3));
        assert!(ops.fetch_failed(RequestId(3)));
        assert_eq!(ops.fetch(), ProxyFetch::Failed);
    }
}
