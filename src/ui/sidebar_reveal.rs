//! Hiding the chat list column, and peeking at it while it is hidden.
//!
//! Two values drive the main window's left column (folder rail plus chat
//! list), both `0..=1` and both gliding on the frame clock:
//!
//! - `docked`: how much of the column takes part in the layout (1 shown,
//!   0 hidden). The shell narrows the column to `docked` of its width and
//!   slides the list out without squeezing it (`shell_layout`).
//! - `peek`: how far the column has slid in over the conversation while it
//!   is hidden and the pointer is at the window's left edge or over it.
//!
//! [`Peek`] is the pointer's side of the peek: it reaches the edge
//! (`Hovering`), leaves (`Leaving`, a short grace before the column slides
//! back out), or nothing is peeking (`Off`). The view reads the values on
//! every render; nothing else mirrors this state. Telegram Desktop has no
//! such feature (docs/decisions/sidebar-hide.md).
#![deny(clippy::indexing_slicing)]

use super::motion::Glide;
use gpui_kit::Task;
use std::time::{Duration, Instant};

/// How long the column takes to slide all the way in or out (Kit's
/// `Sheet` slides in 150 ms; this column is wider).
pub(crate) const SLIDE_DURATION: Duration = Duration::from_millis(200);
/// The instant the column's motion is measured against: the wall clock, or
/// in a test a clock the test moves by hand, so a slow machine cannot end a
/// grace period or a slide early.
pub(crate) fn now() -> Instant {
    #[cfg(test)]
    if let Some(pinned) = test_clock::PINNED.with(std::cell::Cell::get) {
        return pinned;
    }
    Instant::now()
}

#[cfg(test)]
pub(crate) mod test_clock {
    use std::time::{Duration, Instant};

    thread_local! {
        pub(super) static PINNED: std::cell::Cell<Option<Instant>> =
            const { std::cell::Cell::new(None) };
    }

    /// Hold the clock at now; it moves only through [`advance`].
    pub(crate) fn pin() {
        PINNED.with(|pinned| pinned.set(Some(Instant::now())));
    }

    pub(crate) fn advance(by: Duration) {
        PINNED.with(|pinned| pinned.set(pinned.get().map(|at| at + by)));
    }
}

/// How long the pointer may be away from the peeking column before it
/// slides back out.
pub(crate) const LEAVE_GRACE: Duration = Duration::from_millis(280);
/// Width of the strip at the window's left edge that reveals the hidden
/// column.
pub(crate) const REVEAL_EDGE_WIDTH: f32 = 6.;

/// Ease-out cubic: fast start, gentle landing.
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    let rest = 1. - t.clamp(0., 1.);
    1. - rest * rest * rest
}

/// Where the pointer is relative to the hidden column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Peek {
    Off,
    /// At the reveal edge or over the peeking column.
    Hovering,
    /// Left the column at `since`; it stays out for [`LEAVE_GRACE`].
    Leaving {
        since: Instant,
    },
}

/// The column's preference. The pointer only peeks at a hidden column, so
/// the peek lives in the hidden variant: showing the column ends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Docked,
    Hidden(Peek),
}

#[derive(Debug)]
pub(crate) struct SidebarReveal {
    mode: Mode,
    /// The redraw after the leave grace. It lives only in `Leaving`: every
    /// change of mode drops it.
    wake: Wake,
    docked: Glide,
    peek: Glide,
}

#[derive(Default)]
enum Wake {
    #[default]
    Idle,
    Waiting {
        _task: Task<()>,
    },
}

impl std::fmt::Debug for Wake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Wake::Idle => "Idle",
            Wake::Waiting { .. } => "Waiting",
        })
    }
}

impl SidebarReveal {
    pub(crate) fn new(hidden: bool, now: Instant) -> Self {
        Self {
            wake: Wake::Idle,
            mode: if hidden {
                Mode::Hidden(Peek::Off)
            } else {
                Mode::Docked
            },
            docked: Glide::settled(if hidden { 0. } else { 1. }, now, ease_out_cubic),
            peek: Glide::settled(0., now, ease_out_cubic),
        }
    }

    /// The one place the mode changes: the grace wake belongs to the
    /// `Leaving` it was made for, so it goes with any change.
    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.wake = Wake::Idle;
    }

    /// Keep the redraw after the leave grace: it runs while the pointer is
    /// away and is dropped with it (a pointer back, Escape, or the grace
    /// over). A wake for a mode that is no longer `Leaving` is dropped.
    pub(crate) fn hold_wake(&mut self, task: Task<()>) {
        if matches!(self.mode, Mode::Hidden(Peek::Leaving { .. })) {
            self.wake = Wake::Waiting { _task: task };
        }
    }

    /// The stored preference: the user hid the column.
    pub(crate) fn hidden(&self) -> bool {
        matches!(self.mode, Mode::Hidden(_))
    }

    /// Hide or show the column. Showing it while it peeks keeps it where it
    /// is on screen and only makes it take its room in the layout.
    pub(crate) fn set_hidden(&mut self, hidden: bool, animate: bool, now: Instant) {
        if self.hidden() == hidden {
            return;
        }
        self.set_mode(if hidden {
            Mode::Hidden(Peek::Off)
        } else {
            Mode::Docked
        });
        if !hidden {
            let peeked = self.peek.value(now);
            if peeked > self.docked.value(now) {
                self.docked = Glide::settled(peeked, now, ease_out_cubic);
            }
        }
        self.peek = Glide::settled(0., now, ease_out_cubic);
        self.docked
            .go(if hidden { 0. } else { 1. }, SLIDE_DURATION, animate, now);
    }

    /// The pointer reached the reveal edge or moved over the peeking
    /// column. Returns whether anything changed.
    pub(crate) fn pointer_entered(&mut self, animate: bool, now: Instant) -> bool {
        let Mode::Hidden(pointer) = self.mode else {
            return false;
        };
        if pointer == Peek::Hovering {
            return false;
        }
        if pointer == Peek::Off && self.docked.value(now) > 0. {
            // Still sliding out after Hide: no peek on top of it.
            return false;
        }
        self.set_mode(Mode::Hidden(Peek::Hovering));
        self.peek.go(1., SLIDE_DURATION, animate, now);
        true
    }

    /// The pointer moved off the peeking column. Returns whether the grace
    /// period started (the caller redraws once it is over).
    pub(crate) fn pointer_left(&mut self, now: Instant) -> bool {
        if self.mode != Mode::Hidden(Peek::Hovering) {
            return false;
        }
        self.set_mode(Mode::Hidden(Peek::Leaving { since: now }));
        true
    }

    /// Escape: slide the peeking column out now. Returns whether it peeked.
    pub(crate) fn dismiss(&mut self, animate: bool, now: Instant) -> bool {
        match self.mode {
            Mode::Hidden(Peek::Hovering | Peek::Leaving { .. }) => {}
            Mode::Hidden(Peek::Off) | Mode::Docked => return false,
        }
        self.set_mode(Mode::Hidden(Peek::Off));
        self.peek.go(0., SLIDE_DURATION, animate, now);
        true
    }

    /// Once per frame: a column left longer than [`LEAVE_GRACE`] ago slides
    /// out, unless something opened from it still needs it (`held`: its
    /// context menu, or its search field has the keyboard).
    pub(crate) fn settle(&mut self, held: bool, animate: bool, now: Instant) {
        if let Mode::Hidden(Peek::Leaving { since }) = self.mode
            && !held
            && now.saturating_duration_since(since) >= LEAVE_GRACE
        {
            self.set_mode(Mode::Hidden(Peek::Off));
            self.peek.go(0., SLIDE_DURATION, animate, now);
        }
    }

    pub(crate) fn pointer(&self) -> Peek {
        match self.mode {
            Mode::Hidden(pointer) => pointer,
            Mode::Docked => Peek::Off,
        }
    }

    /// Share of the column's width in the layout, `0..=1`.
    pub(crate) fn docked(&self, now: Instant) -> f32 {
        self.docked.value(now).clamp(0., 1.)
    }

    /// How far the hidden column has slid in over the conversation, `0..=1`.
    pub(crate) fn peek(&self, now: Instant) -> f32 {
        self.peek.value(now).clamp(0., 1.)
    }

    /// Whether either value is still moving (the view asks for frames).
    pub(crate) fn moving(&self, now: Instant) -> bool {
        self.docked.animating(now) || self.peek.animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::{LEAVE_GRACE, Peek, SLIDE_DURATION, SidebarReveal, ease_out_cubic};
    use std::time::{Duration, Instant};

    const HALF: Duration = Duration::from_millis(100);

    #[test]
    fn ease_out_cubic_runs_from_zero_to_one_front_loaded() {
        assert_eq!(ease_out_cubic(0.), 0.);
        assert_eq!(ease_out_cubic(1.), 1.);
        assert!(ease_out_cubic(0.5) > 0.8);
        assert_eq!(ease_out_cubic(-1.), 0.);
        assert_eq!(ease_out_cubic(2.), 1.);
    }

    #[test]
    fn hiding_slides_the_column_out_and_showing_slides_it_back() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(false, t0);
        assert_eq!(reveal.docked(t0), 1.);
        reveal.set_hidden(true, true, t0);
        assert!(reveal.hidden());
        let mid = reveal.docked(t0 + HALF);
        assert!(mid > 0. && mid < 1., "{mid}");
        assert!(reveal.moving(t0 + HALF));
        assert_eq!(reveal.docked(t0 + SLIDE_DURATION), 0.);
        assert!(!reveal.moving(t0 + SLIDE_DURATION));
        // Reversing mid-flight starts from where the column is.
        reveal.set_hidden(false, true, t0 + HALF);
        assert!((reveal.docked(t0 + HALF) - mid).abs() < 1e-4);
        assert!(reveal.docked(t0 + HALF + Duration::from_millis(16)) > mid);
    }

    #[test]
    fn without_animation_the_column_jumps() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(false, t0);
        reveal.set_hidden(true, false, t0);
        assert_eq!(reveal.docked(t0), 0.);
        assert!(!reveal.moving(t0));
        assert!(reveal.pointer_entered(false, t0));
        assert_eq!(reveal.peek(t0), 1.);
    }

    #[test]
    fn the_edge_peeks_only_while_hidden_and_settled() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(false, t0);
        assert!(!reveal.pointer_entered(true, t0));
        reveal.set_hidden(true, true, t0);
        assert!(
            !reveal.pointer_entered(true, t0 + HALF),
            "still sliding out"
        );
        let t1 = t0 + SLIDE_DURATION;
        assert!(reveal.pointer_entered(true, t1));
        assert_eq!(reveal.pointer(), Peek::Hovering);
        assert!(!reveal.pointer_entered(true, t1), "already peeking");
        assert_eq!(reveal.peek(t1 + SLIDE_DURATION), 1.);
        assert_eq!(reveal.docked(t1 + SLIDE_DURATION), 0., "overlay, no push");
    }

    #[test]
    fn leaving_waits_out_the_grace_and_a_return_keeps_it() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.pointer_entered(true, t0);
        let t1 = t0 + SLIDE_DURATION;
        assert!(reveal.pointer_left(t1));
        assert!(!reveal.pointer_left(t1), "one grace per leave");
        reveal.settle(false, true, t1 + LEAVE_GRACE / 2);
        assert_eq!(reveal.pointer(), Peek::Leaving { since: t1 });
        // Back over the column within the grace.
        assert!(reveal.pointer_entered(true, t1 + LEAVE_GRACE / 2));
        reveal.settle(false, true, t1 + LEAVE_GRACE * 2);
        assert_eq!(reveal.pointer(), Peek::Hovering);
        // Away for good.
        let t2 = t1 + LEAVE_GRACE * 2;
        reveal.pointer_left(t2);
        reveal.settle(false, true, t2 + LEAVE_GRACE);
        assert_eq!(reveal.pointer(), Peek::Off);
        assert_eq!(reveal.peek(t2 + LEAVE_GRACE + SLIDE_DURATION), 0.);
    }

    #[test]
    fn a_menu_or_search_focus_holds_the_column_open() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.pointer_entered(false, t0);
        reveal.pointer_left(t0);
        let late = t0 + LEAVE_GRACE * 4;
        reveal.settle(true, false, late);
        assert_eq!(reveal.peek(late), 1.);
        reveal.settle(false, false, late);
        assert_eq!(reveal.peek(late), 0.);
    }

    #[test]
    fn escape_dismisses_the_peek() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        assert!(!reveal.dismiss(true, t0));
        reveal.pointer_entered(false, t0);
        assert!(reveal.dismiss(false, t0));
        assert_eq!(reveal.pointer(), Peek::Off);
        assert_eq!(reveal.peek(t0), 0.);
    }

    #[test]
    fn showing_from_a_peek_docks_it_in_place() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.pointer_entered(true, t0);
        let t1 = t0 + SLIDE_DURATION;
        reveal.set_hidden(false, true, t1);
        assert_eq!(reveal.docked(t1), 1.);
        assert_eq!(reveal.peek(t1), 0.);
        assert_eq!(reveal.pointer(), Peek::Off);
        assert!(!reveal.moving(t1));
    }
}
