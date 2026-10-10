//! Copy and rules of "Clear all" on the Calls list (pure, no UI).
//!
//! Telegram Desktop's `ClearCallsBox` (`calls/calls_box_controller.cpp`)
//! asks "Are you sure you want to completely clear your calls log?" with a
//! "Delete for everyone" checkbox and a Clear / Cancel pair. TDLib does
//! the same with one request, `deleteAllCallMessages(revoke)`.

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

#[cfg(test)]
mod tests {
    use super::*;

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
