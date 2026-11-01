// Added by the Quill project (2026) to gpui-pre-wgpu. See QUILL-CHANGES.md
// next to this crate's Cargo.toml.

//! When to try again after the GPU device was lost, and when to stop.
//!
//! Upstream slept 350 ms on the UI thread before every attempt and retried
//! on every frame forever, rejecting software adapters so a real GPU that
//! comes back late (after suspend/resume) is not traded for a CPU renderer
//! for the rest of the session. A GPU that never returns then kept the UI
//! thread asleep most of the time, indefinitely. Here attempts back off,
//! nothing sleeps (the caller waits for the returned instant on a timer),
//! a software adapter is accepted after [`SOFTWARE_AFTER`] failures (or at
//! once when the lost device was already one, since no real GPU is coming),
//! and after [`GIVE_UP_AFTER`] failures the renderer gives up.
//!
//! std-only, so `scripts/test-device-recovery.sh` runs its tests on any host.

use std::time::{Duration, Instant};

/// Before the first attempt: lets the driver settle (upstream's sleep).
const FIRST_DELAY: Duration = Duration::from_millis(350);
const MAX_DELAY: Duration = Duration::from_secs(8);
/// Failed attempts on hardware only before software adapters are accepted.
const SOFTWARE_AFTER: u32 = 3;
/// Failed attempts in all before giving up (about 35 s).
const GIVE_UP_AFTER: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    /// Not yet: ask again at this instant.
    Wait(Instant),
    Attempt {
        allow_software: bool,
    },
    GiveUp,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecoveryPolicy {
    lost_was_software: bool,
    failures: u32,
    /// The loss, or the last failed attempt.
    since: Instant,
}

impl RecoveryPolicy {
    pub(crate) fn new(now: Instant, lost_was_software: bool) -> Self {
        Self {
            lost_was_software,
            failures: 0,
            since: now,
        }
    }

    pub(crate) fn step(&self, now: Instant) -> RecoveryStep {
        if self.failures >= GIVE_UP_AFTER {
            return RecoveryStep::GiveUp;
        }
        let due = self.since + self.delay();
        if now < due {
            return RecoveryStep::Wait(due);
        }
        RecoveryStep::Attempt {
            allow_software: self.lost_was_software || self.failures >= SOFTWARE_AFTER,
        }
    }

    pub(crate) fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        self.since = now;
    }

    fn delay(&self) -> Duration {
        FIRST_DELAY
            .saturating_mul(1 << self.failures.min(8))
            .min(MAX_DELAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt_times(policy: &mut RecoveryPolicy, start: Instant) -> Vec<(Duration, bool)> {
        let mut now = start;
        let mut out = Vec::new();
        loop {
            match policy.step(now) {
                RecoveryStep::Wait(at) => {
                    assert!(at > now);
                    now = at;
                }
                RecoveryStep::Attempt { allow_software } => {
                    out.push((now - start, allow_software));
                    policy.failed(now);
                }
                RecoveryStep::GiveUp => return out,
            }
        }
    }

    #[test]
    fn the_first_attempt_waits_for_the_driver() {
        let start = Instant::now();
        let policy = RecoveryPolicy::new(start, false);
        assert_eq!(policy.step(start), RecoveryStep::Wait(start + FIRST_DELAY));
        assert_eq!(
            policy.step(start + FIRST_DELAY),
            RecoveryStep::Attempt {
                allow_software: false
            }
        );
    }

    #[test]
    fn delays_grow_and_attempts_stop() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, false), start);
        assert_eq!(attempts.len(), GIVE_UP_AFTER as usize);
        let gaps: Vec<Duration> = attempts.windows(2).map(|w| w[1].0 - w[0].0).collect();
        assert!(gaps.windows(2).all(|g| g[1] >= g[0]), "{gaps:?}");
        assert!(gaps.iter().all(|gap| *gap <= MAX_DELAY));
        let (last, _) = attempts.last().copied().unwrap();
        assert!(
            last < Duration::from_secs(60),
            "gives up within a minute: {last:?}"
        );
    }

    #[test]
    fn software_is_accepted_only_after_hardware_failed() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, false), start);
        let software: Vec<bool> = attempts.iter().map(|(_, software)| *software).collect();
        let hardware_only = SOFTWARE_AFTER as usize;
        assert!(software[..hardware_only].iter().all(|s| !s));
        assert!(software[hardware_only..].iter().all(|s| *s));
    }

    #[test]
    fn a_lost_software_device_accepts_software_at_once() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, true), start);
        assert!(attempts.iter().all(|(_, software)| *software));
    }
}
