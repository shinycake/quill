//! Where a chat list's paging stands (`loadChats` for the main list and the
//! archive).
use crate::ids::RequestId;
use std::time::{Duration, Instant};

/// The stage of one chat list's paging, one value instead of an
/// "exhausted" flag beside a retry slot.
///
/// `Retrying` leaves on the retry's page: its answer (`More`), a 404
/// (`Exhausted`) or another failure (`Retrying` again, with a doubled
/// wait). `Exhausted` leaves when the account's chat lists reset (a new
/// authorization or a log-out).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ListPaging {
    /// Pages remain; none is out and none is waiting out a failure.
    #[default]
    More,
    /// A page is out as `request`; `after` is the wait of the failure before
    /// it, so the next failure doubles it. The answer or the error of that
    /// request leaves the stage; no other does.
    Loading {
        request: RequestId,
        after: Option<Duration>,
    },
    /// A page failed (not a 404): no page before `at`, and `wait` is the
    /// wait that was applied, which doubles per failure. The driver's tick
    /// asks again then (`ConnectDriver::chat_list_tick`).
    Retrying { at: Instant, wait: Duration },
    /// `loadChats` answered 404: the list is fully loaded.
    Exhausted,
}

impl ListPaging {
    const FIRST_WAIT: Duration = Duration::from_secs(1);
    const MAX_WAIT: Duration = Duration::from_secs(300);

    pub fn is_exhausted(&self) -> bool {
        matches!(self, Self::Exhausted)
    }

    /// A page is out.
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// The wait after a failed page, while there is one.
    pub fn retry(&self) -> Option<(Instant, Duration)> {
        match self {
            Self::Retrying { at, wait } => Some((*at, *wait)),
            _ => None,
        }
    }

    /// Whether the wait after a failed page is over at `now`.
    pub fn retry_due(&self, now: Instant) -> bool {
        self.retry().is_some_and(|(at, _)| now >= at)
    }

    /// A page went out as `request` (a list that ended stays ended).
    pub fn begin(&mut self, request: RequestId) {
        *self = match *self {
            Self::More => Self::Loading {
                request,
                after: None,
            },
            Self::Retrying { wait, .. } => Self::Loading {
                request,
                after: Some(wait),
            },
            Self::Loading { after, .. } => Self::Loading { request, after },
            Self::Exhausted => Self::Exhausted,
        };
    }

    /// `request` is the page this list waits for.
    pub fn waits_for(&self, asked: Option<RequestId>) -> bool {
        matches!(self, Self::Loading { request, .. } if Some(*request) == asked)
    }

    /// `request` went through: an earlier failure's wait is over.
    pub fn page_ok(&mut self, asked: Option<RequestId>) {
        if self.waits_for(asked) {
            *self = Self::More;
        }
    }

    /// The user left the list: a retry waiting out a failed page stops.
    pub fn cancel_retry(&mut self) {
        if matches!(self, Self::Retrying { .. }) {
            *self = Self::More;
        }
    }

    /// `request` failed with `code`: a 404 ends the list, anything else (a
    /// timeout, a 500) waits one second, doubling per failure up to five
    /// minutes, and asks again. Only the page the list waits for counts.
    pub fn page_failed(&mut self, code: i32, now: Instant, asked: Option<RequestId>) {
        let Self::Loading { after, .. } = *self else {
            return;
        };
        if !self.waits_for(asked) {
            return;
        }
        if code == 404 {
            *self = Self::Exhausted;
            return;
        }
        let wait = after.map_or(Self::FIRST_WAIT, |wait| {
            wait.saturating_mul(2).min(Self::MAX_WAIT)
        });
        *self = Self::Retrying {
            at: now.checked_add(wait).unwrap_or(now),
            wait,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(paging: &mut ListPaging, id: u64) -> Option<RequestId> {
        let request = RequestId(id);
        paging.begin(request);
        Some(request)
    }

    #[test]
    fn a_failed_page_waits_longer_each_time_and_a_page_ends_the_wait() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        let asked = out(&mut paging, 1);
        paging.page_failed(500, now, asked);
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(1))
        );
        assert!(!paging.retry_due(now));
        assert!(paging.retry_due(now + Duration::from_secs(1)));
        let asked = out(&mut paging, 2);
        paging.page_failed(500, now, asked);
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(2))
        );
        let asked = out(&mut paging, 3);
        paging.page_ok(asked);
        assert_eq!(paging, ListPaging::More);
    }

    #[test]
    fn a_404_ends_the_list_and_nothing_but_a_reset_reopens_it() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        let asked = out(&mut paging, 1);
        paging.page_failed(500, now, asked);
        let asked = out(&mut paging, 2);
        paging.page_failed(404, now, asked);
        assert!(paging.is_exhausted());
        assert!(paging.retry().is_none());
        paging.begin(RequestId(3));
        paging.page_failed(500, now, Some(RequestId(3)));
        paging.page_ok(Some(RequestId(3)));
        assert!(paging.is_exhausted());
    }

    #[test]
    fn the_wait_stops_doubling_at_five_minutes() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        for id in 0..20 {
            let asked = out(&mut paging, id);
            paging.page_failed(500, now, asked);
        }
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(300))
        );
    }

    #[test]
    fn only_the_page_the_list_waits_for_answers_it() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        paging.begin(RequestId(2));
        paging.page_failed(500, now, Some(RequestId(1)));
        paging.page_ok(Some(RequestId(1)));
        assert!(
            paging.is_loading(),
            "an older page's answer changes nothing"
        );
        paging.page_ok(Some(RequestId(2)));
        assert_eq!(paging, ListPaging::More);
    }
}
