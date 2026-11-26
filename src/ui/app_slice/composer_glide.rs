//! Floating's composer height (Web K's W12): when the draft gains or loses
//! a line, the rows under the composer pill follow its new height over
//! 250 ms on the standard curve instead of jumping. They are laid out
//! against the new height at once (so the list keeps its place) and drawn
//! offset by what the glide has left, as Web K moves its bubbles. A bar sliding open
//! over the composer (reply, edit, forward) already moves its height
//! frame by frame, so while one moves the rows follow at once. Classic
//! follows at once, as before.

use super::super::motion::Glide;
use quill::layout_style::Curve;
use std::time::{Duration, Instant};

/// A height change this soon after a composer bar moved is the bar's.
const BAR_QUIET: Duration = Duration::from_millis(50);

fn standard(t: f32) -> f32 {
    Curve::Standard.ease(t)
}

/// The composer height the rows are drawn against, gliding to the measured one.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct ComposerGlide {
    glide: Glide,
    /// When a bar over the composer last moved.
    bar_moved: Option<Instant>,
}

impl ComposerGlide {
    pub(in crate::ui) fn new(at: f32, now: Instant) -> Self {
        Self {
            glide: Glide::settled(at, now, standard),
            bar_moved: None,
        }
    }

    /// A bar over the composer moved at `now`.
    pub(in crate::ui) fn bar_moved(&mut self, now: Instant) {
        self.bar_moved = Some(now);
    }

    /// Follows `measured` at `now`, gliding over `ms` (0: at once), and
    /// returns the height to keep clear of.
    pub(in crate::ui) fn follow(&mut self, measured: f32, ms: u16, now: Instant) -> f32 {
        let bar = self
            .bar_moved
            .is_some_and(|at| now.saturating_duration_since(at) < BAR_QUIET);
        // The first measure (from no height) is where the rows start.
        let first = self.glide.target() <= 0.;
        if ms == 0 || bar || first {
            if self.glide.target() != measured || self.glide.animating(now) {
                self.glide = Glide::settled(measured, now, standard);
            }
        } else {
            self.glide
                .go(measured, Duration::from_millis(ms.into()), true, now);
        }
        self.glide.value(now)
    }

    /// Whether it is still on its way at `now`.
    pub(in crate::ui) fn moving(&self, now: Instant) -> bool {
        self.glide.animating(now)
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "composer_glide_tests.rs"]
mod tests;
