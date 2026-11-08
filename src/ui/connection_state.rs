//! Connection status, the status line and presence sync.
#![deny(clippy::indexing_slicing)]

use super::connect_ui::ConnectUiStatus;
use super::status_note::{NoteKind, StatusNote};
use super::toast_queue::ToastQueue;
use std::time::Instant;

pub(crate) struct ConnectionUi {
    pub(super) status: ConnectUiStatus,
    pub(super) generation: u64,
    /// The connection generation whose lost bridge already posted the
    /// "connection closed" note. A new connection bumps `generation`, which
    /// re-arms it; other notes cannot, so they are never overwritten again.
    closed_noted_generation: Option<u64>,
    /// The current note. Private: every write names its kind through
    /// `set_note`, so a toast never depends on the words.
    note: StatusNote,
    /// The toasts on screen and waiting, with the visible one's timer.
    pub(super) toasts: ToastQueue,
    /// The window a note set now toasts in (`None`: the main window): the
    /// one the last input happened in (`QuillApp::note_window_input`).
    pub(super) note_window: Option<quill::ids::ChatId>,
    /// The deadline a wake-up is already scheduled for.
    pub(super) toast_wake: Option<Instant>,
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
            closed_noted_generation: None,
            note: StatusNote::new(NoteKind::Silent, status_note),
            toasts: ToastQueue::default(),
            note_window: None,
            toast_wake: None,
            status_traced: String::new(),
            presence: Default::default(),
        }
    }
}

impl ConnectionUi {
    /// Replaces the current note. Progress chatter ([`NoteKind::Silent`])
    /// never replaces a note the toast still shows, so "pinned" or "sending"
    /// cannot wipe an unread failure; the toast's own timer clears that one.
    pub(super) fn set_note(&mut self, note: StatusNote) {
        self.set_note_in(self.note_window, note);
    }

    /// [`Self::set_note`], toasting in the window `host` (`None`: the
    /// main window) whichever window the last input happened in.
    pub(super) fn set_note_in(&mut self, host: Option<quill::ids::ChatId>, note: StatusNote) {
        if note.kind() == NoteKind::Silent && self.note.is_toast() {
            return;
        }
        self.toasts.push_in(note.clone(), host, Instant::now());
        self.note = note;
    }

    /// The current note.
    pub(super) fn note(&self) -> &StatusNote {
        &self.note
    }

    /// The current note's text.
    pub(super) fn status_note(&self) -> &str {
        self.note.text()
    }

    /// Drops the current note.
    pub(super) fn clear_note(&mut self) {
        self.note = StatusNote::default();
        self.toasts = ToastQueue::default();
    }

    /// Drops queued toasts without touching the current note: the footer
    /// line shows that note while there is no session.
    pub(super) fn drop_toasts(&mut self) {
        self.toasts = ToastQueue::default();
        self.toast_wake = None;
    }

    /// Advances the toast timers to `now`, and drops the current note once
    /// its toast is over so the same message set again shows again.
    pub(super) fn advance_toasts(&mut self, now: Instant) {
        self.toasts.tick(now);
        if self.note.is_toast() && !self.toasts.holds(&self.note) {
            self.note = StatusNote::default();
        }
    }

    /// Whether the poll should post the "connection closed" note now: the
    /// bridge stopped on its own, its queue is fully drained, and this
    /// connection has not posted it yet. True at most once per connection.
    pub(super) fn claim_closed_note(&mut self, bridge_stopped: bool, budget_hit: bool) -> bool {
        if !bridge_stopped || budget_hit || self.closed_noted_generation == Some(self.generation) {
            return false;
        }
        self.closed_noted_generation = Some(self.generation);
        true
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
    use super::{ConnectionUi, StatusNote, auth_for_bridge};
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

    fn connection() -> ConnectionUi {
        ConnectionUi::new(super::ConnectUiStatus::Live, String::new())
    }

    #[test]
    fn closed_note_waits_for_the_queue_and_fires_once() {
        let mut c = connection();
        assert!(!c.claim_closed_note(false, false));
        assert!(!c.claim_closed_note(true, true), "queue not drained");
        assert!(c.claim_closed_note(true, false));
        assert!(!c.claim_closed_note(true, false), "latched");
    }

    #[test]
    fn replacing_the_note_does_not_refire_while_the_bridge_stays_stopped() {
        let mut c = connection();
        assert!(c.claim_closed_note(true, false));
        c.set_note(StatusNote::info("Copied"));
        assert!(!c.claim_closed_note(true, false));
        assert_eq!(c.status_note(), "Copied", "the user's note is kept");
        // A new connection re-arms the latch.
        c.generation += 1;
        assert!(c.claim_closed_note(true, false));
    }

    #[test]
    fn progress_chatter_cannot_wipe_a_shown_failure() {
        let mut c = connection();
        c.set_note(StatusNote::error("Senden fehlgeschlagen"));
        c.set_note(StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "Senden fehlgeschlagen");
        assert!(c.note().is_toast(), "a German failure still toasts");
        c.set_note(StatusNote::info("Link kopiert"));
        assert_eq!(c.status_note(), "Link kopiert");
        c.clear_note();
        c.set_note(StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "sending…");
    }

    #[test]
    fn a_finished_toast_frees_the_note() {
        let mut c = connection();
        c.set_note(StatusNote::error("Couldn't send"));
        c.advance_toasts(std::time::Instant::now() + std::time::Duration::from_secs(7));
        assert!(c.note().is_empty());
        c.set_note(StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "sending…");
    }
}
