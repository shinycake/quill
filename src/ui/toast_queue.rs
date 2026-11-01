//! The toast queue: which status note the toast shows now, which wait, and
//! how much time the visible one has left. Pure state with the clock passed
//! in, so the rules (an error is never lost to an info note, the same note
//! twice shows once, hovering pauses the timer) are unit-tested without a
//! window. Render reads [`ToastQueue::visible`]; the app only schedules a
//! wake-up for [`ToastQueue::deadline`].
#![deny(clippy::indexing_slicing)]

use super::status_note::{NoteKind, StatusNote};
use quill::ids::ChatId;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Notes waiting behind the visible one. Past this, the oldest waiting
/// confirmation is dropped; when only failures wait, the oldest of them goes.
const MAX_WAITING: usize = 8;

/// A note and the window it toasts in (`None`: the main window), fixed
/// when it was queued.
#[derive(Clone, Debug, PartialEq)]
struct Toast {
    note: StatusNote,
    host: Option<ChatId>,
}

/// The toast on screen and its remaining time.
#[derive(Clone, Debug)]
struct Showing {
    toast: Toast,
    /// Time left when the timer last stopped (or the full duration).
    remaining: Duration,
    /// Set while the timer runs; `None` while hovered.
    running_since: Option<Instant>,
}

impl Showing {
    fn left(&self, now: Instant) -> Duration {
        match self.running_since {
            Some(since) => self
                .remaining
                .saturating_sub(now.saturating_duration_since(since)),
            None => self.remaining,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ToastQueue {
    showing: Option<Showing>,
    waiting: VecDeque<Toast>,
    /// The pointer is over the toast slot: the next toast starts paused too,
    /// since the pointer has not left.
    hovered: bool,
}

impl ToastQueue {
    /// Queues a note for the main window.
    #[cfg(test)]
    pub(crate) fn push(&mut self, note: StatusNote, now: Instant) {
        self.push_in(note, None, now);
    }

    /// Queues a note for the window `host` (`None`: the main window).
    /// Silent and empty notes never toast. The note already showing (or
    /// last in line) is not repeated: the visible one just gets its full
    /// time back. A failure goes ahead of waiting confirmations.
    pub(crate) fn push_in(&mut self, note: StatusNote, host: Option<ChatId>, now: Instant) {
        if !note.is_toast() {
            return;
        }
        let toast = Toast { note, host };
        let Some(showing) = self.showing.as_mut() else {
            self.show(toast, now);
            return;
        };
        if showing.toast == toast {
            showing.remaining = toast.note.toast_duration();
            if showing.running_since.is_some() {
                showing.running_since = Some(now);
            }
            return;
        }
        if self.waiting.back() == Some(&toast) {
            return;
        }
        if toast.note.kind() == NoteKind::Error {
            let at = self
                .waiting
                .iter()
                .position(|queued| queued.note.kind() != NoteKind::Error)
                .unwrap_or(self.waiting.len());
            self.waiting.insert(at, toast);
        } else {
            self.waiting.push_back(toast);
        }
        while self.waiting.len() > MAX_WAITING {
            let Some(at) = self
                .waiting
                .iter()
                .position(|queued| queued.note.kind() != NoteKind::Error)
            else {
                self.waiting.pop_front();
                continue;
            };
            self.waiting.remove(at);
        }
    }

    /// The window `closed` went: its toasts move to the main window.
    pub(crate) fn window_closed(&mut self, closed: ChatId, _now: Instant) {
        for toast in self
            .showing
            .iter_mut()
            .map(|showing| &mut showing.toast)
            .chain(self.waiting.iter_mut())
            .filter(|toast| toast.host == Some(closed))
        {
            toast.host = None;
        }
    }

    /// Ends the visible toast when its time is up and shows the next one.
    pub(crate) fn tick(&mut self, now: Instant) {
        while self
            .showing
            .as_ref()
            .is_some_and(|showing| showing.left(now).is_zero())
        {
            self.dismiss(now);
        }
    }

    /// A click: the visible toast goes, the next one comes.
    pub(crate) fn dismiss(&mut self, now: Instant) {
        self.showing = None;
        match self.waiting.pop_front() {
            Some(next) => self.show(next, now),
            None => self.hovered = false,
        }
    }

    /// Pointer over the toast stops its timer; leaving restarts it with the
    /// time that was left.
    pub(crate) fn set_hovered(&mut self, hovered: bool, now: Instant) {
        // Only the rendered toast reports hover, so a hover with nothing
        // showing is stale and must not pause the next toast.
        let Some(showing) = self.showing.as_mut() else {
            return;
        };
        self.hovered = hovered;
        match (hovered, showing.running_since) {
            (true, Some(_)) => {
                showing.remaining = showing.left(now);
                showing.running_since = None;
            }
            (false, None) => showing.running_since = Some(now),
            _ => {}
        }
    }

    /// The note on screen.
    pub(crate) fn visible(&self) -> Option<&StatusNote> {
        self.showing.as_ref().map(|showing| &showing.toast.note)
    }

    /// The window the note on screen toasts in (`None`: the main window).
    pub(crate) fn visible_host(&self) -> Option<Option<ChatId>> {
        self.showing.as_ref().map(|showing| showing.toast.host)
    }

    /// When the visible toast ends, or `None` while paused or empty.
    pub(crate) fn deadline(&self, now: Instant) -> Option<Instant> {
        let showing = self.showing.as_ref()?;
        showing.running_since?;
        let left = showing.left(now);
        if left.is_zero() {
            return None;
        }
        now.checked_add(left)
    }

    /// Whether this note is on screen or still waiting.
    pub(crate) fn holds(&self, note: &StatusNote) -> bool {
        self.visible() == Some(note) || self.waiting.iter().any(|toast| &toast.note == note)
    }

    /// Notes behind the visible one.
    #[cfg(test)]
    pub(crate) fn waiting(&self) -> usize {
        self.waiting.len()
    }

    fn show(&mut self, toast: Toast, now: Instant) {
        self.showing = Some(Showing {
            remaining: toast.note.toast_duration(),
            running_since: (!self.hovered).then_some(now),
            toast,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::ToastQueue;
    use crate::ui::status_note::StatusNote;
    use std::time::{Duration, Instant};

    fn secs(start: Instant, s: u64) -> Instant {
        start + Duration::from_secs(s)
    }

    fn text(queue: &ToastQueue) -> Option<&str> {
        queue.visible().map(StatusNote::text)
    }

    #[test]
    fn an_info_note_never_replaces_a_failure() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::error("Couldn't send"), t0);
        queue.push(StatusNote::info("Saved to Downloads"), secs(t0, 1));
        assert_eq!(text(&queue), Some("Couldn't send"));
        assert_eq!(queue.waiting(), 1);
        queue.tick(secs(t0, 6));
        assert_eq!(text(&queue), Some("Saved to Downloads"));
    }

    #[test]
    fn a_failure_goes_ahead_of_waiting_confirmations() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::info("one"), t0);
        queue.push(StatusNote::info("two"), t0);
        queue.push(StatusNote::error("boom"), t0);
        queue.dismiss(t0);
        assert_eq!(text(&queue), Some("boom"));
        queue.dismiss(t0);
        assert_eq!(text(&queue), Some("two"));
    }

    #[test]
    fn identical_consecutive_notes_show_once_and_restart_the_timer() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::info("Copied"), t0);
        queue.push(StatusNote::info("Copied"), secs(t0, 2));
        assert_eq!(queue.waiting(), 0);
        queue.tick(secs(t0, 4));
        assert_eq!(text(&queue), Some("Copied"));
        queue.tick(secs(t0, 5));
        assert_eq!(text(&queue), None);
        queue.push(StatusNote::info("a"), t0);
        queue.push(StatusNote::info("b"), t0);
        queue.push(StatusNote::info("b"), t0);
        assert_eq!(queue.waiting(), 1);
    }

    #[test]
    fn hovering_pauses_the_timer_and_leaving_resumes_it() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::info("Copied"), t0);
        queue.set_hovered(true, secs(t0, 2));
        assert_eq!(queue.deadline(secs(t0, 10)), None);
        queue.tick(secs(t0, 60));
        assert_eq!(text(&queue), Some("Copied"));
        queue.set_hovered(false, secs(t0, 60));
        assert_eq!(queue.deadline(secs(t0, 60)), Some(secs(t0, 61)));
        queue.tick(secs(t0, 61));
        assert_eq!(text(&queue), None);
    }

    #[test]
    fn a_hover_with_no_toast_never_pauses_the_next_one() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.set_hovered(true, t0);
        queue.push(StatusNote::info("Copied"), t0);
        assert!(queue.deadline(secs(t0, 1)).is_some());
        queue.tick(secs(t0, 60));
        assert_eq!(text(&queue), None);
    }

    #[test]
    fn silent_and_empty_notes_never_queue() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::silent("sending"), t0);
        queue.push(StatusNote::info(""), t0);
        assert!(queue.visible().is_none());
    }

    #[test]
    fn the_waiting_line_drops_old_confirmations_but_never_failures() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::error("first"), t0);
        for n in 0..10 {
            queue.push(StatusNote::info(format!("info {n}")), t0);
        }
        queue.push(StatusNote::error("second"), t0);
        assert_eq!(queue.waiting(), 8);
        queue.dismiss(t0);
        assert_eq!(text(&queue), Some("second"));
    }

    #[test]
    fn a_flood_of_failures_stays_capped() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        for n in 0..30 {
            queue.push(StatusNote::error(format!("error {n}")), t0);
        }
        assert_eq!(queue.waiting(), 8);
        queue.dismiss(t0);
        assert_eq!(text(&queue), Some("error 22"));
    }

    #[test]
    fn dismissing_while_hovered_keeps_the_next_toast_paused() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::error("one"), t0);
        queue.push(StatusNote::info("two"), t0);
        queue.set_hovered(true, secs(t0, 1));
        queue.dismiss(secs(t0, 1));
        assert_eq!(text(&queue), Some("two"));
        assert_eq!(queue.deadline(secs(t0, 2)), None);
        queue.tick(secs(t0, 60));
        assert_eq!(text(&queue), Some("two"));
        queue.set_hovered(false, secs(t0, 60));
        assert!(queue.deadline(secs(t0, 60)).is_some());
        queue.dismiss(secs(t0, 61));
        assert_eq!(text(&queue), None);
        queue.push(StatusNote::info("three"), secs(t0, 62));
        assert!(queue.deadline(secs(t0, 62)).is_some());
    }

    #[test]
    fn a_closed_windows_toasts_move_to_the_main_window() {
        let t0 = Instant::now();
        let window = quill::ids::ChatId(12);
        let mut queue = ToastQueue::default();
        queue.push_in(StatusNote::error("one"), Some(window), t0);
        queue.push_in(StatusNote::info("two"), Some(window), t0);
        assert_eq!(queue.visible_host(), Some(Some(window)));
        queue.window_closed(window, secs(t0, 2));
        assert_eq!(queue.visible_host(), Some(None), "in the main window");
        queue.dismiss(secs(t0, 3));
        assert_eq!(text(&queue), Some("two"));
        assert_eq!(queue.visible_host(), Some(None));
    }

    #[test]
    fn the_same_note_for_two_windows_shows_in_each() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push_in(StatusNote::error("boom"), Some(quill::ids::ChatId(12)), t0);
        queue.push_in(StatusNote::error("boom"), None, t0);
        assert_eq!(queue.waiting(), 1);
    }

    #[test]
    fn an_empty_queue_has_no_deadline_after_the_time_is_up() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::info("Copied"), t0);
        assert_eq!(queue.deadline(secs(t0, 60)), None);
    }
}
