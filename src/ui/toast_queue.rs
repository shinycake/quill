//! The toast queue: which status note the toast shows now, which wait, and
//! how much time the visible one has left. Pure state with the clock passed
//! in, so the rules (an error is never lost to an info note, the same note
//! twice shows once, hovering pauses the timer) are unit-tested without a
//! window. Render reads [`ToastQueue::visible`]; the app only schedules a
//! wake-up for [`ToastQueue::deadline`].
#![deny(clippy::indexing_slicing)]

use super::status_note::{NoteKind, StatusNote};
use gpui_kit::Task;
use quill::ids::ChatId;
use std::collections::VecDeque;
use std::rc::Rc;
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
    timer: Timer,
}

/// The visible toast's time: running, or stopped while hovered.
#[derive(Clone, Copy, Debug)]
enum Timer {
    /// Ticking since `since`, with `left` left at that moment.
    Running { left: Duration, since: Instant },
    /// Stopped with `left` left.
    Paused { left: Duration },
}

/// The wake-up scheduled for the visible toast's deadline. The queue owns
/// its task, so a click, a hover, an empty queue or a dropped queue cancels
/// it. (`Rc` so the queue stays `Clone`.)
#[derive(Clone, Default)]
enum Wake {
    #[default]
    Idle,
    Scheduled {
        deadline: Instant,
        task: Rc<Task<()>>,
    },
}

impl std::fmt::Debug for Wake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("Idle"),
            Self::Scheduled { deadline, .. } => write!(f, "Scheduled({deadline:?})"),
        }
    }
}

impl Wake {
    fn deadline(&self) -> Option<Instant> {
        match self {
            Self::Idle => None,
            Self::Scheduled { deadline, .. } => Some(*deadline),
        }
    }
}

impl Showing {
    fn left(&self, now: Instant) -> Duration {
        match self.timer {
            Timer::Running { left, since } => {
                left.saturating_sub(now.saturating_duration_since(since))
            }
            Timer::Paused { left } => left,
        }
    }
}

/// The current status note, the toasts it feeds and the wake-up scheduled
/// for the visible one: one value, so a note, its toast and its timer end
/// together.
#[derive(Default)]
pub(crate) struct ToastQueue {
    /// The current note. Every write names its kind through
    /// [`Self::set_note_in`], so a toast never depends on the words.
    note: StatusNote,
    /// The wake-up scheduled for the visible toast's deadline.
    wake: Wake,
    showing: Option<Showing>,
    waiting: VecDeque<Toast>,
}

impl ToastQueue {
    /// A queue whose current note is `note` and nothing toasts.
    pub(crate) fn with_note(note: StatusNote) -> Self {
        Self {
            note,
            ..Self::default()
        }
    }

    /// The current note.
    pub(crate) fn note(&self) -> &StatusNote {
        &self.note
    }

    /// Replaces the current note, toasting in the window `host` (`None`: the
    /// main window). Progress chatter ([`NoteKind::Silent`]) never replaces a
    /// note the toast still shows, so "pinned" or "sending" cannot wipe an
    /// unread failure; the toast's own timer clears that one.
    // note-host: the queue's own write; `ConnectionUi::set_note_in` passes the window the Asker named.
    pub(crate) fn set_note_in(&mut self, note: StatusNote, host: Option<ChatId>, now: Instant) {
        if note.kind() == NoteKind::Silent && self.note.is_toast() {
            return;
        }
        self.push_in(note.clone(), host, now);
        self.note = note;
    }

    /// Drops the current note and its toasts.
    pub(crate) fn clear_note(&mut self) {
        *self = Self {
            wake: std::mem::take(&mut self.wake),
            ..Self::default()
        };
    }

    /// Drops queued toasts without touching the current note: the footer
    /// line shows that note while there is no session.
    pub(crate) fn drop_toasts(&mut self) {
        *self = Self::with_note(std::mem::take(&mut self.note));
    }

    /// Advances the timers to `now`, and drops the current note once its
    /// toast is over so the same message set again shows again.
    pub(crate) fn advance(&mut self, now: Instant) {
        self.tick(now);
        if self.note.is_toast() && !self.holds(&self.note) {
            self.note = StatusNote::default();
        }
    }

    /// The deadline a wake-up must be scheduled for, when the visible
    /// toast's has moved since the last one (the old wake-up is cancelled).
    /// The caller schedules it and hands the task to [`Self::scheduled`].
    pub(crate) fn wanted_wake(&mut self, now: Instant) -> Option<Instant> {
        let deadline = self.deadline(now);
        if deadline == self.wake.deadline() {
            return None;
        }
        self.wake = Wake::Idle;
        deadline
    }

    /// The wake-up for `deadline` is scheduled as `task`.
    pub(crate) fn scheduled(&mut self, deadline: Instant, task: Task<()>) {
        self.wake = Wake::Scheduled {
            deadline,
            task: Rc::new(task),
        };
    }

    /// The wake-up for `deadline` fired. Its task is the one running this,
    /// so it is let go, not cancelled from inside itself.
    pub(crate) fn woke(&mut self, deadline: Instant) {
        if self.wake.deadline() != Some(deadline) {
            return;
        }
        if let Wake::Scheduled { task, .. } = std::mem::take(&mut self.wake)
            && let Ok(task) = Rc::try_unwrap(task)
        {
            // This task is the one running this call: detach lets it end, dropping would cancel it.
            task.detach();
        }
    }

    /// A click on the toast: it goes, the next one comes, and its timer's
    /// wake-up is no longer wanted.
    pub(crate) fn dismiss_by_click(&mut self, now: Instant) {
        self.dismiss(now);
        self.wake = Wake::Idle;
    }

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
            self.show(toast, false, now);
            return;
        };
        if showing.toast == toast {
            let full = toast.note.toast_duration();
            showing.timer = match showing.timer {
                Timer::Running { .. } => Timer::Running {
                    left: full,
                    since: now,
                },
                Timer::Paused { .. } => Timer::Paused { left: full },
            };
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

    /// The window `closed` went: its toasts move to the main window, and a
    /// hover there can never end, so the visible toast's timer runs again.
    pub(crate) fn window_closed(&mut self, closed: ChatId, now: Instant) {
        let showing_there = self
            .showing
            .as_ref()
            .is_some_and(|showing| showing.toast.host == Some(closed));
        for toast in self
            .showing
            .iter_mut()
            .map(|showing| &mut showing.toast)
            .chain(self.waiting.iter_mut())
            .filter(|toast| toast.host == Some(closed))
        {
            toast.host = None;
        }
        if showing_there {
            self.set_hovered(false, now);
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
        // The pointer has not left the slot: the next toast starts paused.
        let hovered = self
            .showing
            .take()
            .is_some_and(|showing| matches!(showing.timer, Timer::Paused { .. }));
        if let Some(next) = self.waiting.pop_front() {
            self.show(next, hovered, now);
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
        let left = showing.left(now);
        match (hovered, showing.timer) {
            (true, Timer::Running { .. }) => showing.timer = Timer::Paused { left },
            (false, Timer::Paused { .. }) => showing.timer = Timer::Running { left, since: now },
            _ => {}
        }
    }

    /// The pointer cannot be over the toast any more (the lock layer draws
    /// none, or its window went): a hover that never ends would pause it for
    /// good and queue every later toast behind it.
    pub(crate) fn pointer_gone(&mut self, now: Instant) {
        if let Some(showing) = self.showing.as_mut()
            && let Timer::Paused { left } = showing.timer
        {
            showing.timer = Timer::Running { left, since: now };
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
        let Timer::Running { .. } = showing.timer else {
            return None;
        };
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

    fn show(&mut self, toast: Toast, hovered: bool, now: Instant) {
        let left = toast.note.toast_duration();
        self.showing = Some(Showing {
            timer: if hovered {
                Timer::Paused { left }
            } else {
                Timer::Running { left, since: now }
            },
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
    fn a_hover_that_never_ends_stops_pausing_once_the_pointer_is_gone() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(StatusNote::error("one"), t0);
        queue.push(StatusNote::info("two"), t0);
        queue.set_hovered(true, secs(t0, 1));
        assert_eq!(queue.deadline(secs(t0, 2)), None, "paused by the hover");
        // The lock layer takes the toast away: nobody reports hover end.
        queue.pointer_gone(secs(t0, 2));
        assert!(
            queue.deadline(secs(t0, 2)).is_some(),
            "the timer runs again"
        );
        queue.tick(secs(t0, 120));
        assert_eq!(text(&queue), Some("two"));
        assert!(
            queue.deadline(secs(t0, 120)).is_some(),
            "the next toast does not start paused"
        );
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
    fn a_closed_windows_toasts_move_to_the_main_window_and_run_again() {
        let t0 = Instant::now();
        let window = quill::ids::ChatId(12);
        let mut queue = ToastQueue::default();
        queue.push_in(StatusNote::error("one"), Some(window), t0);
        queue.push_in(StatusNote::info("two"), Some(window), t0);
        assert_eq!(queue.visible_host(), Some(Some(window)));
        queue.set_hovered(true, secs(t0, 1));
        assert_eq!(queue.deadline(secs(t0, 2)), None, "paused");
        queue.window_closed(window, secs(t0, 2));
        assert_eq!(queue.visible_host(), Some(None), "in the main window");
        assert!(
            queue.deadline(secs(t0, 2)).is_some(),
            "the timer runs again"
        );
        queue.dismiss(secs(t0, 3));
        assert_eq!(text(&queue), Some("two"));
        assert_eq!(queue.visible_host(), Some(None));
        assert!(
            queue.deadline(secs(t0, 3)).is_some(),
            "the next is not paused"
        );
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

    #[test]
    fn the_note_its_toast_and_its_wake_end_together() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.set_note_in(StatusNote::error("boom"), None, t0);
        assert_eq!(queue.note().text(), "boom");
        // A wake-up is wanted once for the visible toast's deadline.
        let deadline = queue.wanted_wake(t0).expect("a wake");
        queue.scheduled(deadline, gpui_kit::Task::ready(()));
        assert_eq!(queue.wanted_wake(t0), None, "already scheduled");
        // Progress chatter does not replace a note its toast still shows.
        queue.set_note_in(StatusNote::silent("sending"), None, t0);
        assert_eq!(queue.note().text(), "boom");
        // The toast's time is up: the note goes with it, and so does the wake.
        queue.advance(secs(t0, 60));
        assert!(queue.note().text().is_empty());
        assert_eq!(queue.wanted_wake(secs(t0, 60)), None);
        // A click sends the toast and its wake-up away at once.
        queue.set_note_in(StatusNote::error("again"), None, secs(t0, 61));
        let later = queue.wanted_wake(secs(t0, 61)).expect("a wake");
        queue.scheduled(later, gpui_kit::Task::ready(()));
        queue.dismiss_by_click(secs(t0, 62));
        assert_eq!(queue.visible(), None);
        assert_eq!(queue.wanted_wake(secs(t0, 62)), None, "the click ended it");
    }

    #[test]
    fn dropping_toasts_keeps_the_note_the_footer_shows() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.set_note_in(StatusNote::error("boom"), None, t0);
        queue.drop_toasts();
        assert_eq!(queue.note().text(), "boom");
        assert_eq!(queue.visible(), None);
        queue.clear_note();
        assert!(queue.note().text().is_empty());
    }
}
