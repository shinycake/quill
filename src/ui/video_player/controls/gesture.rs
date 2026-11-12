//! The controller's stall delay, one value with its timer. The timer lives
//! inside the stage that needs it, so a stage that ends takes its timer
//! with it and nothing clears the timer by hand. (The Space hold is
//! `quill::media_viewer::BoostState`, which holds its timer the same way.)

use gpui_kit::Task;
use quill::media_viewer::waiting_wait;
use std::time::{Duration, Instant};

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
    use super::{WaitingCheck, WaitingGate};
    use gpui_kit::Task;
    use std::time::{Duration, Instant};

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
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
