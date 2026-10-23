//! How the Recent actions list follows the loaded events. The log shows the
//! oldest event first and the newest last, like a chat, so a page of older
//! events joins at the top and a poll's new events at the bottom. The
//! scrolling list is told which of the two happened so the reader keeps the
//! place they were at.

/// What changed between the events the list was built for and the events
/// now loaded (both oldest first, as event ids).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListSync {
    /// The same events.
    Keep,
    /// Something else (another filter, a search): start over.
    Reset,
    /// The earlier events are still there, with `older` more before them
    /// and `newer` more after them.
    Grow { older: usize, newer: usize },
}

pub fn plan_sync(shown: &[i64], now: &[i64]) -> ListSync {
    if shown == now {
        return ListSync::Keep;
    }
    let Some(first) = shown.first() else {
        return ListSync::Reset;
    };
    let Some(start) = now.iter().position(|id| id == first) else {
        return ListSync::Reset;
    };
    match now.get(start..) {
        Some(rest) if rest.starts_with(shown) => ListSync::Grow {
            older: start,
            newer: rest.len().saturating_sub(shown.len()),
        },
        _ => ListSync::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_events_keep_the_list() {
        assert_eq!(plan_sync(&[1, 2, 3], &[1, 2, 3]), ListSync::Keep);
        assert_eq!(plan_sync(&[], &[]), ListSync::Keep);
    }

    #[test]
    fn older_page_joins_at_the_top() {
        assert_eq!(
            plan_sync(&[4, 5], &[1, 2, 3, 4, 5]),
            ListSync::Grow { older: 3, newer: 0 }
        );
    }

    #[test]
    fn polled_events_join_at_the_bottom() {
        assert_eq!(
            plan_sync(&[1, 2], &[1, 2, 3]),
            ListSync::Grow { older: 0, newer: 1 }
        );
    }

    #[test]
    fn both_ends_can_grow_at_once() {
        assert_eq!(
            plan_sync(&[3, 4], &[1, 2, 3, 4, 5]),
            ListSync::Grow { older: 2, newer: 1 }
        );
    }

    #[test]
    fn a_different_set_starts_over() {
        assert_eq!(plan_sync(&[1, 2], &[7, 8]), ListSync::Reset);
        assert_eq!(plan_sync(&[], &[7, 8]), ListSync::Reset);
        assert_eq!(plan_sync(&[1, 2], &[1, 3]), ListSync::Reset);
        assert_eq!(plan_sync(&[1, 2], &[]), ListSync::Reset);
    }
}
