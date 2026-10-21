//! Pull-down expansion of the stories strip (tdesktop `dialogs_widget.cpp`
//! `setOverscrollTypes`, `storiesToggleExplicitExpand`, and
//! `Stories::List::updateExpanding`).
//!
//! In tdesktop the strip rests collapsed, as a stack of small avatars beside
//! the search field. Pulling the list down past its top stretches a virtual
//! overscroll that grows the strip; once the strip is more than
//! `kExpandAfterRatio` (0.72) of its height, it expands and stays expanded.
//! A pull that ends before that springs back. Momentum (a fling hitting the
//! top) never expands it (`_expandIgnored`). The strip collapses again once
//! the list has scrolled past it.
//!
//! Quill's strip is the first row of the chat list, so expanding inserts the
//! row above the list's top and slides it into view, and collapsing drops the
//! row once it has scrolled out of sight (the list's offset is shifted by the
//! row's height, which changes nothing on screen). This module holds the
//! state and the decisions; the UI applies them to the list's scroll handle.
//!
//! Pure core, no GPUI.
#![deny(clippy::indexing_slicing)]

use crate::stories_strip::{EXPAND_AFTER, FULL_HEIGHT};
use std::time::{Duration, Instant};

/// How far the list must be pulled past its top, in px, before the strip
/// expands (`kExpandAfterRatio` of the strip's height).
pub const EXPAND_PULL_PX: f32 = EXPAND_AFTER * FULL_HEIGHT;

/// `kStoriesExpandDuration` (dialogs_widget.cpp:115).
pub const REVEAL_DURATION: Duration = Duration::from_millis(200);

/// Whether a pull of `offset` px (list drawn that far below its top) expands
/// the strip. Only a finger holding the stretched edge counts: an edge thrown
/// by a fling is momentum.
pub fn pull_expands(offset: f32, held: bool) -> bool {
    held && offset >= EXPAND_PULL_PX
}

/// How far a pull has taken the collapsed stack's place, 0 to 1: the stack
/// fades as the strip grows beneath it.
pub fn pull_progress(offset: f32) -> f32 {
    (offset / EXPAND_PULL_PX).clamp(0.0, 1.0)
}

/// Whether the expanded strip has scrolled out of sight and can be dropped.
/// Not while it is still sliding in.
pub fn collapse_due(scroll_top: f32, revealing: bool) -> bool {
    !revealing && scroll_top >= FULL_HEIGHT
}

/// `anim::sineInOut`.
fn sine_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    -((std::f32::consts::PI * t).cos() - 1.0) / 2.0
}

/// The strip sliding into view: the list's scroll top eases from the strip's
/// height (hidden above) down to 0.
#[derive(Clone, Copy, Debug)]
struct Reveal {
    started: Instant,
    /// The scroll top last written by the slide, to notice a user scrolling
    /// away.
    written: f32,
}

/// Whether the strip is expanded, and its slide-in.
#[derive(Debug, Default)]
pub struct StripPull {
    expanded: bool,
    reveal: Option<Reveal>,
}

impl StripPull {
    /// The strip is a row of the list.
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    /// The strip is sliding into view.
    pub fn revealing(&self) -> bool {
        self.reveal.is_some()
    }

    /// Expand and slide the strip in from above the list's top. Returns the
    /// scroll top to start at (the strip hidden above the first row).
    pub fn expand(&mut self, now: Instant) -> f32 {
        self.expanded = true;
        self.reveal = Some(Reveal {
            started: now,
            written: FULL_HEIGHT,
        });
        FULL_HEIGHT
    }

    /// Expand without a slide (the strip is already in view).
    pub fn expand_in_place(&mut self) {
        self.expanded = true;
        self.reveal = None;
    }

    /// Drop the strip.
    pub fn collapse(&mut self) {
        self.expanded = false;
        self.reveal = None;
    }

    /// Advance the slide. `scroll_top` is where the list is now. Returns the
    /// scroll top to write, or `None` when nothing is sliding (finished, or
    /// the user scrolled down away from it). A list that sits above the
    /// slide (its range is shorter than the strip, or the user scrolled up)
    /// is left where it is: the slide only ever moves the list up.
    pub fn reveal_scroll_top(&mut self, now: Instant, scroll_top: f32) -> Option<f32> {
        let reveal = self.reveal.as_mut()?;
        if scroll_top > reveal.written + 1.0 {
            self.reveal = None;
            return None;
        }
        let t = now.saturating_duration_since(reveal.started).as_secs_f32()
            / REVEAL_DURATION.as_secs_f32();
        let top = if t >= 1.0 {
            0.0
        } else {
            (FULL_HEIGHT * (1.0 - sine_in_out(t))).min(scroll_top)
        };
        reveal.written = top;
        if t >= 1.0 {
            self.reveal = None;
        }
        Some(top)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EXPAND_PULL_PX, REVEAL_DURATION, StripPull, collapse_due, pull_expands, pull_progress,
    };
    use crate::stories_strip::FULL_HEIGHT;
    use std::time::Instant;

    #[test]
    fn a_short_pull_does_not_expand_and_a_long_one_does() {
        assert!(!pull_expands(EXPAND_PULL_PX - 1.0, true));
        assert!(pull_expands(EXPAND_PULL_PX, true));
        assert!(pull_expands(FULL_HEIGHT, true));
    }

    #[test]
    fn a_thrown_edge_never_expands() {
        assert!(!pull_expands(FULL_HEIGHT * 2.0, false));
    }

    #[test]
    fn the_threshold_is_tdesktops_ratio() {
        assert!((EXPAND_PULL_PX - 0.72 * 77.0).abs() < 1e-4);
    }

    #[test]
    fn the_stack_fades_over_the_pull() {
        assert_eq!(pull_progress(-5.0), 0.0);
        assert_eq!(pull_progress(0.0), 0.0);
        assert!((pull_progress(EXPAND_PULL_PX / 2.0) - 0.5).abs() < 1e-4);
        assert_eq!(pull_progress(200.0), 1.0);
    }

    #[test]
    fn the_strip_collapses_once_it_has_scrolled_out() {
        assert!(!collapse_due(FULL_HEIGHT - 1.0, false));
        assert!(collapse_due(FULL_HEIGHT, false));
        assert!(!collapse_due(FULL_HEIGHT * 3.0, true));
    }

    #[test]
    fn expanding_slides_the_strip_down_to_the_top() {
        let t0 = Instant::now();
        let mut pull = StripPull::default();
        assert!(!pull.expanded());
        let start = pull.expand(t0);
        assert_eq!(start, FULL_HEIGHT);
        assert!(pull.expanded() && pull.revealing());

        let mid = pull.reveal_scroll_top(t0 + REVEAL_DURATION / 2, start);
        assert!(
            mid.is_some_and(|top| (top - FULL_HEIGHT / 2.0).abs() < 0.5),
            "{mid:?}"
        );
        let end = pull.reveal_scroll_top(t0 + REVEAL_DURATION * 2, mid.unwrap_or(0.0));
        assert_eq!(end, Some(0.0));
        assert!(!pull.revealing());
        assert!(pull.expanded());
        assert_eq!(pull.reveal_scroll_top(t0 + REVEAL_DURATION * 3, 0.0), None);
    }

    #[test]
    fn scrolling_away_during_the_slide_stops_it() {
        let t0 = Instant::now();
        let mut pull = StripPull::default();
        let start = pull.expand(t0);
        let step = pull
            .reveal_scroll_top(t0 + REVEAL_DURATION / 4, start)
            .unwrap_or(0.0);
        // The user scrolled the list 30 px down since the last write.
        assert_eq!(
            pull.reveal_scroll_top(t0 + REVEAL_DURATION / 2, step + 30.0),
            None
        );
        assert!(!pull.revealing());
        assert!(pull.expanded());
    }

    #[test]
    fn a_list_that_cannot_reach_the_start_still_ends_at_the_top() {
        let t0 = Instant::now();
        let mut pull = StripPull::default();
        pull.expand(t0);
        // The list's range only allows 20 px: it was clamped there.
        let step = pull.reveal_scroll_top(t0 + REVEAL_DURATION / 8, 20.0);
        assert_eq!(step, Some(20.0));
        assert!(pull.revealing());
        let end = pull.reveal_scroll_top(t0 + REVEAL_DURATION * 2, 20.0);
        assert_eq!(end, Some(0.0));
        assert!(!pull.revealing());
    }

    #[test]
    fn collapsing_clears_the_slide() {
        let mut pull = StripPull::default();
        pull.expand(Instant::now());
        pull.collapse();
        assert!(!pull.expanded() && !pull.revealing());
    }
}
