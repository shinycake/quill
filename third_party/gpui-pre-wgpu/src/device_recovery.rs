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
//! A GPU that keeps dying is not recovered onto forever ([`SharedRecovery`]):
//! from the [`SOFTWARE_AFTER_LOSSES`]th loss within [`LOSS_WINDOW`] recovery
//! makes the new context on a CPU adapter only ([`Adapters::SoftwareOnly`])
//! and stops looking for the GPU. It gives up when no CPU adapter works, or
//! when that CPU context is lost too.
//!
//! A real GPU can take a while to return after a resume or driver reset,
//! so attempts stay hardware-only for about 27 s. When recovery still had
//! to fall back to a CPU adapter the windows keep looking for the GPU on a
//! slow timer ([`HardwareProbe`], held in [`SharedRecovery`] so that any
//! window looks, not only the one that recreated the context and may have
//! closed since) and move back onto it when it returns.
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
/// Failed attempts on hardware only before software adapters are accepted
/// (the last at about 27 s).
const SOFTWARE_AFTER: u32 = 7;
/// Failed attempts in all before giving up (the last at about 43 s).
const GIVE_UP_AFTER: u32 = 9;
/// On a CPU adapter recovery fell back to: the first look for the GPU, and
/// the longest gap between looks.
const PROBE_FIRST: Duration = Duration::from_secs(10);
const PROBE_MAX: Duration = Duration::from_secs(120);
/// Losses of the shared context counted together, and how many of them make
/// recovery stay on a CPU adapter or give up.
const LOSS_WINDOW: Duration = Duration::from_secs(600);
const SOFTWARE_AFTER_LOSSES: usize = 3;
/// While a look runs on its thread: how often a window checks for its answer.
const PROBE_POLL: Duration = Duration::from_millis(250);

/// Which adapters a new context may be made on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Adapters {
    /// Not a CPU adapter: a lost GPU may come back.
    HardwareOnly,
    /// The best that works, which is a GPU when there is one.
    Any,
    /// A CPU adapter only: the GPU keeps dying.
    SoftwareOnly,
}

impl Adapters {
    /// Whether an adapter, a CPU one or not, may be tried.
    pub(crate) fn admits(self, cpu: bool) -> bool {
        match self {
            Self::HardwareOnly => !cpu,
            Self::Any => true,
            Self::SoftwareOnly => cpu,
        }
    }
}

/// What a recovery is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    /// Make a new shared context: no window has one. `adapters`: what the
    /// first attempts may use; hardware only widens to any after
    /// [`SOFTWARE_AFTER`] failures.
    Recreate { adapters: Adapters },
    /// Use the shared context another window recreated.
    Adopt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Attempt {
    Recreate { adapters: Adapters },
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
            Target::Recreate {
                adapters: Adapters::HardwareOnly,
            } if self.failures >= SOFTWARE_AFTER => Attempt::Recreate {
                adapters: Adapters::Any,
            },
            Target::Recreate { adapters } => Attempt::Recreate { adapters },
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
    /// Lost or already dropped; what a new context may be made on (see
    /// [`Losses::adapters`]).
    Lost { adapters: Adapters },
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
            Shared::Lost { adapters } => {
                if matches!(current, Some(Target::Recreate { .. })) {
                    return false;
                }
                self.0 = Some(RecoveryPolicy::new(now, Target::Recreate { adapters }));
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

    pub(crate) fn target(&self) -> Option<Target> {
        self.0.map(|policy| policy.target())
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

/// A context a recreation made becomes the shared one (`install`) only once
/// this window's renderer is built on it. Installed before, a failed build
/// left a context that looked recovered: the window's next call switched to
/// adopting it, with its failures reset, and the other windows adopted it
/// too.
pub(crate) fn install_once_built<C, E>(
    context: C,
    build: impl FnOnce(&C) -> Result<(), E>,
    install: impl FnOnce(C),
) -> Result<(), E> {
    build(&context)?;
    install(context);
    Ok(())
}

/// When a window on a CPU adapter that recovery fell back to looks for the
/// GPU again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HardwareProbe {
    looks: u32,
    due: Instant,
    /// A look is running on its own thread: when to check for its answer.
    looking: Option<Instant>,
}

/// What a window does when its probe timer fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeStep {
    /// Not yet: the next look is at this instant.
    Wait(Instant),
    /// Start a look off the UI thread.
    Look,
    /// A look is running: check for its answer at this instant.
    Await(Instant),
}

impl HardwareProbe {
    /// After a recovery that reached `target` and landed on a CPU adapter
    /// or not. Only a recreation from a lost GPU probes: a lost CPU device
    /// has no GPU to return to, and adopting leaves it to the window that
    /// recreated the context.
    pub(crate) fn after_recovery(
        now: Instant,
        target: Option<Target>,
        landed_on_software: bool,
    ) -> Option<Self> {
        let from_gpu = target
            == Some(Target::Recreate {
                adapters: Adapters::HardwareOnly,
            });
        (from_gpu && landed_on_software).then_some(Self {
            looks: 0,
            due: now + PROBE_FIRST,
            looking: None,
        })
    }

    pub(crate) fn due(&self) -> Instant {
        self.due
    }

    /// When a window's probe timer is due: the next check for a running
    /// look's answer, else the next look. The same instant on every draw
    /// until a step moves it, so a draw does not arm another timer.
    pub(crate) fn next_at(&self) -> Instant {
        self.looking.unwrap_or(self.due)
    }

    pub(crate) fn step(&self, now: Instant) -> ProbeStep {
        match self.looking {
            Some(check) => ProbeStep::Await(check),
            None if now < self.due => ProbeStep::Wait(self.due),
            None => ProbeStep::Look,
        }
    }

    /// A look started on its own thread.
    pub(crate) fn started(&mut self, now: Instant) {
        self.looking = Some(now + PROBE_POLL);
    }

    /// The running look had no answer yet: check again later.
    pub(crate) fn polled(&mut self, now: Instant) {
        self.looking = Some(now + PROBE_POLL);
    }

    /// The GPU was not back: look again later, less often each time.
    pub(crate) fn missed(&mut self, now: Instant) {
        self.looking = None;
        self.looks = self.looks.saturating_add(1);
        self.due = now
            + PROBE_FIRST
                .saturating_mul(1 << self.looks.min(8))
                .min(PROBE_MAX);
    }
}

/// What the windows sharing a GPU context know about its recovery.
#[derive(Debug, Default)]
pub(crate) struct SharedRecovery {
    probe: Option<HardwareProbe>,
    /// When the shared context was lost, within [`LOSS_WINDOW`] of the last.
    losses: Vec<Instant>,
    /// The context is the CPU one a GPU that kept dying forced.
    forced_software: bool,
    /// That forced CPU context was lost too.
    gave_up: bool,
}

/// How a loss of the shared context counts against the GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Losses {
    Few,
    /// Recover onto a CPU adapter only, and do not look for the GPU again.
    KeepsDying,
    GiveUp,
}

impl Losses {
    /// What a recreation after this loss may use, given whether the lost
    /// context was on a CPU adapter, as far as known.
    pub(crate) fn adapters(self, was_software: Option<bool>) -> Adapters {
        match (self, was_software) {
            (Self::KeepsDying | Self::GiveUp, _) => Adapters::SoftwareOnly,
            // No GPU to wait for.
            (Self::Few, Some(true)) => Adapters::Any,
            (Self::Few, _) => Adapters::HardwareOnly,
        }
    }
}

impl SharedRecovery {
    /// The context was lost: whatever was looked for on it is moot. Called
    /// once per context, by the window that releases it.
    pub(crate) fn lost(&mut self, now: Instant) -> Losses {
        self.probe = None;
        self.gave_up |= self.forced_software;
        self.losses
            .retain(|at| now.saturating_duration_since(*at) < LOSS_WINDOW);
        self.losses.push(now);
        self.losses_now()
    }

    /// The count as of the last loss.
    pub(crate) fn losses_now(&self) -> Losses {
        if self.gave_up {
            Losses::GiveUp
        } else if self.losses.len() >= SOFTWARE_AFTER_LOSSES {
            Losses::KeepsDying
        } else {
            Losses::Few
        }
    }

    /// A window's recovery that reached `target` succeeded. Only a
    /// recreation decides whether to look for the GPU; a window adopting the
    /// context afterwards keeps the look the recreation set.
    pub(crate) fn recovered(
        &mut self,
        now: Instant,
        target: Option<Target>,
        landed_on_software: bool,
    ) {
        if let Some(Target::Recreate { adapters }) = target {
            self.probe = HardwareProbe::after_recovery(now, target, landed_on_software);
            self.forced_software = adapters == Adapters::SoftwareOnly;
        }
    }

    pub(crate) fn probe(&self) -> Option<HardwareProbe> {
        self.probe
    }

    pub(crate) fn set_probe(&mut self, probe: Option<HardwareProbe>) {
        self.probe = probe;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        install_once_built, Adapters, Attempt, FIRST_DELAY, GIVE_UP_AFTER, HardwareProbe, LOSS_WINDOW, Losses,
        MAX_DELAY, Next, PROBE_FIRST, PROBE_MAX, PROBE_POLL, ProbeStep, Recovering,
        RecoveryPolicy, RecoveryStep, SOFTWARE_AFTER, SOFTWARE_AFTER_LOSSES, Shared,
        SharedRecovery, Target,
    };
    use std::time::{Duration, Instant};

    fn hardware() -> Target {
        Target::Recreate {
            adapters: Adapters::HardwareOnly,
        }
    }

    fn attempt_times(policy: &mut RecoveryPolicy, start: Instant) -> Vec<(Duration, Adapters)> {
        let mut now = start;
        let mut out = Vec::new();
        loop {
            match policy.step(now) {
                RecoveryStep::Wait(at) => {
                    assert!(at > now);
                    now = at;
                }
                RecoveryStep::Attempt(attempt) => {
                    let adapters = match attempt {
                        Attempt::Recreate { adapters } => adapters,
                        Attempt::Adopt => Adapters::HardwareOnly,
                    };
                    out.push((now - start, adapters));
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
                adapters: Adapters::HardwareOnly
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
        let adapters: Vec<Adapters> = attempts.iter().map(|(_, adapters)| *adapters).collect();
        let hardware_only = SOFTWARE_AFTER as usize;
        assert!(
            adapters[..hardware_only]
                .iter()
                .all(|a| *a == Adapters::HardwareOnly)
        );
        assert!(adapters[hardware_only..].iter().all(|a| *a == Adapters::Any));
    }

    #[test]
    fn a_lost_software_device_accepts_software_at_once() {
        let start = Instant::now();
        let attempts = attempt_times(
            &mut RecoveryPolicy::new(
                start,
                Target::Recreate {
                    adapters: Losses::Few.adapters(Some(true)),
                },
            ),
            start,
        );
        assert!(attempts.iter().all(|(_, a)| *a == Adapters::Any));
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
        assert!(attempts.iter().all(|(_, a)| *a == Adapters::HardwareOnly));
    }

    const HARDWARE_LOST: Shared = Shared::Lost {
        adapters: Adapters::HardwareOnly,
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
        let software = Shared::Lost {
            adapters: Adapters::Any,
        };
        assert!(!recovering.begin(start, software));
    }

    /// The renderer could not be built on a new context: the slot stays
    /// empty, so the window's next call keeps recreating, with its failures
    /// counted, instead of adopting a context it cannot draw with.
    #[test]
    fn a_context_the_renderer_failed_on_is_not_installed() {
        let start = Instant::now();
        let mut slot: Option<&str> = None;
        let mut recovering = Recovering::default();
        assert!(recovering.begin(start, HARDWARE_LOST));
        let Next::Wait(due) = recovering.next(start) else {
            panic!("the first attempt waits for the driver");
        };
        let built = install_once_built(
            "new context",
            |_| Err("surface size above the device's limit"),
            |context| slot = Some(context),
        );
        assert_eq!(built, Err("surface size above the device's limit"));
        assert_eq!(slot, None);
        let Next::Wait(next) = recovering.failed(due) else {
            panic!("a failed attempt waits");
        };

        // The next call finds the slot as the window left it.
        let shared = match slot {
            Some(_) => Shared::Healthy,
            None => HARDWARE_LOST,
        };
        assert!(!recovering.begin(next, shared));
        assert_eq!(recovering.target(), Some(hardware()));
        assert!(next - due > FIRST_DELAY, "the failure was counted");

        let built = install_once_built("new context", |_| Ok::<(), &str>(()), |context| {
            slot = Some(context)
        });
        assert_eq!(built, Ok(()));
        assert_eq!(slot, Some("new context"));
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

    #[test]
    fn attempts_follow_the_backoff_curve() {
        let start = Instant::now();
        let attempts = attempt_times(&mut RecoveryPolicy::new(start, hardware()), start);
        let millis: Vec<(u128, bool)> = attempts
            .iter()
            .map(|(at, adapters)| (at.as_millis(), *adapters == Adapters::Any))
            .collect();
        assert_eq!(
            millis,
            [
                (350, false),
                (1_050, false),
                (2_450, false),
                (5_250, false),
                (10_850, false),
                (18_850, false),
                (26_850, false),
                (34_850, true),
                (42_850, true),
            ]
        );
    }

    #[test]
    fn a_software_fallback_from_a_lost_gpu_looks_for_it_less_and_less_often() {
        let start = Instant::now();
        let mut probe = HardwareProbe::after_recovery(start, Some(hardware()), true)
            .expect("falling back to a CPU adapter looks for the GPU");
        assert_eq!(probe.due(), start + PROBE_FIRST);
        let mut gaps = Vec::new();
        let mut now = probe.due();
        for _ in 0..6 {
            probe.missed(now);
            gaps.push((probe.due() - now).as_secs());
            now = probe.due();
        }
        assert_eq!(gaps, [20, 40, 80, 120, 120, 120]);
        assert!(probe.due() - now <= PROBE_MAX);
    }

    #[test]
    fn only_a_fallback_from_a_lost_gpu_looks_for_it() {
        let now = Instant::now();
        assert_eq!(
            HardwareProbe::after_recovery(now, Some(hardware()), false),
            None
        );
        let lost_software = Target::Recreate {
            adapters: Adapters::Any,
        };
        assert_eq!(
            HardwareProbe::after_recovery(now, Some(lost_software), true),
            None
        );
        assert_eq!(
            HardwareProbe::after_recovery(now, Some(Target::Adopt), true),
            None
        );
    }

    /// The window that recreated the context on a CPU adapter closed: the
    /// look for the GPU is the context's, so the windows left still look.
    #[test]
    fn any_window_on_the_fallback_looks_for_the_gpu() {
        let start = Instant::now();
        let mut shared = SharedRecovery::default();
        shared.recovered(start, Some(hardware()), true);
        // A second window adopts the fallback context.
        shared.recovered(start, Some(Target::Adopt), true);
        assert_eq!(
            shared.probe().map(|probe| probe.due()),
            Some(start + PROBE_FIRST)
        );
        shared.lost(start);
        assert_eq!(shared.probe(), None);
    }

    /// A look makes a Vulkan instance and lists the adapters, tens of
    /// milliseconds: it runs on its own thread, and the windows only check
    /// back for its answer until it comes.
    #[test]
    fn a_look_for_the_gpu_runs_off_the_ui_thread() {
        let start = Instant::now();
        let mut probe = HardwareProbe::after_recovery(start, Some(hardware()), true)
            .expect("falling back to a CPU adapter looks for the GPU");
        assert_eq!(probe.step(start), ProbeStep::Wait(start + PROBE_FIRST));
        let due = probe.due();
        assert_eq!(probe.step(due), ProbeStep::Look);
        probe.started(due);
        // Another window's timer, or the same one, while the look runs.
        assert_eq!(probe.step(due), ProbeStep::Await(due + PROBE_POLL));
        let checked = due + PROBE_POLL;
        probe.polled(checked);
        assert_eq!(probe.step(checked), ProbeStep::Await(checked + PROBE_POLL));
        let answered = checked + Duration::from_millis(80);
        probe.missed(answered);
        assert_eq!(probe.step(answered), ProbeStep::Wait(probe.due()));
        assert!(probe.due() > answered);
    }

    /// While a look runs, every draw (one each 16 ms) asks for the timer
    /// the poll armed, not for the past instant the look started at.
    #[test]
    fn draws_during_a_look_ask_for_the_armed_check() {
        let start = Instant::now();
        let mut probe = HardwareProbe::after_recovery(start, Some(hardware()), true)
            .expect("falling back to a CPU adapter looks for the GPU");
        let due = probe.next_at();
        probe.started(due);
        let ProbeStep::Await(check) = probe.step(due) else {
            panic!("a running look awaits its answer");
        };
        let mut now = due;
        while now < check {
            assert_eq!(probe.next_at(), check);
            now += Duration::from_millis(16);
        }
        probe.polled(check);
        assert_eq!(probe.next_at(), check + PROBE_POLL);
        probe.missed(check + PROBE_POLL);
        assert_eq!(probe.next_at(), probe.due());
    }

    /// A GPU that dies again after every recovery (a failing card, a driver
    /// that resets in a loop): from the third loss recovery makes the new
    /// context on a CPU adapter only, stays there, and gives up only when
    /// that CPU context is lost too.
    #[test]
    fn a_gpu_that_keeps_dying_is_left_for_a_cpu_adapter() {
        let start = Instant::now();
        let mut shared = SharedRecovery::default();
        let minute = Duration::from_secs(60);
        let mut counted = Vec::new();
        for n in 0..SOFTWARE_AFTER_LOSSES {
            counted.push(shared.lost(start + minute * n as u32));
        }
        let mut expected = vec![Losses::Few; SOFTWARE_AFTER_LOSSES - 1];
        expected.push(Losses::KeepsDying);
        assert_eq!(counted, expected);

        // Every attempt of this recovery is on a CPU adapter, the first too.
        let keeps_dying = Shared::Lost {
            adapters: Losses::KeepsDying.adapters(Some(false)),
        };
        let mut recovering = Recovering::default();
        let lost_at = start + minute * 2;
        assert!(recovering.begin(lost_at, keeps_dying));
        let mut now = lost_at;
        let mut attempts = Vec::new();
        loop {
            match recovering.next(now) {
                Next::Wait(at) => now = at,
                Next::Attempt(attempt) => {
                    attempts.push(attempt);
                    recovering.failed(now);
                }
                Next::GiveUp(_) => break,
            }
        }
        let software_only = Attempt::Recreate {
            adapters: Adapters::SoftwareOnly,
        };
        assert_eq!(attempts, vec![software_only; GIVE_UP_AFTER as usize]);

        // The forced CPU context does not look for the GPU, and a loss of it
        // gives up.
        let forced = Target::Recreate {
            adapters: Adapters::SoftwareOnly,
        };
        shared.recovered(lost_at, Some(forced), true);
        assert_eq!(shared.probe(), None);
        assert_eq!(shared.losses_now(), Losses::KeepsDying);
        assert_eq!(shared.lost(lost_at + minute), Losses::GiveUp);
        assert_eq!(shared.losses_now(), Losses::GiveUp);
    }

    /// The adapters come sorted best first, a GPU before a CPU one; a
    /// recreation takes the first its attempt admits.
    #[test]
    fn a_keeps_dying_attempt_takes_the_cpu_adapter_listed_after_the_gpu() {
        let sorted = [("discrete GPU", false), ("lavapipe", true)];
        let first = |adapters: Adapters| {
            sorted
                .iter()
                .find(|(_, cpu)| adapters.admits(*cpu))
                .map(|(name, _)| *name)
        };
        assert_eq!(
            first(Losses::KeepsDying.adapters(Some(false))),
            Some("lavapipe")
        );
        assert_eq!(first(Losses::Few.adapters(Some(true))), Some("discrete GPU"));
        assert_eq!(first(Losses::Few.adapters(None)), Some("discrete GPU"));
        assert_eq!(first(Adapters::HardwareOnly), Some("discrete GPU"));
    }

    #[test]
    fn losses_far_apart_are_each_recovered_from_afresh() {
        let start = Instant::now();
        let mut shared = SharedRecovery::default();
        for n in 0..10 {
            let at = start + LOSS_WINDOW * n;
            assert_eq!(shared.lost(at), Losses::Few);
        }
    }
}
