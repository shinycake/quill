//! The proxy list's requests: the `getProxies` its list waits for and the
//! one change in flight, as one value (split out of [`crate::proxy`]).
use crate::ids::RequestId;
use std::time::{Duration, Instant};

/// How long a `getProxies` that could not be sent waits before the next
/// ingest sends it again.
pub const UNSENT_RETRY_AFTER: Duration = Duration::from_secs(5);

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
    /// The request's answer was an error: nothing asks again by itself,
    /// only the dialog's open / Refresh (`Retry`); the box's error line
    /// says so.
    Failed,
    /// The request could not be sent (the client is closing, a native
    /// error) at `at`: the next ingest after [`UNSENT_RETRY_AFTER`] sends
    /// it again, so a background account or a box nobody opens still
    /// settles.
    NotSent { at: Instant },
    /// The refetch of a stale list could not be sent at `at`. Retried like
    /// `NotSent`, but the held list is still out of date, so the state stays
    /// busy and nothing plans on it.
    NotSentStale { at: Instant },
}

impl ProxyFetch {
    pub fn loading(self) -> bool {
        matches!(self, Self::Loading(_))
    }

    /// The list is out of date and no request is on its way for it.
    pub fn stale(self) -> bool {
        self == Self::Stale
    }

    /// A `getProxies` should go out at `now`: never asked, invalidated, or
    /// one that could not be sent has waited out its retry.
    pub fn wanted(self, now: Instant) -> bool {
        match self {
            Self::NotAsked | Self::Stale => true,
            Self::NotSent { at } | Self::NotSentStale { at } => {
                now.saturating_duration_since(at) >= UNSENT_RETRY_AFTER
            }
            _ => false,
        }
    }

    /// The last send was refused (a retry is owed).
    pub fn unsent(self) -> bool {
        matches!(self, Self::NotSent { .. } | Self::NotSentStale { .. })
    }

    /// `request` is the fetch the list waits for.
    pub fn answers(self, request: RequestId) -> bool {
        self == Self::Loading(request)
    }
}

/// Where the `prefer_ipv6` option's write is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Ipv6Set {
    #[default]
    Idle,
    /// `setOption("prefer_ipv6", on)` is in flight as `request`; only its
    /// answer or error leaves the stage.
    Setting { on: bool, request: RequestId },
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
    ipv6: Ipv6Set,
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
            || matches!(self.fetch, ProxyFetch::NotSentStale { .. })
    }

    /// A change is in flight.
    pub fn changing(&self) -> bool {
        self.change.is_some()
    }

    /// A `getProxies` should go out at `now`.
    pub fn wanted(&self, now: Instant) -> bool {
        self.fetch.wanted(now)
    }

    /// A fetch should go out because it is owed or its last send was
    /// refused and has waited out its retry (what `ingest` checks).
    pub fn refetch_due(&self, now: Instant) -> bool {
        (self.fetch.stale() || self.fetch.unsent()) && self.fetch.wanted(now)
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

    /// `getProxies` could not be sent at `now`: the next ingest after the
    /// retry wait sends it again. A list that was owed a refetch stays busy
    /// (`NotSentStale`).
    pub fn fetch_unsent(&mut self, now: Instant) {
        self.fetch = match self.fetch {
            ProxyFetch::Stale | ProxyFetch::NotSentStale { .. } => {
                ProxyFetch::NotSentStale { at: now }
            }
            _ => ProxyFetch::NotSent { at: now },
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

    /// Writing the IPv6 option to `on` went out as `request`.
    pub fn ipv6_sent(&mut self, on: bool, request: RequestId) {
        self.ipv6 = Ipv6Set::Setting { on, request };
    }

    /// A write of the IPv6 option to `on` is in flight.
    pub fn ipv6_setting(&self, on: bool) -> bool {
        matches!(self.ipv6, Ipv6Set::Setting { on: asked, .. } if asked == on)
    }

    /// The IPv6 write `request` came back (answer or error). Only the write
    /// in flight leaves it.
    pub fn ipv6_ended(&mut self, request: Option<RequestId>) {
        if let Ipv6Set::Setting { request: asked, .. } = self.ipv6
            && request == Some(asked)
        {
            self.ipv6 = Ipv6Set::Idle;
        }
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
        let now = Instant::now();
        let mut ops = ProxyListOps::default();
        assert!(ops.wanted(now) && !ops.busy());
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
        // A refetch that cannot be sent keeps the list busy until it is
        // asked again after the wait.
        ops.fetch_unsent(now);
        assert_eq!(ops.fetch(), ProxyFetch::NotSentStale { at: now });
        assert!(ops.busy() && !ops.wanted(now));
        assert!(!ops.refetch_due(now + UNSENT_RETRY_AFTER / 2));
        assert!(ops.refetch_due(now + UNSENT_RETRY_AFTER));
        ops.fetch_sent(RequestId(3));
        assert!(ops.fetch_failed(RequestId(3)));
        assert_eq!(ops.fetch(), ProxyFetch::Failed);
        assert!(
            !ops.refetch_due(now + UNSENT_RETRY_AFTER * 9),
            "an error answer waits for Refresh"
        );
    }

    #[test]
    fn an_ipv6_write_is_one_stage_left_by_its_own_answer() {
        let mut ops = ProxyListOps::default();
        ops.ipv6_sent(true, RequestId(4));
        assert!(ops.ipv6_setting(true) && !ops.ipv6_setting(false));
        ops.ipv6_ended(Some(RequestId(3)));
        assert!(ops.ipv6_setting(true), "an older answer leaves it");
        ops.ipv6_ended(Some(RequestId(4)));
        assert!(!ops.ipv6_setting(true));
    }

    #[test]
    fn a_first_fetch_that_cannot_be_sent_is_asked_again_after_the_wait() {
        let now = Instant::now();
        let mut ops = ProxyListOps::default();
        ops.fetch_unsent(now);
        assert!(!ops.busy(), "no list is held, nothing is out of date");
        assert!(!ops.wanted(now));
        assert!(ops.refetch_due(now + UNSENT_RETRY_AFTER));
    }
}
