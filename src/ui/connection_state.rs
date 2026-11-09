//! Connection status, the status line and presence sync.
#![deny(clippy::indexing_slicing)]

use super::connect_ui::ConnectUiStatus;
use super::status_note::{NoteKind, StatusNote};
use super::toast_queue::ToastQueue;
use std::time::Instant;

/// The connection as the account on screen shows it: its status, the
/// status note and the toasts, presence sync. A field of `OnScreen`; the
/// connection count itself is the app's ([`ConnectGeneration`]).
pub(crate) struct ConnectionUi {
    pub(super) status: ConnectUiStatus,
    /// The current note. Private: every write names its kind through
    /// `set_note`, so a toast never depends on the words.
    note: StatusNote,
    /// The toasts on screen and waiting, with the visible one's timer.
    pub(super) toasts: ToastQueue,
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
            note: StatusNote::new(NoteKind::Silent, status_note),
            toasts: ToastQueue::default(),
            toast_wake: None,
            status_traced: String::new(),
            presence: Default::default(),
        }
    }
}

/// The note and its toasts were the old account's; presence is sent
/// again by the client put on screen next.
impl super::on_screen::Farewell for ConnectionUi {}

impl ConnectionUi {
    /// Replaces the current note, toasting in the window `host` (`None`:
    /// the main window). Progress chatter ([`NoteKind::Silent`]) never
    /// replaces a note the toast still shows, so "pinned" or "sending"
    /// cannot wipe an unread failure; the toast's own timer clears that one.
    /// Code in `src/ui` sets a note with [`QuillApp::set_note`], which
    /// names the window from the [`Asker`](super::chat_window::dialog_hosts::Asker);
    /// `scripts/note_hosts.py` keeps the other callers of this one listed.
    /// [`QuillApp::set_note`], for code that holds other parts of the app
    /// borrowed (the client): `windows` is the app's, whose Asker names
    /// the window.
    pub(super) fn set_note_for(
        &mut self,
        windows: &super::chat_window::ChatWindows,
        note: StatusNote,
    ) {
        self.set_note_in(windows.asking_window_or_last_press(), note);
    }

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
}

impl super::app::QuillApp {
    /// Replaces the current note ([`ConnectionUi::set_note_in`]). Its toast
    /// shows in the window that asked for what runs now, as a box does
    /// ([`Self::asking_window_or_last_press`]): the window of the press,
    /// of the frame, of the deferred work, or the one a flow carried under
    /// `as_asked_by`; where no window asked, the window of the last press.
    pub(super) fn set_note(&mut self, note: StatusNote) {
        self.screen
            .connection
            .set_note_for(&self.screen.windows, note);
    }
}

/// Counts the clients put on screen, so a poll loop or a "connection
/// closed" note of an older one stops. The process's: a `QuillApp` field
/// (`connect`), bumped by `start_connection` and `adopt_live`.
pub(crate) struct ConnectGeneration {
    pub(super) generation: u64,
    /// The connection generation whose lost bridge already posted the
    /// "connection closed" note. A new connection bumps `generation`, which
    /// re-arms it; other notes cannot, so they are never overwritten again.
    closed_noted: Option<u64>,
}

impl ConnectGeneration {
    pub(super) fn new() -> Self {
        Self {
            generation: 0,
            closed_noted: None,
        }
    }

    /// Whether the poll should post the "connection closed" note now: the
    /// bridge stopped on its own, its queue is fully drained, and this
    /// connection has not posted it yet. True at most once per connection.
    pub(super) fn claim_closed_note(&mut self, bridge_stopped: bool, budget_hit: bool) -> bool {
        if !bridge_stopped || budget_hit || self.closed_noted == Some(self.generation) {
            return false;
        }
        self.closed_noted = Some(self.generation);
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
    use super::{ConnectGeneration, ConnectionUi, StatusNote, auth_for_bridge};
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
        let mut g = ConnectGeneration::new();
        assert!(!g.claim_closed_note(false, false));
        assert!(!g.claim_closed_note(true, true), "queue not drained");
        assert!(g.claim_closed_note(true, false));
        assert!(!g.claim_closed_note(true, false), "latched");
    }

    #[test]
    fn a_new_connection_rearms_the_closed_note() {
        let mut g = ConnectGeneration::new();
        assert!(g.claim_closed_note(true, false));
        assert!(!g.claim_closed_note(true, false));
        g.generation += 1;
        assert!(g.claim_closed_note(true, false));
    }

    #[test]
    fn progress_chatter_cannot_wipe_a_shown_failure() {
        let mut c = connection();
        c.set_note_in(None, StatusNote::error("Senden fehlgeschlagen"));
        c.set_note_in(None, StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "Senden fehlgeschlagen");
        assert!(c.note().is_toast(), "a German failure still toasts");
        c.set_note_in(None, StatusNote::info("Link kopiert"));
        assert_eq!(c.status_note(), "Link kopiert");
        c.clear_note();
        c.set_note_in(None, StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "sending…");
    }

    #[test]
    fn a_finished_toast_frees_the_note() {
        let mut c = connection();
        c.set_note_in(None, StatusNote::error("Couldn't send"));
        c.advance_toasts(std::time::Instant::now() + std::time::Duration::from_secs(7));
        assert!(c.note().is_empty());
        c.set_note_in(None, StatusNote::silent("sending…"));
        assert_eq!(c.status_note(), "sending…");
    }
}
