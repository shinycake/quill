//! Horizontal trackpad swipes over the viewer (pure, no GPUI), after
//! tdesktop `OverlayWidget::setupSwipeNavigation` and
//! `Ui::Controls::SetupSwipeHandler`: a two-finger swipe pulls a round
//! arrow in from the edge, and letting go once it is fully in moves one
//! item. The gesture locks horizontal or vertical on its first clear
//! movement; a vertical one is left to `WheelNav`.

use super::wheel::{VIEWER_TRACKPAD_GESTURE_GAP, WheelSource};
use std::time::Instant;

/// Swipe distance that arms the gesture (tdesktop `kThresholdWidth`).
pub const VIEWER_SWIPE_THRESHOLD_PX: f32 = 50.0;

/// The horizontal distance must beat the vertical one by more than this
/// to lock a gesture horizontal (tdesktop `kOrientationThreshold`).
const ORIENTATION_LOCK_PX: f32 = 1.0;

/// An armed gesture disarms when pulled back below this ratio
/// (tdesktop `kResetReachedOn`).
const DISARM_RATIO: f32 = 0.95;

/// Which item a swipe moves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDir {
    Next,
    Previous,
}

/// One scroll event over the viewer. Deltas are in px; negative `dx`
/// moves the content left, toward the next item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwipeInput {
    pub dx: f32,
    pub dy: f32,
    pub source: WheelSource,
    pub at: Instant,
    pub can_next: bool,
    pub can_prev: bool,
}

/// What a scroll event does to the swipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeAction {
    /// Not a swipe event: `WheelNav` gets it.
    None,
    /// Part of a swipe: `WheelNav` must not see it.
    Capture,
    /// A swipe that lost its end event finished armed when a new gesture
    /// began.
    Step(SwipeDir),
}

/// The arrow a swipe in progress pulls in from the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwipeIndicator {
    pub dir: SwipeDir,
    /// How far it is pulled in, 0 to 1.
    pub ratio: f32,
    /// Letting go now moves to the item.
    pub reached: bool,
}

/// The trackpad gesture over the viewer, as far as swiping goes.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SwipeNav {
    gesture: Gesture,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Gesture {
    #[default]
    Idle,
    /// Neither clearly horizontal nor vertical yet: the deltas so far.
    Undecided {
        dx: f32,
        dy: f32,
        phased: bool,
        last_at: Instant,
    },
    /// Vertical, or toward an end with no item there: `WheelNav`'s.
    Elsewhere {
        last_at: Instant,
    },
    Pulling(Pull),
    /// Let go: the rest of it (macOS momentum) is swallowed.
    Released {
        last_at: Instant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pull {
    dir: SwipeDir,
    /// Distance swiped toward `dir`, in px.
    pulled: f32,
    /// The pull at which the gesture is armed. It slides with the pull
    /// (tdesktop `RatioRange`), so backing off one threshold from any
    /// point cancels.
    top: f32,
    reached: bool,
    /// The gesture began with a begin phase, so an end phase will close
    /// it; without one it ends after a pause.
    phased: bool,
    last_at: Instant,
}

impl Pull {
    fn new(dir: SwipeDir, pulled: f32, phased: bool, at: Instant) -> Self {
        let mut pull = Self {
            dir,
            pulled: 0.0,
            top: VIEWER_SWIPE_THRESHOLD_PX,
            reached: false,
            phased,
            last_at: at,
        };
        pull.pull_to(pulled);
        pull
    }

    fn pull_by(&mut self, dx: f32, at: Instant) {
        let along = match self.dir {
            SwipeDir::Next => -dx,
            SwipeDir::Previous => dx,
        };
        self.pull_to(self.pulled + along);
        self.last_at = at;
    }

    fn pull_to(&mut self, pulled: f32) {
        self.pulled = pulled;
        self.top = self.top.min(pulled + VIEWER_SWIPE_THRESHOLD_PX).max(pulled);
        let ratio = self.ratio();
        if ratio >= 1.0 {
            self.reached = true;
        } else if ratio < DISARM_RATIO {
            self.reached = false;
        }
    }

    fn ratio(&self) -> f32 {
        (1.0 - (self.top - self.pulled) / VIEWER_SWIPE_THRESHOLD_PX).clamp(0.0, 1.0)
    }
}

impl SwipeNav {
    pub fn feed(&mut self, input: SwipeInput) -> SwipeAction {
        let SwipeInput {
            dx,
            dy,
            source,
            at,
            can_next,
            can_prev,
        } = input;
        let WheelSource::Trackpad { begins_gesture } = source else {
            return SwipeAction::None;
        };
        if !dx.is_finite() || !dy.is_finite() {
            return SwipeAction::None;
        }
        let finished = if self.continues(begins_gesture, at) {
            None
        } else {
            let dir = self.end(at);
            self.gesture = Gesture::Idle;
            dir
        };
        let (sum_dx, sum_dy, phased) = match &mut self.gesture {
            Gesture::Idle => (dx, dy, begins_gesture),
            Gesture::Undecided {
                dx: sum_dx,
                dy: sum_dy,
                phased,
                ..
            } => (*sum_dx + dx, *sum_dy + dy, *phased),
            Gesture::Elsewhere { last_at } => {
                *last_at = at;
                return SwipeAction::None;
            }
            Gesture::Released { last_at } => {
                *last_at = at;
                return SwipeAction::Capture;
            }
            Gesture::Pulling(pull) => {
                pull.pull_by(dx, at);
                return SwipeAction::Capture;
            }
        };
        let lead = sum_dx.abs() - sum_dy.abs();
        let action = if lead > ORIENTATION_LOCK_PX {
            let dir = if sum_dx < 0.0 {
                SwipeDir::Next
            } else {
                SwipeDir::Previous
            };
            let open = match dir {
                SwipeDir::Next => can_next,
                SwipeDir::Previous => can_prev,
            };
            if open {
                self.gesture = Gesture::Pulling(Pull::new(dir, sum_dx.abs(), phased, at));
                SwipeAction::Capture
            } else {
                self.gesture = Gesture::Elsewhere { last_at: at };
                SwipeAction::None
            }
        } else if lead < -ORIENTATION_LOCK_PX {
            self.gesture = Gesture::Elsewhere { last_at: at };
            SwipeAction::None
        } else {
            self.gesture = Gesture::Undecided {
                dx: sum_dx,
                dy: sum_dy,
                phased,
                last_at: at,
            };
            SwipeAction::None
        };
        finished.map_or(action, SwipeAction::Step)
    }

    /// The gesture ended (an end phase, or the pause after a gesture
    /// without phases). Returns the item to move to when it was armed.
    pub fn end(&mut self, at: Instant) -> Option<SwipeDir> {
        match self.gesture {
            Gesture::Pulling(pull) => {
                // Only a phased gesture (macOS) has momentum to swallow.
                self.gesture = if pull.phased {
                    Gesture::Released { last_at: at }
                } else {
                    Gesture::Idle
                };
                (pull.ratio() >= 1.0).then_some(pull.dir)
            }
            Gesture::Released { .. } => {
                self.gesture = Gesture::Released { last_at: at };
                None
            }
            Gesture::Idle | Gesture::Undecided { .. } | Gesture::Elsewhere { .. } => {
                self.gesture = Gesture::Idle;
                None
            }
        }
    }

    /// Another item is shown: a pull in progress is dropped, and the rest
    /// of its gesture is swallowed.
    pub fn new_item(&mut self) {
        if let Gesture::Pulling(pull) = self.gesture {
            self.gesture = Gesture::Released {
                last_at: pull.last_at,
            };
        }
    }

    /// When a swipe without phases ends unless another event arrives.
    pub fn timeout_at(&self) -> Option<Instant> {
        match self.gesture {
            Gesture::Pulling(pull) if !pull.phased => {
                pull.last_at.checked_add(VIEWER_TRACKPAD_GESTURE_GAP)
            }
            _ => None,
        }
    }

    pub fn indicator(&self) -> Option<SwipeIndicator> {
        let Gesture::Pulling(pull) = self.gesture else {
            return None;
        };
        let ratio = pull.ratio();
        (ratio > 0.0).then_some(SwipeIndicator {
            dir: pull.dir,
            ratio,
            reached: pull.reached,
        })
    }

    /// Whether this event belongs to the gesture in progress. A begin
    /// phase or a pause starts a new one, except that a phased swipe
    /// waits for its end phase however long the fingers rest.
    fn continues(&self, begins_gesture: bool, at: Instant) -> bool {
        let last_at = match self.gesture {
            _ if begins_gesture => return false,
            Gesture::Idle => return false,
            Gesture::Pulling(pull) if pull.phased => return true,
            Gesture::Pulling(Pull { last_at, .. })
            | Gesture::Undecided { last_at, .. }
            | Gesture::Elsewhere { last_at }
            | Gesture::Released { last_at } => last_at,
        };
        at.saturating_duration_since(last_at) < VIEWER_TRACKPAD_GESTURE_GAP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn ms(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    fn event(dx: f32, dy: f32, begins_gesture: bool, at: Instant) -> SwipeInput {
        SwipeInput {
            dx,
            dy,
            source: WheelSource::Trackpad { begins_gesture },
            at,
            can_next: true,
            can_prev: true,
        }
    }

    /// Feeds a phased gesture of horizontal deltas, 16 ms apart, without
    /// its end phase. Returns the time of the last event.
    fn swipe(nav: &mut SwipeNav, start: Instant, deltas: &[f32]) -> Instant {
        let mut at = start;
        for (frame, dx) in deltas.iter().enumerate() {
            at = ms(start, frame as u64 * 16);
            assert_eq!(
                nav.feed(event(*dx, 0.0, frame == 0, at)),
                SwipeAction::Capture,
                "a horizontal swipe toward an item is captured"
            );
        }
        at
    }

    #[test]
    fn a_short_swipe_cancels() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last = swipe(&mut nav, now, &[-15.0, -15.0]);
        let shown = nav.indicator().expect("pulled in part way");
        assert_eq!(shown.dir, SwipeDir::Next);
        assert!((shown.ratio - 0.6).abs() < 1e-4);
        assert!(!shown.reached);
        assert_eq!(nav.end(last), None);
        assert_eq!(nav.indicator(), None);
    }

    #[test]
    fn a_full_swipe_steps_once() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last = swipe(&mut nav, now, &[-20.0, -25.0, -30.0]);
        let shown = nav.indicator().expect("pulled in");
        assert_eq!(shown.ratio, 1.0);
        assert!(shown.reached);
        assert_eq!(nav.end(last), Some(SwipeDir::Next));
        assert_eq!(nav.indicator(), None);
    }

    #[test]
    fn a_right_swipe_moves_to_the_previous_item() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last = swipe(&mut nav, now, &[30.0, 30.0]);
        assert_eq!(nav.end(last), Some(SwipeDir::Previous));
    }

    #[test]
    fn momentum_after_the_release_does_not_step_again() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last = swipe(&mut nav, now, &[-60.0]);
        assert_eq!(nav.end(last), Some(SwipeDir::Next));
        nav.new_item();
        for frame in 1..40 {
            let momentum = event(-30.0, 0.0, false, ms(last, frame * 16));
            assert_eq!(nav.feed(momentum), SwipeAction::Capture);
            assert_eq!(nav.indicator(), None, "momentum pulls nothing in");
        }
        let after = ms(last, 40 * 16);
        assert_eq!(nav.end(after), None);
        let next = event(-60.0, 0.0, true, ms(after, 16));
        assert_eq!(nav.feed(next), SwipeAction::Capture, "a new gesture swipes");
        assert_eq!(nav.end(ms(after, 16)), Some(SwipeDir::Next));
    }

    #[test]
    fn pulling_back_one_threshold_from_an_overshoot_cancels() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last = swipe(&mut nav, now, &[-200.0, 30.0]);
        let shown = nav.indicator().expect("still pulled in");
        assert!(!shown.reached, "30 px back disarms it");
        assert!((shown.ratio - 0.4).abs() < 1e-4);
        assert_eq!(nav.end(last), None);
    }

    #[test]
    fn a_vertical_first_gesture_is_not_captured() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        assert_eq!(nav.feed(event(-5.0, -40.0, true, now)), SwipeAction::None);
        for frame in 1..5 {
            let sideways = event(-60.0, 0.0, false, ms(now, frame * 16));
            assert_eq!(nav.feed(sideways), SwipeAction::None, "locked vertical");
        }
        assert_eq!(nav.indicator(), None);
        assert_eq!(nav.end(ms(now, 80)), None);
    }

    #[test]
    fn a_gesture_locks_once_it_leans_clearly_one_way() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        assert_eq!(nav.feed(event(0.0, 0.0, true, now)), SwipeAction::None);
        assert_eq!(
            nav.feed(event(-1.0, -0.5, false, ms(now, 16))),
            SwipeAction::None
        );
        assert_eq!(
            nav.feed(event(-20.0, -1.0, false, ms(now, 32))),
            SwipeAction::Capture
        );
        let shown = nav.indicator().expect("pulled in");
        assert!((shown.ratio - 21.0 / VIEWER_SWIPE_THRESHOLD_PX).abs() < 1e-4);
    }

    #[test]
    fn no_next_item_means_no_capture() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let last_item = SwipeInput {
            can_next: false,
            ..event(-60.0, 0.0, true, now)
        };
        assert_eq!(nav.feed(last_item), SwipeAction::None);
        assert_eq!(nav.indicator(), None);
        assert_eq!(nav.end(now), None);
    }

    #[test]
    fn a_mouse_wheel_never_swipes() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        let wheel = SwipeInput {
            source: WheelSource::Wheel,
            ..event(-120.0, 0.0, false, now)
        };
        assert_eq!(nav.feed(wheel), SwipeAction::None);
        assert_eq!(nav.indicator(), None);
    }

    #[test]
    fn a_swipe_without_phases_ends_after_a_pause() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        assert_eq!(
            nav.feed(event(-35.0, 0.0, false, now)),
            SwipeAction::Capture
        );
        let last = ms(now, 16);
        assert_eq!(
            nav.feed(event(-35.0, 0.0, false, last)),
            SwipeAction::Capture
        );
        let deadline = nav.timeout_at().expect("a timeout without phases");
        assert_eq!(deadline, last + VIEWER_TRACKPAD_GESTURE_GAP);
        assert_eq!(nav.end(deadline), Some(SwipeDir::Next));
        assert_eq!(nav.timeout_at(), None);
        let again = event(-60.0, 0.0, false, ms(deadline, 16));
        assert_eq!(
            nav.feed(again),
            SwipeAction::Capture,
            "the next swipe starts fresh"
        );
    }

    #[test]
    fn a_phased_swipe_has_no_timeout() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        swipe(&mut nav, now, &[-60.0]);
        assert_eq!(nav.timeout_at(), None);
        let rested = event(-5.0, 0.0, false, ms(now, 2_000));
        assert_eq!(
            nav.feed(rested),
            SwipeAction::Capture,
            "still the same swipe"
        );
        assert!(nav.indicator().is_some_and(|shown| shown.reached));
    }

    #[test]
    fn a_new_gesture_finishes_a_swipe_that_lost_its_end() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        swipe(&mut nav, now, &[-60.0]);
        let next = event(0.0, -30.0, true, ms(now, 500));
        assert_eq!(nav.feed(next), SwipeAction::Step(SwipeDir::Next));
        assert_eq!(nav.indicator(), None);
    }

    #[test]
    fn a_new_item_drops_the_pull() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        swipe(&mut nav, now, &[-60.0]);
        nav.new_item();
        assert_eq!(nav.indicator(), None);
        let rest = event(-10.0, 0.0, false, ms(now, 16));
        assert_eq!(nav.feed(rest), SwipeAction::Capture);
        assert_eq!(nav.end(ms(now, 16)), None);
    }

    #[test]
    fn non_finite_deltas_do_nothing() {
        let now = Instant::now();
        let mut nav = SwipeNav::default();
        assert_eq!(nav.feed(event(f32::NAN, 0.0, true, now)), SwipeAction::None);
        assert_eq!(
            nav.feed(event(f32::INFINITY, 0.0, true, now)),
            SwipeAction::None
        );
        assert_eq!(nav.indicator(), None);
    }
}
