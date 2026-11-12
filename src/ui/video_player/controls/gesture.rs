//! The controller's key gestures, each one value: the Space hold with its timer, and the stall delay with its
//! timer. A timer lives inside the stage that needs it, so a stage that
//! ends takes its timer with it and nothing clears the timer by hand.

use gpui_kit::Task;
use quill::media_viewer::{BoostPress, BoostRelease, BoostState, waiting_wait};
use std::time::{Duration, Instant};

/// Space held to play at 2×: the press state and the check that fires
/// [`SPEED_BOOST_HOLD_MS`](quill::media_viewer::SPEED_BOOST_HOLD_MS) after
/// the key went down. The timer is kept only while the press waits for it
/// (`Armed`); every move out of that stage drops it.
#[derive(Default)]
pub(crate) struct SpaceHold {
    state: BoostState,
    timer: Option<Task<()>>,
}

impl SpaceHold {
    /// Whatever left the armed stage no longer needs its timer.
    fn settle(&mut self) {
        if !matches!(self.state, BoostState::Armed { .. }) {
            self.timer = None;
        }
    }

    pub(crate) fn press(&mut self, now: Instant) -> BoostPress {
        let press = self.state.press(now);
        self.settle();
        press
    }

    /// Keep the check that fires when the hold delay is up; dropped at once
    /// if no press waits for it.
    pub(crate) fn arm(&mut self, timer: Task<()>) {
        self.timer = Some(timer);
        self.settle();
    }

    pub(crate) fn tick(&mut self, now: Instant) -> bool {
        let started = self.state.tick(now);
        self.settle();
        started
    }

    pub(crate) fn release(&mut self) -> BoostRelease {
        let release = self.state.release();
        self.settle();
        release
    }

    /// Forget the press; `true` when a boost was on.
    pub(crate) fn cancel(&mut self) -> bool {
        let was_on = self.state.cancel();
        self.settle();
        was_on
    }

    pub(crate) fn is_active(&self) -> bool {
        self.state.is_active()
    }

    #[cfg(test)]
    pub(crate) fn holds_timer(&self) -> bool {
        self.timer.is_some()
    }
}

/// The delay before a stall shows its spinner (tdesktop
/// `kWaitingShowDelay`): the stage remembers when waiting began and holds
/// the timer that redraws when the delay is up.
#[derive(Default)]
pub(crate) enum WaitingGate {
    /// Not waiting.
    #[default]
    Idle,
    /// Waiting since `since`, the delay not up; `timer` redraws when it is
    /// (`None` until [`WaitingGate::arm`]).
    Delayed {
        since: Instant,
        timer: Option<Task<()>>,
    },
    /// Waiting past the delay: the spinner shows.
    Shown,
}

/// What [`WaitingGate::observe`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaitingCheck {
    /// Not waiting, or the delay is up and the spinner shows.
    Settled { shown: bool },
    /// Waiting, not long enough yet: redraw after this long.
    Pending(Duration),
}

impl WaitingGate {
    /// Feed whether the clip waits now. A pending delay that already has
    /// its timer reports `Settled { shown: false }`.
    pub(crate) fn observe(&mut self, waiting: bool, now: Instant) -> WaitingCheck {
        if !waiting {
            *self = Self::Idle;
            return WaitingCheck::Settled { shown: false };
        }
        let since = match self {
            Self::Delayed { since, .. } => *since,
            Self::Idle => now,
            Self::Shown => return WaitingCheck::Settled { shown: true },
        };
        match waiting_wait(since, now) {
            None => {
                *self = Self::Shown;
                WaitingCheck::Settled { shown: true }
            }
            Some(wait) => {
                match self {
                    Self::Delayed { timer: Some(_), .. } => {
                        return WaitingCheck::Settled { shown: false };
                    }
                    Self::Delayed { .. } => {}
                    Self::Idle | Self::Shown => {
                        *self = Self::Delayed { since, timer: None };
                    }
                }
                WaitingCheck::Pending(wait)
            }
        }
    }

    /// Keep the timer that redraws once the delay is up (dropped if the
    /// wait already ended).
    pub(crate) fn arm(&mut self, armed: Task<()>) {
        if let Self::Delayed { timer, .. } = self {
            *timer = Some(armed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SpaceHold, WaitingCheck, WaitingGate};
    use gpui_kit::Task;
    use std::time::{Duration, Instant};

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn the_hold_timer_lives_only_while_the_press_waits_for_it() {
        let t0 = Instant::now();
        let mut hold = SpaceHold::default();
        hold.arm(Task::ready(()));
        assert!(!hold.holds_timer(), "no press waits for it");
        hold.press(t0);
        hold.arm(Task::ready(()));
        assert!(hold.holds_timer());
        assert!(hold.tick(ms(t0, 250)), "the hold starts the boost");
        assert!(!hold.holds_timer(), "the stage ended with the timer");
        hold.press(ms(t0, 300));
        assert!(!hold.holds_timer());
        hold.arm(Task::ready(()));
        assert!(hold.cancel(), "a running boost ends");
        assert!(!hold.holds_timer());
        hold.press(ms(t0, 400));
        hold.arm(Task::ready(()));
        hold.release();
        assert!(!hold.holds_timer(), "a tap drops the check");
    }

    #[test]
    fn a_wait_that_ends_takes_its_timer_and_a_shown_spinner_needs_none() {
        let t0 = Instant::now();
        let mut gate = WaitingGate::default();
        gate.arm(Task::ready(()));
        assert!(matches!(gate, WaitingGate::Idle), "nothing waits for it");
        assert_eq!(
            gate.observe(true, ms(t0, 0)),
            WaitingCheck::Pending(Duration::from_millis(500))
        );
        gate.arm(Task::ready(()));
        assert_eq!(
            gate.observe(true, ms(t0, 100)),
            WaitingCheck::Settled { shown: false },
            "the timer is already set"
        );
        assert_eq!(
            gate.observe(false, ms(t0, 200)),
            WaitingCheck::Settled { shown: false }
        );
        assert!(matches!(gate, WaitingGate::Idle));
        gate.observe(true, ms(t0, 300));
        assert_eq!(
            gate.observe(true, ms(t0, 900)),
            WaitingCheck::Settled { shown: true }
        );
        assert!(matches!(gate, WaitingGate::Shown));
    }
}
