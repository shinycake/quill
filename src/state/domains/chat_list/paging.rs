//! Where a chat list's paging stands (`loadChats` for the main list and the
//! archive).
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
    /// Pages remain; none is waiting out a failure.
    #[default]
    More,
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

    /// A page went through: an earlier failure's wait is over.
    pub fn page_ok(&mut self) {
        if matches!(self, Self::Retrying { .. }) {
            *self = Self::More;
        }
    }

    /// A page failed with `code`: a 404 ends the list, anything else (a
    /// timeout, a 500) waits one second, doubling per failure up to five
    /// minutes, and asks again.
    pub fn page_failed(&mut self, code: i32, now: Instant) {
        if *self == Self::Exhausted {
            return;
        }
        if code == 404 {
            *self = Self::Exhausted;
            return;
        }
        let wait = self.retry().map_or(Self::FIRST_WAIT, |(_, wait)| {
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

    #[test]
    fn a_failed_page_waits_longer_each_time_and_a_page_ends_the_wait() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        paging.page_failed(500, now);
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(1))
        );
        assert!(!paging.retry_due(now));
        assert!(paging.retry_due(now + Duration::from_secs(1)));
        paging.page_failed(500, now);
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(2))
        );
        paging.page_ok();
        assert_eq!(paging, ListPaging::More);
    }

    #[test]
    fn a_404_ends_the_list_and_nothing_but_a_reset_reopens_it() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        paging.page_failed(500, now);
        paging.page_failed(404, now);
        assert!(paging.is_exhausted());
        assert!(paging.retry().is_none());
        paging.page_failed(500, now);
        paging.page_ok();
        assert!(paging.is_exhausted());
    }

    #[test]
    fn the_wait_stops_doubling_at_five_minutes() {
        let now = Instant::now();
        let mut paging = ListPaging::More;
        for _ in 0..20 {
            paging.page_failed(500, now);
        }
        assert_eq!(
            paging.retry().map(|(_, wait)| wait),
            Some(Duration::from_secs(300))
        );
    }
}
