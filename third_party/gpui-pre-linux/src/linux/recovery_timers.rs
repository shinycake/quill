// Added by the Quill project (2026) to gpui-pre-linux. See QUILL-CHANGES.md
// next to this crate's Cargo.toml.

//! The calloop timers a window arms for GPU recovery steps, at most one of
//! each kind at a time. A draw asks again on every frame, and each ask used
//! to arm another timer: a window that recovered twice ran two hardware
//! probe chains, and every draw before a close ran added another close.
//! A timer carries the instant it was armed for and does nothing when that
//! is no longer the window's (`*_fired`).

use std::time::Instant;

#[derive(Clone, Copy, Debug)]
pub(crate) enum TimerKind {
    Retry,
    Probe,
    Close,
}

#[derive(Debug, Default)]
pub(crate) struct RecoveryTimers {
    retry: Option<Instant>,
    probe: Option<Instant>,
    closing: bool,
}

impl RecoveryTimers {
    /// A recovery attempt is due at `at`: whether to arm a timer for it.
    pub(crate) fn arm_retry(&mut self, at: Instant) -> bool {
        self.retry.replace(at) != Some(at)
    }

    /// Whether the retry timer armed for `at` is the window's current one;
    /// it is then spent.
    pub(crate) fn retry_fired(&mut self, at: Instant) -> bool {
        let current = self.retry == Some(at);
        if current {
            self.retry = None;
        }
        current
    }

    pub(crate) fn retry_pending(&self) -> bool {
        self.retry.is_some()
    }

    /// Recovered or gave up: no attempt is due.
    pub(crate) fn clear_retry(&mut self) {
        self.retry = None;
    }

    /// The renderer asks for a look for the GPU at `at` (`None`: no more
    /// looks): whether to arm a timer for it.
    pub(crate) fn arm_probe(&mut self, at: Option<Instant>) -> bool {
        let changed = self.probe != at;
        self.probe = at;
        changed && at.is_some()
    }

    /// Whether the probe timer armed for `at` is the window's current one;
    /// it is then spent.
    pub(crate) fn probe_fired(&mut self, at: Instant) -> bool {
        let current = self.probe == Some(at);
        if current {
            self.probe = None;
        }
        current
    }

    /// Recovery gave up on this window: whether to arm the close.
    pub(crate) fn arm_close(&mut self) -> bool {
        !std::mem::replace(&mut self.closing, true)
    }

    /// The timer `arm_*` asked for could not be armed: the next ask arms it.
    pub(crate) fn not_armed(&mut self, kind: TimerKind) {
        match kind {
            TimerKind::Retry => self.retry = None,
            TimerKind::Probe => self.probe = None,
            TimerKind::Close => self.closing = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RecoveryTimers, TimerKind};
    use std::time::{Duration, Instant};

    #[test]
    fn every_draw_asking_for_the_same_attempt_arms_one_timer() {
        let at = Instant::now() + Duration::from_millis(350);
        let mut timers = RecoveryTimers::default();
        let armed = (0..60).filter(|_| timers.arm_retry(at)).count();
        assert_eq!(armed, 1);
        assert!(timers.retry_pending());
        assert!(timers.retry_fired(at));
        assert!(!timers.retry_pending());
        // The next failed attempt asks for a later one.
        assert!(timers.arm_retry(at + Duration::from_secs(1)));
    }

    #[test]
    fn a_second_recovery_does_not_start_a_second_probe_chain() {
        let start = Instant::now();
        let first = start + Duration::from_secs(10);
        let mut timers = RecoveryTimers::default();
        assert!(timers.arm_probe(Some(first)));
        assert!(!timers.arm_probe(Some(first)));
        // Recovered again before the first look: the look moves, and the
        // timer armed for the old instant does nothing when it fires.
        let second = start + Duration::from_secs(25);
        assert!(timers.arm_probe(Some(second)));
        assert!(!timers.probe_fired(first));
        assert!(timers.probe_fired(second));
        // A look runs: the poll arms its check, and every draw until then
        // asks for that same check (`HardwareProbe::next_at`).
        let check = second + Duration::from_millis(250);
        assert!(timers.arm_probe(Some(check)));
        assert_eq!((0..15).filter(|_| timers.arm_probe(Some(check))).count(), 0);
        assert!(timers.probe_fired(check));
        assert!(!timers.arm_probe(None));
    }

    #[test]
    fn a_window_recovery_gave_up_on_closes_once() {
        let mut timers = RecoveryTimers::default();
        assert!(timers.arm_close());
        assert!(!timers.arm_close());
    }

    /// The event loop was busy: the next draw's ask arms the timer, instead
    /// of the window waiting for one that never fires.
    #[test]
    fn a_timer_that_could_not_be_armed_is_asked_for_again() {
        let at = Instant::now();
        let mut timers = RecoveryTimers::default();
        assert!(timers.arm_retry(at));
        timers.not_armed(TimerKind::Retry);
        assert!(!timers.retry_pending());
        assert!(timers.arm_retry(at));
        assert!(timers.arm_probe(Some(at)));
        timers.not_armed(TimerKind::Probe);
        assert!(timers.arm_probe(Some(at)));
        assert!(timers.arm_close());
        timers.not_armed(TimerKind::Close);
        assert!(timers.arm_close());
    }
}
