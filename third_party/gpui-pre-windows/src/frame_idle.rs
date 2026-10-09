// Added by the Quill project (2026) to gpui-pre-linux / gpui-pre-windows 0.3.7
// (Apache-2.0). See QUILL-CHANGES.md next to this crate's Cargo.toml. The file
// is identical in both crates; scripts/test-frame-idle.sh runs its tests.

//! When a window's periodic frame source may sleep.
//!
//! Upstream asks every visible window for a frame at the display's refresh
//! rate forever, so an idle window still wakes the main thread 60–144 times a
//! second. This is the decision the platforms share: after
//! [`FRAME_IDLE_AFTER`] of frames that neither drew, presented nor asked for
//! another frame, with no frame demand from GPUI in between, the source
//! *parks* and only delivers a [`PARKED_HEARTBEAT`]-spaced frame. GPUI's frame
//! waker ([`FrameIdle::demand`]) unparks it at once.
//!
//! Fail-safe by construction: anything that counts as activity keeps the
//! source running, a demand always unparks, and the heartbeat bounds how long
//! a demand that somehow bypassed the waker can go unserved.
//!
//! Kill switch: `QUILL_IDLE_FRAMES=0` (or `off`) in the environment, read
//! once, makes every frame count as active, so frame sources never park and
//! behave exactly as upstream.

use std::{
    ffi::OsStr,
    sync::OnceLock,
    time::{Duration, Instant},
};

/// The kill switch's environment variable.
pub(crate) const IDLE_FRAMES_ENV: &str = "QUILL_IDLE_FRAMES";

/// Whether frame sources may park: false when `QUILL_IDLE_FRAMES` is `0` or
/// `off` (any case). Read once; platforms call it while setting up their
/// first window, so later changes to the variable have no effect.
pub(crate) fn idle_frames_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| idle_frames_enabled_from(std::env::var_os(IDLE_FRAMES_ENV).as_deref()))
}

fn idle_frames_enabled_from(value: Option<&OsStr>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let value = value.to_string_lossy();
    let value = value.trim();
    !(value == "0" || value.eq_ignore_ascii_case("off"))
}

/// How long the frame source keeps running after the last frame that drew,
/// presented or asked for another frame (or the last frame demand). The
/// macOS display link uses the same value.
pub(crate) const FRAME_IDLE_AFTER: Duration = Duration::from_millis(250);

/// While parked, a window still gets one frame request this often. Normally it
/// finds nothing to do; it exists so a frame demand that never reached the
/// frame waker is served within this interval instead of never.
pub(crate) const PARKED_HEARTBEAT: Duration = Duration::from_secs(1);

/// What the frame source must do after [`FrameIdle::frame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transition {
    /// Keep the current pacing (refresh rate, or heartbeat while parked).
    Keep,
    /// Switch from the refresh rate to the heartbeat.
    Park,
    /// Switch from the heartbeat back to the refresh rate.
    Resume,
}

/// Per-window idle state. `Copy`, so platforms can keep it in a `Cell`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameIdle {
    last_activity: Instant,
    parked: bool,
}

impl FrameIdle {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_activity: now,
            parked: false,
        }
    }

    // Not every platform uses every method (Windows tracks the parked state
    // in its vsync gate and has no restart path).
    #[allow(dead_code)]
    pub(crate) fn is_parked(&self) -> bool {
        self.parked
    }

    /// GPUI demanded a frame (the platform's frame waker ran: the window
    /// became dirty or queued a next-frame callback). Counts as activity.
    /// Returns `true` when the source was parked: the caller must resume
    /// refresh-rate pacing now.
    pub(crate) fn demand(&mut self, now: Instant) -> bool {
        self.last_activity = now;
        std::mem::replace(&mut self.parked, false)
    }

    /// A frame request from the source was handled. `active`: it drew,
    /// presented, or asked for another frame (or the platform has another
    /// reason to keep frames coming).
    pub(crate) fn frame(&mut self, active: bool, now: Instant) -> Transition {
        if active {
            self.last_activity = now;
            if std::mem::replace(&mut self.parked, false) {
                Transition::Resume
            } else {
                Transition::Keep
            }
        } else if !self.parked
            && now.saturating_duration_since(self.last_activity) >= FRAME_IDLE_AFTER
        {
            self.parked = true;
            Transition::Park
        } else {
            Transition::Keep
        }
    }

    /// The source was (re)started for another reason (the window was shown
    /// again): start from running, with a fresh grace period.
    #[allow(dead_code)]
    pub(crate) fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn kill_switch_values() {
        let enabled = |value: Option<&str>| idle_frames_enabled_from(value.map(OsStr::new));
        assert!(enabled(None));
        assert!(enabled(Some("")));
        assert!(enabled(Some("1")));
        assert!(enabled(Some("on")));
        assert!(!enabled(Some("0")));
        assert!(!enabled(Some("off")));
        assert!(!enabled(Some(" OFF ")));
        assert!(!enabled(Some("Off")));
        // The documented name; the cached read itself depends on the
        // environment the tests run in.
        assert_eq!(IDLE_FRAMES_ENV, "QUILL_IDLE_FRAMES");
        let _ = idle_frames_enabled();
    }

    #[test]
    fn idle_frames_park_only_after_the_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        for i in 0..15 {
            assert_eq!(idle.frame(false, t0 + ms(16 * i)), Transition::Keep);
        }
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(false, t0 + FRAME_IDLE_AFTER), Transition::Park);
        assert!(idle.is_parked());
        // Further idle frames (heartbeats) keep it parked without transitions.
        assert_eq!(idle.frame(false, t0 + ms(1250)), Transition::Keep);
        assert!(idle.is_parked());
    }

    #[test]
    fn activity_restarts_the_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(true, t0 + ms(200)), Transition::Keep);
        assert_eq!(idle.frame(false, t0 + ms(440)), Transition::Keep);
        assert_eq!(idle.frame(false, t0 + ms(450)), Transition::Park);
    }

    #[test]
    fn demand_unparks_and_restarts_the_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(false, t0 + ms(300)), Transition::Park);
        assert!(idle.demand(t0 + ms(1000)));
        assert!(!idle.is_parked());
        // A second demand before the source ran is not a second resume.
        assert!(!idle.demand(t0 + ms(1001)));
        assert_eq!(idle.frame(false, t0 + ms(1200)), Transition::Keep);
        assert_eq!(idle.frame(false, t0 + ms(1251)), Transition::Park);
    }

    #[test]
    fn demand_while_running_delays_parking() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert!(!idle.demand(t0 + ms(240)));
        assert_eq!(idle.frame(false, t0 + ms(260)), Transition::Keep);
        assert_eq!(idle.frame(false, t0 + ms(490)), Transition::Park);
    }

    #[test]
    fn heartbeat_that_finds_work_resumes() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(false, t0 + ms(250)), Transition::Park);
        assert_eq!(idle.frame(true, t0 + ms(1250)), Transition::Resume);
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(true, t0 + ms(1266)), Transition::Keep);
    }

    #[test]
    fn reset_starts_running_with_a_fresh_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(false, t0 + ms(250)), Transition::Park);
        idle.reset(t0 + ms(5000));
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(false, t0 + ms(5100)), Transition::Keep);
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_park() {
        let t0 = Instant::now() + Duration::from_secs(10);
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(false, t0 - ms(500)), Transition::Keep);
        assert!(!idle.is_parked());
    }

    /// Drives the state the way a platform does: frames at a refresh rate
    /// while running and at the heartbeat while parked, demands at random
    /// times, frames active at random. Checks the invariants the platforms
    /// rely on.
    #[test]
    fn simulated_frame_source_never_strands_a_demand() {
        // Small xorshift so the test needs no dependencies and is repeatable.
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let refresh = ms(16);
        let t0 = Instant::now();
        let mut now = t0;
        let mut idle = FrameIdle::new(t0);
        // Mirrors the platform: running or parked, and when the next frame is.
        let mut parked = false;
        let mut next_frame = t0;
        let mut last_active_or_demand = t0;
        let mut parks = 0;
        let mut resumes = 0;
        for _ in 0..200_000 {
            let roll = next() % 100;
            if roll < 3 {
                // A demand arrives between frames.
                now += Duration::from_micros(next() % 2_000);
                last_active_or_demand = now;
                if idle.demand(now) {
                    assert!(parked, "resume requested while running");
                    parked = false;
                    resumes += 1;
                    next_frame = now;
                }
                assert!(!idle.is_parked());
                continue;
            }
            now = now.max(next_frame);
            let active = roll < 3 + (next() % 30);
            if active {
                last_active_or_demand = now;
            }
            match idle.frame(active, now) {
                Transition::Keep => {}
                Transition::Park => {
                    assert!(!parked);
                    assert!(now - last_active_or_demand >= FRAME_IDLE_AFTER);
                    parked = true;
                    parks += 1;
                }
                Transition::Resume => {
                    assert!(parked && active);
                    parked = false;
                    resumes += 1;
                }
            }
            assert_eq!(parked, idle.is_parked());
            next_frame = now + if parked { PARKED_HEARTBEAT } else { refresh };
            // While running, the source never sits idle past the grace period
            // plus one refresh without parking.
            if !parked {
                assert!(now - last_active_or_demand < FRAME_IDLE_AFTER + refresh);
            }
        }
        assert!(
            parks > 100 && resumes > 100,
            "parks {parks}, resumes {resumes}"
        );
    }
}
