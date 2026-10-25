//! Timing rules of the viewer's video controller that are not layout: the
//! Space-hold speed boost and the delay before a stall shows a spinner.
//! Pure (no GPUI); the UI feeds key presses and render times in.

use std::time::{Duration, Instant};

/// tdesktop `mediaviewSpeedBoostHoldDelay` (media_view.style:251): Space
/// held this long plays at [`SPEED_BOOST_RATE`] until it is released.
pub const SPEED_BOOST_HOLD_MS: u64 = 200;
/// The rate a held Space plays at (`OverlayWidget::startSpeedBoost`,
/// `_speedBoostSpeed = 2.0`).
pub const SPEED_BOOST_RATE: f64 = 2.0;
/// tdesktop `kWaitingShowDelay` (media_streaming_document.cpp:30): a stall
/// shows the waiting indicator only after this long.
pub const WAITING_SHOW_DELAY_MS: u64 = 500;
/// A press this long after the last press or key repeat means the
/// release was never seen (the key-up went to another window): it starts
/// over. Longer than the slowest system key-repeat delay (2 s on macOS).
pub const BOOST_STALE_MS: u64 = 2_500;

/// What a Space press asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoostPress {
    /// A new press: check again after [`SPEED_BOOST_HOLD_MS`].
    Armed,
    /// Key repeat of a press already held: nothing to do.
    Repeat,
}

/// What a Space release asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoostRelease {
    /// A tap (released before the hold timer fired): toggle pause.
    TogglePause,
    /// The boost was on: go back to the chosen speed.
    EndBoost,
    /// No press was seen: change nothing.
    Nothing,
}

/// The Space-hold speed boost (`OverlayWidget::handleKeyPress` /
/// `handleKeyRelease`): a press arms a 200 ms timer, a release before it
/// fires toggles pause, and a hold past it plays at 2x until the release,
/// which then does not toggle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoostState {
    pressed_at: Option<Instant>,
    /// The last press or key repeat.
    last_seen: Option<Instant>,
    active: bool,
}

impl BoostState {
    /// Space went down at `now` (key repeats included). A press long after
    /// the last one ([`BOOST_STALE_MS`]) drops a hold whose release was
    /// missed, so a boost that was on ends: callers compare
    /// [`Self::is_active`] before and after.
    pub fn press(&mut self, now: Instant) -> BoostPress {
        let stale = self.last_seen.is_some_and(|seen| {
            now.saturating_duration_since(seen) > Duration::from_millis(BOOST_STALE_MS)
        });
        if stale {
            *self = Self::default();
        }
        self.last_seen = Some(now);
        if self.pressed_at.is_some() || self.active {
            return BoostPress::Repeat;
        }
        self.pressed_at = Some(now);
        BoostPress::Armed
    }

    /// The hold timer looked at `now`: `true` when the boost starts now.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.active || !self.held_long_enough(now) {
            return false;
        }
        self.active = true;
        true
    }

    /// Time left until a press becomes a hold, while one is pending.
    pub fn wait(&self, now: Instant) -> Option<Duration> {
        if self.active {
            return None;
        }
        let pressed_at = self.pressed_at?;
        let hold = Duration::from_millis(SPEED_BOOST_HOLD_MS);
        Some(hold.saturating_sub(now.saturating_duration_since(pressed_at)))
    }

    /// Space went up at `now`.
    /// tdesktop `handleKeyRelease`: a hold timer that has not fired yet
    /// makes the release a tap, however late the release is handled.
    pub fn release(&mut self) -> BoostRelease {
        let was_active = self.active;
        let pressed = self.pressed_at.is_some();
        *self = Self::default();
        match (was_active, pressed) {
            (true, _) => BoostRelease::EndBoost,
            (false, true) => BoostRelease::TogglePause,
            (false, false) => BoostRelease::Nothing,
        }
    }

    /// The key-up will never come (the window lost focus, the clip
    /// changed): forget the press. `true` when the boost was on.
    pub fn cancel(&mut self) -> bool {
        let was_active = self.active;
        *self = Self::default();
        was_active
    }

    /// Playing at the boost rate now.
    pub fn is_active(&self) -> bool {
        self.active
    }

    fn held_long_enough(&self, now: Instant) -> bool {
        self.pressed_at.is_some_and(|at| {
            now.saturating_duration_since(at) >= Duration::from_millis(SPEED_BOOST_HOLD_MS)
        })
    }
}

/// Whether a stall that began at `since` shows the waiting indicator at
/// `now` (never for one shorter than [`WAITING_SHOW_DELAY_MS`]).
pub fn waiting_shown(since: Instant, now: Instant) -> bool {
    waiting_wait(since, now).is_none()
}

/// How long until a stall that began at `since` shows its indicator;
/// `None` once it shows.
pub fn waiting_wait(since: Instant, now: Instant) -> Option<Duration> {
    let delay = Duration::from_millis(WAITING_SHOW_DELAY_MS);
    let elapsed = now.saturating_duration_since(since);
    (elapsed < delay).then(|| delay.saturating_sub(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn a_tap_toggles_pause_and_never_boosts() {
        let t0 = Instant::now();
        let mut boost = BoostState::default();
        assert_eq!(boost.press(t0), BoostPress::Armed);
        assert!(!boost.tick(ms(t0, 120)));
        assert_eq!(boost.release(), BoostRelease::TogglePause);
        assert!(!boost.is_active());
        assert_eq!(boost.wait(ms(t0, 160)), None);
    }

    #[test]
    fn a_hold_boosts_until_release_and_does_not_toggle() {
        let t0 = Instant::now();
        let mut boost = BoostState::default();
        boost.press(t0);
        assert_eq!(boost.wait(ms(t0, 50)), Some(Duration::from_millis(150)));
        // Key repeats while held change nothing.
        assert_eq!(boost.press(ms(t0, 100)), BoostPress::Repeat);
        assert!(!boost.tick(ms(t0, 199)));
        assert!(boost.tick(ms(t0, 200)));
        assert!(boost.is_active());
        // Started once: later checks and repeats do nothing.
        assert!(!boost.tick(ms(t0, 400)));
        assert_eq!(boost.press(ms(t0, 450)), BoostPress::Repeat);
        assert_eq!(boost.wait(ms(t0, 450)), None);
        assert_eq!(boost.release(), BoostRelease::EndBoost);
        assert!(!boost.is_active());
    }

    #[test]
    fn a_release_before_the_timer_fires_is_a_tap_however_late() {
        let t0 = Instant::now();
        let mut boost = BoostState::default();
        boost.press(t0);
        // The timer has not fired (a busy frame): still a tap, as in
        // tdesktop, where only the hold timer starts the boost.
        assert_eq!(boost.release(), BoostRelease::TogglePause);
        // A release with no press (the press went to another window).
        assert_eq!(boost.release(), BoostRelease::Nothing);
    }

    #[test]
    fn cancel_forgets_the_press_and_reports_a_running_boost() {
        let t0 = Instant::now();
        let mut boost = BoostState::default();
        boost.press(t0);
        assert!(!boost.cancel());
        assert_eq!(boost.press(ms(t0, 10)), BoostPress::Armed);
        assert!(boost.tick(ms(t0, 300)));
        assert!(boost.cancel());
        assert!(!boost.is_active());
        assert_eq!(boost.release(), BoostRelease::Nothing);
    }

    #[test]
    fn a_missed_release_does_not_wedge_space() {
        let t0 = Instant::now();
        let mut boost = BoostState::default();
        boost.press(t0);
        assert!(boost.tick(ms(t0, 200)));
        // Key repeats keep the hold alive.
        assert_eq!(boost.press(ms(t0, 1_000)), BoostPress::Repeat);
        assert_eq!(boost.press(ms(t0, 3_000)), BoostPress::Repeat);
        assert!(boost.is_active());
        // The release went elsewhere; the next press, seconds later, is new.
        assert_eq!(boost.press(ms(t0, 6_000)), BoostPress::Armed);
        assert!(!boost.is_active(), "the stale boost ended");
        assert_eq!(boost.release(), BoostRelease::TogglePause);
    }

    #[test]
    fn waiting_shows_only_after_half_a_second() {
        let t0 = Instant::now();
        assert!(!waiting_shown(t0, ms(t0, 0)));
        assert!(!waiting_shown(t0, ms(t0, 499)));
        assert_eq!(
            waiting_wait(t0, ms(t0, 120)),
            Some(Duration::from_millis(380))
        );
        assert!(waiting_shown(t0, ms(t0, 500)));
        assert_eq!(waiting_wait(t0, ms(t0, 800)), None);
        // A clock that went backwards never shows it early.
        assert!(!waiting_shown(ms(t0, 100), t0));
    }
}
