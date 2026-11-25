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
    /// The drawer is held open (the drawer band of the Floating layout):
    /// the header's button, the chat-back key or a click at the reveal
    /// edge opened it, and it stays until something closes it.
    Held,
}

/// The chat list as a drawer over the chat (Floating, 601 to 925 px wide):
/// held open or not. It holds no task. What closes it: a chat opened, a
/// click on the chat beside it, Escape, the button again, the window
/// leaving the band, the window closing (it is dropped with its owner).
/// The user's hidden-sidebar preference is never written by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drawer {
    Closed,
    Held,
}

/// [`Peek`] with the redraw task of the leave grace inside `Leaving`, so
/// the task is dropped with the stage.
#[derive(Debug)]
enum Pointer {
    Off,
    Hovering,
    Leaving { since: Instant, _wake: Task<()> },
}

impl Pointer {
    fn view(&self) -> Peek {
        match self {
            Pointer::Off => Peek::Off,
            Pointer::Hovering => Peek::Hovering,
            Pointer::Leaving { since, .. } => Peek::Leaving { since: *since },
        }
    }
}

/// The column's preference. The pointer only peeks at a hidden column, so
/// the peek lives in the hidden variant: showing the column ends it.
#[derive(Debug)]
enum Mode {
    Docked,
    Hidden(Pointer),
}

#[derive(Debug)]
pub(crate) struct SidebarReveal {
    mode: Mode,
    drawer: Drawer,
    docked: Glide,
    peek: Glide,
    /// A test holds the clock still and moves it by hand, so a slow
    /// machine cannot end a grace period or a slide early.
    #[cfg(test)]
    pinned: Option<Instant>,
}

impl SidebarReveal {
    pub(crate) fn new(hidden: bool, now: Instant) -> Self {
        Self {
            mode: if hidden {
                Mode::Hidden(Pointer::Off)
            } else {
                Mode::Docked
            },
            drawer: Drawer::Closed,
            docked: Glide::settled(if hidden { 0. } else { 1. }, now, ease_out_cubic),
            peek: Glide::settled(0., now, ease_out_cubic),
            #[cfg(test)]
            pinned: None,
        }
    }

    /// The instant the column's motion is measured against: the wall
    /// clock, or in a test the clock it holds.
    pub(crate) fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(pinned) = self.pinned {
            return pinned;
        }
        Instant::now()
    }

    /// Hold the clock at now; it moves only through [`Self::advance`].
    #[cfg(test)]
    pub(crate) fn pin(&mut self) {
        self.pinned = Some(Instant::now());
    }

    #[cfg(test)]
    pub(crate) fn advance(&mut self, by: Duration) {
        self.pinned = self.pinned.map(|at| at + by);
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
        self.mode = if hidden {
            Mode::Hidden(Pointer::Off)
        } else {
            Mode::Docked
        };
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
        let Mode::Hidden(pointer) = &self.mode else {
            return false;
        };
        match pointer {
            Pointer::Hovering => return false,
            // Still sliding out after Hide: no peek on top of it.
            Pointer::Off if self.docked.value(now) > 0. => return false,
            Pointer::Off | Pointer::Leaving { .. } => {}
        }
        self.mode = Mode::Hidden(Pointer::Hovering);
        self.peek.go(1., SLIDE_DURATION, animate, now);
        true
    }

    /// The pointer moved off the peeking column. Returns whether the grace
    /// period started: `wake` then makes the redraw for when it is over,
    /// and the stage holds it (a pointer back, Escape or the grace over
    /// drops it).
    pub(crate) fn pointer_left(&mut self, now: Instant, wake: impl FnOnce() -> Task<()>) -> bool {
        if !matches!(self.mode, Mode::Hidden(Pointer::Hovering)) {
            return false;
        }
        self.mode = Mode::Hidden(Pointer::Leaving {
            since: now,
            _wake: wake(),
        });
        true
    }

    /// Open the drawer and hold it open. Returns whether it was closed.
    pub(crate) fn hold_drawer(&mut self, animate: bool, now: Instant) -> bool {
        if self.drawer == Drawer::Held {
            return false;
        }
        self.drawer = Drawer::Held;
        self.peek.go(1., SLIDE_DURATION, animate, now);
        true
    }

    /// Close a held drawer (a chat opened, a click beside it, the button
    /// again). Returns whether it was held.
    pub(crate) fn close_drawer(&mut self, animate: bool, now: Instant) -> bool {
        if self.drawer == Drawer::Closed {
            return false;
        }
        self.drawer = Drawer::Closed;
        if !matches!(self.mode, Mode::Hidden(Pointer::Hovering)) {
            self.peek.go(0., SLIDE_DURATION, animate, now);
        }
        true
    }

    /// The header's button in the drawer band.
    pub(crate) fn toggle_drawer(&mut self, animate: bool, now: Instant) {
        if !self.close_drawer(animate, now) {
            self.hold_drawer(animate, now);
        }
    }

    /// The window left the drawer band: a held drawer is gone at once, as
    /// the docked (or hidden) list takes over by the user's preference.
    pub(crate) fn leave_drawer_band(&mut self, now: Instant) {
        if self.drawer == Drawer::Held {
            self.drawer = Drawer::Closed;
            if !matches!(self.mode, Mode::Hidden(Pointer::Hovering)) {
                self.peek = Glide::settled(0., now, ease_out_cubic);
            }
        }
    }

    /// Escape: slide the peeking column out now. Returns whether it peeked.
    pub(crate) fn dismiss(&mut self, animate: bool, now: Instant) -> bool {
        if self.drawer == Drawer::Held {
            self.drawer = Drawer::Closed;
            if let Mode::Hidden(pointer) = &mut self.mode {
                *pointer = Pointer::Off;
            }
            self.peek.go(0., SLIDE_DURATION, animate, now);
            return true;
        }
        match self.mode {
            Mode::Hidden(Pointer::Hovering | Pointer::Leaving { .. }) => {}
            Mode::Hidden(Pointer::Off) | Mode::Docked => return false,
        }
        self.mode = Mode::Hidden(Pointer::Off);
        self.peek.go(0., SLIDE_DURATION, animate, now);
        true
    }

    /// Once per frame: a column left longer than [`LEAVE_GRACE`] ago slides
    /// out, unless something opened from it still needs it (`held`: its
    /// context menu, or its search field has the keyboard).
    pub(crate) fn settle(&mut self, held: bool, animate: bool, now: Instant) {
        if let Mode::Hidden(Pointer::Leaving { since, .. }) = &self.mode
            && !held
            && now.saturating_duration_since(*since) >= LEAVE_GRACE
        {
            self.mode = Mode::Hidden(Pointer::Off);
            if self.drawer == Drawer::Closed {
                self.peek.go(0., SLIDE_DURATION, animate, now);
            }
        }
    }

    pub(crate) fn pointer(&self) -> Peek {
        if self.drawer == Drawer::Held {
            return Peek::Held;
        }
        match &self.mode {
            Mode::Hidden(pointer) => pointer.view(),
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
    use gpui_kit::Task;
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
        assert!(reveal.pointer_left(t1, || Task::ready(())));
        assert!(
            !reveal.pointer_left(t1, || Task::ready(())),
            "one grace per leave"
        );
        reveal.settle(false, true, t1 + LEAVE_GRACE / 2);
        assert_eq!(reveal.pointer(), Peek::Leaving { since: t1 });
        // Back over the column within the grace.
        assert!(reveal.pointer_entered(true, t1 + LEAVE_GRACE / 2));
        reveal.settle(false, true, t1 + LEAVE_GRACE * 2);
        assert_eq!(reveal.pointer(), Peek::Hovering);
        // Away for good.
        let t2 = t1 + LEAVE_GRACE * 2;
        reveal.pointer_left(t2, || Task::ready(()));
        reveal.settle(false, true, t2 + LEAVE_GRACE);
        assert_eq!(reveal.pointer(), Peek::Off);
        assert_eq!(reveal.peek(t2 + LEAVE_GRACE + SLIDE_DURATION), 0.);
    }

    #[test]
    fn a_menu_or_search_focus_holds_the_column_open() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.pointer_entered(false, t0);
        reveal.pointer_left(t0, || Task::ready(()));
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
    fn the_drawer_holds_until_the_button_closes_it() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(false, t0);
        assert!(reveal.hold_drawer(true, t0));
        assert_eq!(reveal.pointer(), Peek::Held);
        assert!(!reveal.hold_drawer(true, t0), "already held");
        let open = t0 + SLIDE_DURATION;
        assert_eq!(reveal.peek(open), 1.);
        assert_eq!(reveal.docked(open), 1., "the preference is untouched");
        // Long after: still held, no grace runs out.
        reveal.settle(false, true, open + LEAVE_GRACE * 10);
        assert_eq!(reveal.pointer(), Peek::Held);
        reveal.toggle_drawer(true, open);
        assert_eq!(reveal.pointer(), Peek::Off);
        assert_eq!(reveal.peek(open + SLIDE_DURATION), 0.);
        reveal.toggle_drawer(false, open);
        assert_eq!(reveal.pointer(), Peek::Held);
        assert!(!reveal.hidden());
    }

    #[test]
    fn a_chat_opened_or_a_click_beside_closes_the_drawer() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(false, t0);
        reveal.hold_drawer(false, t0);
        assert!(reveal.close_drawer(true, t0));
        assert_eq!(reveal.pointer(), Peek::Off);
        assert!(!reveal.close_drawer(true, t0), "already closed");
        assert!(reveal.moving(t0 + HALF));
        assert_eq!(reveal.peek(t0 + SLIDE_DURATION), 0.);
    }

    #[test]
    fn escape_closes_the_drawer() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.hold_drawer(false, t0);
        assert!(reveal.dismiss(false, t0));
        assert_eq!(reveal.pointer(), Peek::Off);
        assert_eq!(reveal.peek(t0), 0.);
        assert!(reveal.hidden(), "the preference is untouched");
    }

    #[test]
    fn leaving_the_band_drops_the_drawer_at_once() {
        let t0 = Instant::now();
        for hidden in [false, true] {
            let mut reveal = SidebarReveal::new(hidden, t0);
            reveal.hold_drawer(true, t0);
            reveal.leave_drawer_band(t0 + HALF);
            assert_eq!(reveal.pointer(), Peek::Off);
            assert_eq!(reveal.peek(t0 + HALF), 0.);
            assert!(!reveal.moving(t0 + HALF));
            assert_eq!(reveal.hidden(), hidden, "the preference survives");
        }
    }

    #[test]
    fn a_hover_peek_left_behind_a_held_drawer_keeps_it_open() {
        let t0 = Instant::now();
        let mut reveal = SidebarReveal::new(true, t0);
        reveal.pointer_entered(false, t0);
        reveal.hold_drawer(false, t0);
        reveal.pointer_left(t0, || Task::ready(()));
        reveal.settle(false, false, t0 + LEAVE_GRACE * 2);
        assert_eq!(reveal.pointer(), Peek::Held);
        assert_eq!(reveal.peek(t0 + LEAVE_GRACE * 2), 1.);
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
