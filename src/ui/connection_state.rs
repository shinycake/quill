//! Connection status, the status line and presence sync.
#![deny(clippy::indexing_slicing)]

use super::connect_ui::ConnectUiStatus;

pub(crate) struct ConnectionUi {
    pub(super) status: ConnectUiStatus,
    pub(super) generation: u64,
    pub(super) status_note: String,
    /// The `status_note` text the toast last showed, and when it appeared:
    /// a changed note restarts the toast's timer.
    pub(super) status_seen: String,
    pub(super) status_shown_at: Option<std::time::Instant>,
    /// The last status note printed by `QUILL_TRACE_STATUS`.
    pub(super) status_traced: String,
    /// Batch 4: the last `online` value sent to TDLib.
    pub(super) presence: quill::presence::PresenceSync,
}

impl ConnectionUi {
    pub(super) fn new(connect_status: ConnectUiStatus, status_note: String) -> Self {
        Self {
            status: connect_status,
            generation: 0,
            status_note,
            status_seen: String::new(),
            status_shown_at: None,
            status_traced: String::new(),
            presence: Default::default(),
        }
    }
}

/// The authorization state the window draws: a receive bridge that stopped
/// on its own counts as an unexpected Closed (the Retry card), whatever the
/// session last said. A pure function of the bridge and the session, so a
/// restart or account switch (a new bridge) needs no flag reset.
pub(super) fn auth_for_bridge(
    bridge_stopped: bool,
    session_auth: quill::telegram::envelope::AuthorizationState,
) -> quill::telegram::envelope::AuthorizationState {
    use quill::telegram::envelope::AuthorizationState;
    if bridge_stopped {
        AuthorizationState::Closed
    } else {
        session_auth
    }
}

#[cfg(test)]
mod tests {
    use super::auth_for_bridge;
    use quill::telegram::envelope::AuthorizationState;

    #[test]
    fn a_stopped_bridge_shows_closed_and_a_fresh_one_shows_the_session() {
        assert_eq!(
            auth_for_bridge(true, AuthorizationState::Ready),
            AuthorizationState::Closed
        );
        assert_eq!(
            auth_for_bridge(false, AuthorizationState::Ready),
            AuthorizationState::Ready
        );
    }
}
