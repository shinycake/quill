//! When the live stream window shows its controls (the bottom bar, and in
//! full screen the title too), as tdesktop's `Calls::Group::Panel` decides:
//!
//! - In a normal window they show while the mouse is over the picture or
//!   over the controls themselves (`mouseInsideValue`, `trackControls`).
//! - Full screen or maximized (`_rtmpFull`): any mouse movement over the
//!   picture shows them and restarts a 5 s timer that hides them again
//!   (`setupFullScreenBehavior`, `kHideControlsTimeout`). Leaving the
//!   controls hides them at once.
//! - Showing and hiding slide over `st::slideWrapDuration` (150 ms,
//!   linear), and an interrupted slide turns back from where it is.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

/// `kHideControlsTimeout` in calls_group_panel.cpp.
pub(crate) const HIDE_TIMEOUT: Duration = Duration::from_secs(5);
/// lib_ui `slideWrapDuration`.
pub(crate) const SLIDE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone)]
pub(crate) struct StreamControls {
    full: bool,
    over_picture: bool,
    over_controls: bool,
    /// Full screen only: the controls were asked for (a mouse move) and
    /// hide at `hide_at`.
    wanted: bool,
    hide_at: Option<Instant>,
    /// The slide: from `from` to `to` starting at `started`.
    from: f32,
    to: f32,
    started: Option<Instant>,
}

impl Default for StreamControls {
    fn default() -> Self {
        Self {
            full: false,
            over_picture: false,
            over_controls: false,
            wanted: false,
            hide_at: None,
            from: 0.0,
            to: 0.0,
            started: None,
        }
    }
}

impl StreamControls {
    /// Whether the controls are (or are sliding to be) shown.
    pub(crate) fn shown(&self) -> bool {
        self.to > 0.5
    }

    /// The window went in or out of full screen (or maximized). Entering
    /// shows the controls and starts the timer; leaving stops the timer
    /// and follows the mouse again.
    pub(crate) fn set_full(&mut self, full: bool, now: Instant) {
        if self.full == full {
            return;
        }
        self.full = full;
        if full {
            self.wanted = true;
            self.hide_at = now.checked_add(HIDE_TIMEOUT);
        } else {
            self.wanted = false;
            self.hide_at = None;
        }
        self.update(now);
    }

    pub(crate) fn is_full(&self) -> bool {
        self.full
    }

    /// The mouse entered or left the picture. Ignored in full screen,
    /// where only movement counts (tdesktop filters `mouseInsideValue`
    /// with `!_rtmpFull`).
    pub(crate) fn hover_picture(&mut self, inside: bool, now: Instant) {
        self.over_picture = inside;
        if !self.full {
            self.update(now);
        }
    }

    /// The mouse entered or left the controls (`trackControls`): entering
    /// shows them, leaving hides them, in either mode.
    pub(crate) fn hover_controls(&mut self, inside: bool, now: Instant) {
        self.over_controls = inside;
        if self.full {
            self.wanted = inside;
            if inside {
                self.hide_at = now.checked_add(HIDE_TIMEOUT);
            }
        }
        self.update(now);
    }

    /// The mouse moved over the picture. In full screen this shows the
    /// controls and restarts the timer.
    pub(crate) fn mouse_moved(&mut self, now: Instant) {
        if !self.full {
            return;
        }
        self.wanted = true;
        self.hide_at = now.checked_add(HIDE_TIMEOUT);
        self.update(now);
    }

    /// When the full-screen timer fires, if it runs.
    pub(crate) fn hide_at(&self) -> Option<Instant> {
        self.hide_at.filter(|_| self.full && self.wanted)
    }

    /// Run the full-screen timer: hides the controls once it is due.
    pub(crate) fn tick(&mut self, now: Instant) {
        if let Some(at) = self.hide_at()
            && now >= at
        {
            self.wanted = false;
            self.hide_at = None;
            self.update(now);
        }
    }

    /// Show the controls right away without a slide (a fresh window
    /// under the mouse, the screenshot fixtures).
    #[cfg_attr(not(feature = "demo-capture"), allow(dead_code))]
    pub(crate) fn show_now(&mut self) {
        self.over_picture = true;
        self.from = 1.0;
        self.to = 1.0;
        self.started = None;
    }

    fn target(&self) -> f32 {
        let shown = if self.full {
            self.wanted
        } else {
            self.over_picture || self.over_controls
        };
        if shown { 1.0 } else { 0.0 }
    }

    fn update(&mut self, now: Instant) {
        let target = self.target();
        if (target - self.to).abs() < f32::EPSILON {
            return;
        }
        // Turn back from where the slide is now.
        self.from = self.value(now);
        self.to = target;
        self.started = Some(now);
    }

    /// The slide's value at `now`: 0 hidden, 1 shown.
    pub(crate) fn value(&self, now: Instant) -> f32 {
        let Some(started) = self.started else {
            return self.to;
        };
        let elapsed = now.saturating_duration_since(started).as_secs_f32();
        let t = (elapsed / SLIDE.as_secs_f32()).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * t
    }

    /// Whether the slide still moves at `now`.
    pub(crate) fn sliding(&self, now: Instant) -> bool {
        self.started
            .is_some_and(|started| now.saturating_duration_since(started) < SLIDE)
    }
}

#[cfg(test)]
mod tests {
    use super::{HIDE_TIMEOUT, SLIDE, StreamControls};
    use std::time::{Duration, Instant};

    #[test]
    fn normal_window_follows_the_mouse() {
        let t0 = Instant::now();
        let mut c = StreamControls::default();
        assert!(!c.shown());
        c.hover_picture(true, t0);
        assert!(c.shown());
        assert!(c.sliding(t0));
        assert_eq!(c.value(t0 + SLIDE), 1.0);
        // Over the bar (which covers the picture) it stays.
        c.hover_controls(true, t0 + SLIDE);
        c.hover_picture(false, t0 + SLIDE);
        assert!(c.shown());
        c.hover_controls(false, t0 + SLIDE * 2);
        assert!(!c.shown());
        // Moving without full screen does nothing.
        c.mouse_moved(t0 + SLIDE * 3);
        assert!(!c.shown());
        assert_eq!(c.hide_at(), None);
    }

    #[test]
    fn full_screen_hides_after_five_idle_seconds() {
        let t0 = Instant::now();
        let mut c = StreamControls::default();
        c.set_full(true, t0);
        assert!(c.shown());
        assert_eq!(c.hide_at(), Some(t0 + HIDE_TIMEOUT));
        c.tick(t0 + Duration::from_secs(4));
        assert!(c.shown());
        // A move restarts the timer.
        let moved = t0 + Duration::from_secs(4);
        c.mouse_moved(moved);
        c.tick(t0 + HIDE_TIMEOUT);
        assert!(c.shown());
        c.tick(moved + HIDE_TIMEOUT);
        assert!(!c.shown());
        // Hovering the picture alone does not bring them back.
        c.hover_picture(true, moved + HIDE_TIMEOUT);
        assert!(!c.shown());
        c.mouse_moved(moved + HIDE_TIMEOUT * 2);
        assert!(c.shown());
        // Leaving full screen follows the hover again.
        c.set_full(false, moved + HIDE_TIMEOUT * 3);
        assert!(c.shown());
        assert_eq!(c.hide_at(), None);
    }

    #[test]
    fn an_interrupted_slide_turns_back_from_where_it_is() {
        let t0 = Instant::now();
        let mut c = StreamControls::default();
        c.hover_picture(true, t0);
        let half = t0 + SLIDE / 2;
        let at_half = c.value(half);
        assert!((at_half - 0.5).abs() < 0.05);
        c.hover_picture(false, half);
        assert!((c.value(half) - at_half).abs() < f32::EPSILON);
        assert!(c.value(half + SLIDE) < f32::EPSILON);
    }
}
