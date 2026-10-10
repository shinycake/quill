//! Connection status, the status line and presence sync.

use super::connect_ui::ConnectUiStatus;

pub(crate) struct ConnectionUi {
    pub(super) status: ConnectUiStatus,
    pub(super) generation: u64,
    /// The TDLib receive bridge stopped; shows the Closed / Retry card.
    pub(super) lost: bool,
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
            lost: false,
            status_note,
            status_seen: String::new(),
            status_shown_at: None,
            status_traced: String::new(),
            presence: Default::default(),
        }
    }
}
