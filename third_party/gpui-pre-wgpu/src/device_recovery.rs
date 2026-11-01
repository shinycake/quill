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
//! A window whose context another window already recreated only adopts it
//! ([`Target::Adopt`]): its first attempt is immediate, and failures back
//! off and give up the same way.
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

/// What a recovery is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// Make a new shared context: no window has one.
    Recreate { lost_was_software: bool },
    /// Use the shared context another window recreated.
    Adopt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attempt {
    Recreate { allow_software: bool },
    Adopt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    /// Not yet: ask again at this instant.
    Wait(Instant),
    Attempt(Attempt),
    GiveUp,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RecoveryPolicy {
    target: Target,
    failures: u32,
    /// The loss, or the last failed attempt.
    since: Instant,
}

impl RecoveryPolicy {
    pub(crate) fn new(now: Instant, target: Target) -> Self {
        Self {
            target,
            failures: 0,
            since: now,
        }
    }

    pub(crate) fn target(&self) -> Target {
        self.target
    }

    pub(crate) fn step(&self, now: Instant) -> RecoveryStep {
        if self.failures >= GIVE_UP_AFTER {
            return RecoveryStep::GiveUp;
        }
        let due = self.since + self.delay();
        if now < due {
            return RecoveryStep::Wait(due);
        }
        RecoveryStep::Attempt(match self.target {
            Target::Recreate { lost_was_software } => Attempt::Recreate {
                allow_software: lost_was_software || self.failures >= SOFTWARE_AFTER,
            },
            Target::Adopt => Attempt::Adopt,
        })
    }

    pub(crate) fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        self.since = now;
    }

    fn delay(&self) -> Duration {
        if self.target == Target::Adopt && self.failures == 0 {
            return Duration::ZERO;
        }
        FIRST_DELAY
            .saturating_mul(1 << self.failures.min(8))
            .min(MAX_DELAY)
    }
}

/// The shared context as a window that lost its device finds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shared {
    /// Another window recreated it.
    Healthy,
    /// Lost or already dropped; whether it was a CPU adapter, as far as known.
    Lost { was_software: bool },
}

/// What one `WgpuRenderer::recover` call does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Next {
    Wait(Instant),
    Attempt(Attempt),
    GiveUp(Target),
}

/// One window's recovery across `recover` calls: `None` while its device is fine.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Recovering(Option<RecoveryPolicy>);

impl Recovering {
    /// Picks the target for `shared`. A change of target starts a fresh
    /// policy, so failures to recreate do not count against adopting the
    /// context another window brought back. Returns true when this call
    /// starts recreating: the caller releases the lost context.
    pub(crate) fn begin(&mut self, now: Instant, shared: Shared) -> bool {
        let current = self.0.map(|policy| policy.target());
        match shared {
            Shared::Healthy => {
                if current != Some(Target::Adopt) {
                    self.0 = Some(RecoveryPolicy::new(now, Target::Adopt));
                }
                false
            }
            Shared::Lost { was_software } => {
                if matches!(current, Some(Target::Recreate { .. })) {
                    return false;
                }
                self.0 = Some(RecoveryPolicy::new(
                    now,
                    Target::Recreate {
                        lost_was_software: was_software,
                    },
                ));
                true
            }
        }
    }

    pub(crate) fn next(&self, now: Instant) -> Next {
        let Some(policy) = self.0 else {
            return Next::Attempt(Attempt::Adopt);
        };
        match policy.step(now) {
            RecoveryStep::Wait(at) => Next::Wait(at),
            RecoveryStep::Attempt(attempt) => Next::Attempt(attempt),
            RecoveryStep::GiveUp => Next::GiveUp(policy.target()),
        }
    }

    pub(crate) fn succeeded(&mut self) {
        self.0 = None;
    }

    pub(crate) fn failed(&mut self, now: Instant) -> Next {
        if let Some(policy) = self.0.as_mut() {
            policy.failed(now);
        }
        self.next(now)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Attempt, FIRST_DELAY, GIVE_UP_AFTER, MAX_DELAY, Next, Recovering, RecoveryPolicy,
        RecoveryStep, SOFTWARE_AFTER, Shared, Target,
    };
    use std::time::{Duration, Instant};

    fn hardware() -> Target {
        Target::Recreate {
            lost_was_software: false,
        }
    }

    fn attempt_times(policy: &mut RecoveryPolicy, start: Instant) -> Vec<(Duration, bool)> {
        let mut now = start;
        let mut out = Vec::new();
        loop {
            match policy.step(now) {
                RecoveryStep::Wait(at) => {
                    assert!(at > now);
                    now = at;
                }
                RecoveryStep::Attempt(attempt) => {
                    let allow_software = match attempt {
                        Attempt::Recreate { allow_software } => allow_software,
                        Attempt::Adopt => false,
                    };
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
        let policy = RecoveryPolicy::new(start, hardware());
        assert_eq!(policy.step(start), RecoveryStep::Wait(start + FIRST_DELAY));
        assert_eq!(
            policy.step(start + FIRST_DELAY),
            RecoveryStep::Attempt(Attempt::Recreate {
                allow_software: false
            })
        );
    }

    #[test]
    fn delays_grow_and_attempts_stop() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, hardware()), start);
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
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, hardware()), start);
        let software: Vec<bool> = attempts.iter().map(|(_, software)| *software).collect();
        let hardware_only = SOFTWARE_AFTER as usize;
        assert!(software[..hardware_only].iter().all(|s| !s));
        assert!(software[hardware_only..].iter().all(|s| *s));
    }

    #[test]
    fn a_lost_software_device_accepts_software_at_once() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, Target::Recreate { lost_was_software: true }), start);
        assert!(attempts.iter().all(|(_, software)| *software));
    }

    #[test]
    fn adopting_a_recreated_context_starts_at_once_and_backs_off() {
        let start = Instant::now();
        let mut policy = RecoveryPolicy::new(start, Target::Adopt);
        assert_eq!(policy.step(start), RecoveryStep::Attempt(Attempt::Adopt));
        policy.failed(start);
        assert!(matches!(policy.step(start), RecoveryStep::Wait(at) if at > start));
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, Target::Adopt), start);
        assert_eq!(attempts.len(), GIVE_UP_AFTER as usize);
        assert!(attempts.iter().all(|(_, software)| !software));
    }

    const HARDWARE_LOST: Shared = Shared::Lost {
        was_software: false,
    };

    #[test]
    fn a_failed_adoption_waits_instead_of_retrying_every_draw() {
        let start = Instant::now();
        let mut recovering = Recovering::default();
        assert!(!recovering.begin(start, Shared::Healthy));
        assert_eq!(recovering.next(start), Next::Attempt(Attempt::Adopt));
        let Next::Wait(due) = recovering.failed(start) else {
            panic!("a failed adoption must wait");
        };
        // A draw every 16 ms until the attempt is due neither attempts nor
        // gives up.
        let mut now = start;
        while now < due {
            recovering.begin(now, Shared::Healthy);
            assert_eq!(recovering.next(now), Next::Wait(due));
            now += Duration::from_millis(16);
        }
        assert_eq!(recovering.next(due), Next::Attempt(Attempt::Adopt));
    }

    #[test]
    fn giving_up_on_adoption_closes_the_window_not_the_app() {
        let start = Instant::now();
        let mut recovering = Recovering::default();
        recovering.begin(start, Shared::Healthy);
        let mut now = start;
        let last = loop {
            match recovering.next(now) {
                Next::Wait(at) => now = at,
                Next::Attempt(_) => {
                    recovering.failed(now);
                }
                give_up @ Next::GiveUp(_) => break give_up,
            }
        };
        assert_eq!(last, Next::GiveUp(Target::Adopt));
    }

    #[test]
    fn the_context_coming_back_resets_the_failures() {
        let start = Instant::now();
        let mut recovering = Recovering::default();
        assert!(recovering.begin(start, HARDWARE_LOST));
        let mut now = start;
        for _ in 0..GIVE_UP_AFTER - 1 {
            if let Next::Wait(at) = recovering.next(now) {
                now = at;
            }
            recovering.failed(now);
        }
        // Another window recreated the context: adopt it at once.
        assert!(!recovering.begin(now, Shared::Healthy));
        assert_eq!(recovering.next(now), Next::Attempt(Attempt::Adopt));
    }

    #[test]
    fn only_the_first_call_after_a_loss_releases_the_context() {
        let start = Instant::now();
        let mut recovering = Recovering::default();
        assert!(recovering.begin(start, HARDWARE_LOST));
        assert!(!recovering.begin(start, HARDWARE_LOST));
        assert!(!recovering.begin(start, Shared::Lost { was_software: true }));
    }

    #[test]
    fn a_recovered_window_starts_afresh_on_its_next_loss() {
        let start = Instant::now();
        let mut recovering = Recovering::default();
        recovering.begin(start, HARDWARE_LOST);
        recovering.failed(start + FIRST_DELAY);
        recovering.succeeded();
        let later = start + Duration::from_secs(60);
        assert!(recovering.begin(later, HARDWARE_LOST));
        assert_eq!(recovering.next(later), Next::Wait(later + FIRST_DELAY));
    }
}
