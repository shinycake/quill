//! Copy and rules of "Clear all" on the Calls list (pure, no UI).
//!
//! Telegram Desktop's `ClearCallsBox` (`calls/calls_box_controller.cpp`)
//! asks "Are you sure you want to completely clear your calls log?" with a
//! "Delete for everyone" checkbox and a Clear / Cancel pair. TDLib does
//! the same with one request, `deleteAllCallMessages(revoke)`.
#![deny(clippy::indexing_slicing)]

use crate::lang::{keys, tr_with};
use crate::local_time::{CivilTime, hhmm, month_name, needs_year, short_date};

pub const CLEAR_TITLE: &str = "Clear calls";
pub const CLEAR_ABOUT: &str = "Are you sure you want to completely clear your calls log?";
pub const CLEAR_REVOKE_LABEL: &str = "Delete for everyone";
pub const CLEAR_BUTTON: &str = "Clear";
pub const CLEAR_ALL_LABEL: &str = "Clear all";

/// Whether the "Clear all" action is offered: there is something to
/// clear and no clear is already in flight.
pub fn can_clear(entry_count: usize, clearing: bool) -> bool {
    entry_count > 0 && !clearing
}

/// Toast after the server refuses the request.
pub fn clear_failed(code: i32) -> String {
    format!("could not clear the call history (error {code})")
}

/// The status line of a call row: "12:34" today, "yesterday at 12:34",
/// "April 15 at 12:34" (with the year past tdesktop's year rule), as
/// `Calls::BoxController::Row::refreshStatus` writes it.
pub fn call_status(date: &CivilTime, now: &CivilTime) -> String {
    let time = hhmm(date);
    match now.day_number() - date.day_number() {
        0 => tr_with(keys::CALL_BOX_STATUS_TODAY, &[("time", &time)]),
        1 => tr_with(keys::CALL_BOX_STATUS_YESTERDAY, &[("time", &time)]),
        _ => {
            let day = if needs_year(date, now) {
                short_date(date)
            } else {
                tr_with(
                    keys::MONTH_DAY,
                    &[
                        ("month", month_name(date.month)),
                        ("day", &date.day.to_string()),
                    ],
                )
            };
            tr_with(
                keys::CALL_BOX_STATUS_DATE,
                &[("date", &day), ("time", &time)],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_time::civil_at;

    #[test]
    fn a_call_today_shows_only_its_time() {
        let now = civil_at(1_760_000_000, 0);
        let earlier = civil_at(1_760_000_000 - 3_600, 0);
        assert_eq!(call_status(&earlier, &now), hhmm(&earlier));
    }

    #[test]
    fn older_calls_name_the_day() {
        let now = civil_at(1_760_000_000, 0);
        let yesterday = civil_at(1_760_000_000 - 86_400, 0);
        let older = civil_at(1_760_000_000 - 5 * 86_400, 0);
        assert_eq!(
            call_status(&yesterday, &now),
            format!("yesterday at {}", hhmm(&yesterday))
        );
        assert!(call_status(&older, &now).ends_with(&format!(" at {}", hhmm(&older))));
        assert!(call_status(&older, &now).starts_with(month_name(older.month)));
    }

    #[test]
    fn clear_is_offered_only_for_a_non_empty_idle_list() {
        assert!(can_clear(3, false));
        assert!(!can_clear(0, false));
        assert!(!can_clear(3, true));
    }

    #[test]
    fn failure_toast_counts_as_a_failure_note() {
        assert!(clear_failed(500).contains("could not"));
    }
}
