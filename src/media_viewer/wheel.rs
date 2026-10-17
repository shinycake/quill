//! Wheel and trackpad scrolling over the viewer (pure, no GPUI), after
//! tdesktop `OverlayWidget::handleWheelEvent`: a vertical scroll moves to
//! the next or previous item one notch at a time, and with the primary
//! modifier it zooms at the pointer. Quill also zooms while the picture
//! is zoomed in, so the wheel can zoom back out to fit.

use super::transform::VIEWER_WHEEL_NOTCH_PX;
use std::time::{Duration, Instant};

/// Trackpad scroll distance that moves one item: three wheel notches, so
/// a light two-finger flick does not count as a notch.
pub const VIEWER_TRACKPAD_STEP_PX: f32 = 3.0 * VIEWER_WHEEL_NOTCH_PX;

/// A trackpad gesture moves at most one item. Its events end when a new
/// gesture begins or after a pause this long: macOS momentum events and
/// Wayland scrolls carry no begin phase.
pub const VIEWER_TRACKPAD_GESTURE_GAP: Duration = Duration::from_millis(300);

/// Where a scroll event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelSource {
    /// A mouse wheel: line deltas.
    Wheel,
    /// A trackpad or a smooth-scrolling mouse: pixel deltas.
    Trackpad { begins_gesture: bool },
}

impl WheelSource {
    fn step_px(self) -> f32 {
        match self {
            Self::Wheel => VIEWER_WHEEL_NOTCH_PX,
            Self::Trackpad { .. } => VIEWER_TRACKPAD_STEP_PX,
        }
    }
}

/// One scroll event over the viewer. Deltas are in px (a wheel line is
/// `VIEWER_WHEEL_NOTCH_PX`); positive `dy` scrolls up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelInput {
    pub dx: f32,
    pub dy: f32,
    /// Cmd on macOS, Ctrl elsewhere.
    pub primary: bool,
    pub source: WheelSource,
    /// The picture is zoomed in (`ViewerZoom::is_zoomed`).
    pub zoomed: bool,
    pub at: Instant,
}

/// What a scroll event does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WheelAction {
    None,
    /// Zoom at the pointer by `wheel_zoom_factor(delta_y)`.
    Zoom {
        delta_y: f32,
    },
    Next,
    Previous,
}

/// The scroll distance gathered toward the next step, and the trackpad
/// gesture that already moved or zoomed.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelNav {
    accum: f32,
    spent_at: Option<Instant>,
}

impl WheelNav {
    pub fn feed(&mut self, input: WheelInput) -> WheelAction {
        let WheelInput {
            dx,
            dy,
            primary,
            source,
            zoomed,
            at,
        } = input;
        let spent = self.continue_gesture(source, at);
        if !dx.is_finite() || !dy.is_finite() || dx.abs() > dy.abs() {
            self.accum = 0.0;
            return WheelAction::None;
        }
        if primary || zoomed {
            self.accum = 0.0;
            self.spend(source, at);
            return WheelAction::Zoom { delta_y: dy };
        }
        if spent {
            return WheelAction::None;
        }
        self.accum += dy;
        if self.accum.abs() < source.step_px() {
            return WheelAction::None;
        }
        let action = if self.accum < 0.0 {
            WheelAction::Next
        } else {
            WheelAction::Previous
        };
        self.accum = 0.0;
        self.spend(source, at);
        action
    }

    /// Another item is shown: drop the gathered delta, but keep the
    /// trackpad gesture, so its momentum does not move again.
    pub fn new_item(&mut self) {
        self.accum = 0.0;
    }

    /// Whether this trackpad event belongs to a gesture that already
    /// moved or zoomed (keeping that gesture alive). A new gesture starts
    /// from zero.
    fn continue_gesture(&mut self, source: WheelSource, at: Instant) -> bool {
        let WheelSource::Trackpad { begins_gesture } = source else {
            return false;
        };
        if begins_gesture {
            self.accum = 0.0;
        }
        let spent = !begins_gesture
            && self.spent_at.is_some_and(|last| {
                at.saturating_duration_since(last) < VIEWER_TRACKPAD_GESTURE_GAP
            });
        self.spent_at = spent.then_some(at);
        spent
    }

    fn spend(&mut self, source: WheelSource, at: Instant) {
        if matches!(source, WheelSource::Trackpad { .. }) {
            self.spent_at = Some(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTCH: f32 = VIEWER_WHEEL_NOTCH_PX;

    fn wheel(dy: f32, at: Instant) -> WheelInput {
        WheelInput {
            dx: 0.0,
            dy,
            primary: false,
            source: WheelSource::Wheel,
            zoomed: false,
            at,
        }
    }

    fn trackpad(dy: f32, begins_gesture: bool, at: Instant) -> WheelInput {
        WheelInput {
            source: WheelSource::Trackpad { begins_gesture },
            ..wheel(dy, at)
        }
    }

    fn ms(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_notch_down_moves_next_and_up_moves_previous() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(wheel(-NOTCH, now)), WheelAction::Next);
        assert_eq!(nav.feed(wheel(NOTCH, now)), WheelAction::Previous);
    }

    #[test]
    fn small_deltas_add_up_to_one_step() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let third = -NOTCH / 3.0 - 0.01;
        assert_eq!(nav.feed(wheel(third, now)), WheelAction::None);
        assert_eq!(nav.feed(wheel(third, now)), WheelAction::None);
        assert_eq!(nav.feed(wheel(third, now)), WheelAction::Next);
        assert_eq!(nav.feed(wheel(third, now)), WheelAction::None);
    }

    #[test]
    fn one_large_event_moves_once() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(wheel(-10.0 * NOTCH, now)), WheelAction::Next);
        assert_eq!(nav.feed(wheel(-NOTCH / 2.0, now)), WheelAction::None);
    }

    #[test]
    fn a_horizontal_scroll_does_nothing_and_drops_the_gathered_delta() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(wheel(-NOTCH * 0.8, now)), WheelAction::None);
        let sideways = WheelInput {
            dx: -NOTCH,
            ..wheel(-NOTCH * 0.5, now)
        };
        assert_eq!(nav.feed(sideways), WheelAction::None);
        assert_eq!(nav.feed(wheel(-NOTCH * 0.8, now)), WheelAction::None);
    }

    #[test]
    fn the_primary_modifier_zooms_instead_of_moving() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let input = WheelInput {
            primary: true,
            ..wheel(NOTCH, now)
        };
        assert_eq!(nav.feed(input), WheelAction::Zoom { delta_y: NOTCH });
    }

    #[test]
    fn a_zoomed_picture_zooms_on_a_plain_scroll() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let input = WheelInput {
            zoomed: true,
            ..wheel(-NOTCH, now)
        };
        assert_eq!(nav.feed(input), WheelAction::Zoom { delta_y: -NOTCH });
    }

    #[test]
    fn a_trackpad_needs_three_notches() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(trackpad(-NOTCH, true, now)), WheelAction::None);
        assert_eq!(nav.feed(trackpad(-NOTCH, false, now)), WheelAction::None);
        assert_eq!(
            nav.feed(trackpad(-NOTCH, false, ms(now, 16))),
            WheelAction::Next
        );
    }

    #[test]
    fn a_trackpad_gesture_moves_once_until_a_new_gesture_or_a_pause() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let flick = -VIEWER_TRACKPAD_STEP_PX;
        assert_eq!(nav.feed(trackpad(flick, true, now)), WheelAction::Next);
        for frame in 1..40 {
            assert_eq!(
                nav.feed(trackpad(flick, false, ms(now, frame * 16))),
                WheelAction::None,
                "momentum stays in the gesture"
            );
        }
        let last = ms(now, 39 * 16);
        assert_eq!(
            nav.feed(trackpad(flick, true, ms(last, 16))),
            WheelAction::Next
        );
        let gap = VIEWER_TRACKPAD_GESTURE_GAP.as_millis() as u64;
        assert_eq!(
            nav.feed(trackpad(flick, false, ms(last, 16 + gap))),
            WheelAction::Next
        );
    }

    #[test]
    fn a_new_trackpad_gesture_drops_the_last_gestures_leftover() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(trackpad(-60.0, true, now)), WheelAction::None);
        assert_eq!(
            nav.feed(trackpad(-40.0, false, ms(now, 16))),
            WheelAction::None
        );
        assert_eq!(
            nav.feed(trackpad(-50.0, true, ms(now, 32))),
            WheelAction::None,
            "100 px left over from the last gesture does not count"
        );
    }

    #[test]
    fn a_trackpad_gesture_that_zoomed_does_not_move() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let zooming_out = WheelInput {
            zoomed: true,
            ..trackpad(-NOTCH, true, now)
        };
        assert_eq!(nav.feed(zooming_out), WheelAction::Zoom { delta_y: -NOTCH });
        let at_fit = trackpad(-VIEWER_TRACKPAD_STEP_PX, false, ms(now, 16));
        assert_eq!(nav.feed(at_fit), WheelAction::None);
    }

    #[test]
    fn a_wheel_is_not_held_by_a_trackpad_gesture() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let flick = trackpad(-VIEWER_TRACKPAD_STEP_PX, true, now);
        assert_eq!(nav.feed(flick), WheelAction::Next);
        assert_eq!(nav.feed(wheel(-NOTCH, ms(now, 16))), WheelAction::Next);
    }

    #[test]
    fn non_finite_deltas_do_nothing() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(wheel(f32::NAN, now)), WheelAction::None);
        assert_eq!(nav.feed(wheel(f32::INFINITY, now)), WheelAction::None);
        assert_eq!(nav.feed(wheel(-NOTCH, now)), WheelAction::Next);
    }

    #[test]
    fn a_new_item_drops_the_gathered_delta() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        assert_eq!(nav.feed(wheel(-NOTCH * 0.8, now)), WheelAction::None);
        nav.new_item();
        assert_eq!(nav.feed(wheel(-NOTCH * 0.8, now)), WheelAction::None);
    }

    #[test]
    fn a_new_item_keeps_the_gesture_that_moved_to_it() {
        let now = Instant::now();
        let mut nav = WheelNav::default();
        let flick = -VIEWER_TRACKPAD_STEP_PX;
        assert_eq!(nav.feed(trackpad(flick, true, now)), WheelAction::Next);
        nav.new_item();
        assert_eq!(
            nav.feed(trackpad(flick, false, ms(now, 40))),
            WheelAction::None
        );
    }
}
