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
//! *parks* and delivers no frames at all ([`next_frame_at`] is `None`), like
//! the macOS display link, which stops. GPUI's frame waker
//! ([`FrameIdle::demand`]) unparks it at once.
//!
//! Fail-safe by construction: anything that counts as activity keeps the
//! source running, and a demand always unparks.
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

/// The refresh interval used when the display reports none (a zero interval
/// would make a frame source spin).
pub(crate) const FALLBACK_REFRESH: Duration = Duration::from_micros(16_667);

/// What the frame source must do after [`FrameIdle::frame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transition {
    /// Keep the current pacing (refresh rate, or none while parked).
    Keep,
    /// Stop delivering frames until the next demand.
    Park,
    /// Go back to the refresh rate.
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

    // Windows tracks the parked state in its vsync gate.
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

    /// The platform stopped the source for its own reason (the window was
    /// hidden or minimized, so no frame runs the decision). Parks it, so the
    /// first demand after the window comes back reports a resume.
    // Not every platform uses every method (X11 has no timer while hidden).
    #[allow(dead_code)]
    pub(crate) fn park(&mut self) {
        self.parked = true;
    }

    /// The source was (re)started for another reason (the window was shown
    /// again): start from running, with a fresh grace period.
    // Windows tracks its restarts through `demand`.
    #[allow(dead_code)]
    pub(crate) fn reset(&mut self, now: Instant) {
        *self = Self::new(now);
    }
}

/// When a periodic frame source delivers its next frame, after handling the
/// frame that was due at `due` at time `now`. Running: the first instant of
/// the `due + k * refresh` grid that is after `now`, so frames that were
/// missed are skipped rather than delivered in a burst. Parked: `None`, no
/// frame at all; only a frame demand restarts the source.
// Windows paces its frames by the vblank instead.
#[allow(dead_code)]
pub(crate) fn next_frame_at(
    parked: bool,
    due: Instant,
    refresh: Duration,
    now: Instant,
) -> Option<Instant> {
    if parked {
        return None;
    }
    let refresh = if refresh.is_zero() {
        FALLBACK_REFRESH
    } else {
        refresh
    };
    let late = now.saturating_duration_since(due);
    // Whole periods to skip, plus the next one; `refresh` is not zero here.
    let periods = (late.as_nanos() / refresh.as_nanos()).saturating_add(1);
    let offset = u64::try_from(periods.saturating_mul(refresh.as_nanos()))
        .ok()
        .map(Duration::from_nanos);
    offset
        .and_then(|offset| due.checked_add(offset))
        .or_else(|| now.checked_add(refresh))
        .or(Some(now))
}

/// Windows vsync thread: whether it has work this round, given whether each
/// window's frames are parked. It works while any window is not parked, or
/// when it was asked to wake (to stop, or to check its GPU device). Otherwise
/// it sleeps until a window unparks or it is asked to wake; there is no
/// timeout. With no windows at all it sleeps too.
// X11 runs one timer per window instead.
#[allow(dead_code)]
pub(crate) fn vsync_has_work(parked: impl IntoIterator<Item = bool>, wake_requested: bool) -> bool {
    wake_requested || parked.into_iter().any(|parked| !parked)
}

/// How long a touchpad contact handed to Windows Direct Manipulation keeps
/// the window's frames running at the refresh rate before a gesture is
/// recognized.
pub(crate) const CONTACT_KEEPS_FRAMES: Duration = Duration::from_secs(5);

/// After [`CONTACT_KEEPS_FRAMES`], how long the contact is still polled at
/// [`CONTACT_POLL_INTERVAL`], so fingers that rest before they move still
/// start a gesture.
pub(crate) const CONTACT_POLLED_FOR: Duration = Duration::from_secs(60);

/// How often a resting touchpad contact is polled.
// Only Windows has Direct Manipulation.
#[allow(dead_code)]
pub(crate) const CONTACT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// What a window with a touchpad contact needs (Windows Direct Manipulation,
/// whose viewport only advances when the window updates it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContactFrames {
    /// A gesture is running, or the contact is recent: frames at the refresh
    /// rate (each frame updates the viewport).
    Frames,
    /// The contact rests: update the viewport every
    /// [`CONTACT_POLL_INTERVAL`], and resume frames once a gesture starts.
    Poll,
    /// No contact, or it is old: nothing.
    Idle,
}

/// The decision for [`ContactFrames`]: `since_contact` is the time since the
/// last touchpad contact was handed to the viewport, `gesture_active` whether
/// the viewport is running or coasting.
// Only Windows has Direct Manipulation.
#[allow(dead_code)]
pub(crate) fn contact_frames(
    since_contact: Option<Duration>,
    gesture_active: bool,
) -> ContactFrames {
    if gesture_active {
        return ContactFrames::Frames;
    }
    match since_contact {
        Some(since) if since < CONTACT_KEEPS_FRAMES => ContactFrames::Frames,
        Some(since) if since < CONTACT_KEEPS_FRAMES.saturating_add(CONTACT_POLLED_FOR) => {
            ContactFrames::Poll
        }
        _ => ContactFrames::Idle,
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
        // A frame while parked (forced by the platform) keeps it parked
        // without transitions.
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
    fn an_active_frame_while_parked_resumes() {
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

    /// A window hidden while its frames were running (hide to tray right
    /// after a click) must report a resume on the first demand once shown,
    /// or a platform that only restarts its source on a resume would never
    /// deliver another frame.
    #[test]
    fn a_source_parked_by_the_platform_resumes_on_the_first_demand() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(true, t0 + ms(16)), Transition::Keep);
        idle.park();
        assert!(idle.is_parked());
        assert!(idle.demand(t0 + ms(30_000)));
        assert!(!idle.is_parked());
    }

    #[test]
    fn a_parked_source_schedules_no_frame() {
        let t0 = Instant::now();
        assert_eq!(next_frame_at(true, t0, ms(16), t0), None);
        assert_eq!(next_frame_at(true, t0, ms(16), t0 + ms(5000)), None);
    }

    #[test]
    fn a_running_source_ticks_on_the_refresh_grid() {
        let t0 = Instant::now();
        let refresh = ms(16);
        // On time: the next instant.
        assert_eq!(next_frame_at(false, t0, refresh, t0), Some(t0 + ms(16)));
        // Late by less than a period.
        assert_eq!(
            next_frame_at(false, t0, refresh, t0 + ms(5)),
            Some(t0 + ms(16))
        );
        // Missed frames are skipped, not delivered in a burst.
        assert_eq!(
            next_frame_at(false, t0, refresh, t0 + ms(50)),
            Some(t0 + ms(64))
        );
        // Exactly on a grid instant: the one after it, never `now` (which
        // would fire the timer again at once).
        assert_eq!(
            next_frame_at(false, t0, refresh, t0 + ms(32)),
            Some(t0 + ms(48))
        );
        // Fired early (a clock that went backwards): one period after `due`.
        let later = t0 + ms(100);
        assert_eq!(
            next_frame_at(false, later, refresh, t0),
            Some(later + ms(16))
        );
    }

    #[test]
    fn a_zero_refresh_interval_does_not_spin() {
        let t0 = Instant::now();
        assert_eq!(
            next_frame_at(false, t0, Duration::ZERO, t0),
            Some(t0 + FALLBACK_REFRESH)
        );
    }

    #[test]
    fn a_long_stall_lands_after_now() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_secs(3 * 24 * 3600);
        let next = next_frame_at(false, t0, Duration::from_nanos(1), now);
        assert!(next.is_some_and(|next| next > now));
    }

    /// The idle floor of one window: after its last active frame, how many
    /// times does a frame source following `next_frame_at` wake in 10 s?
    /// Only through the grace period; with the old one-second heartbeat it
    /// was ten more.
    #[test]
    fn an_idle_window_stops_waking_after_the_grace_period() {
        let t0 = Instant::now();
        let refresh = ms(16);
        let end = t0 + Duration::from_secs(10);
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(true, t0), Transition::Keep);
        let mut wakeups = 0;
        let mut last_wakeup = t0;
        let mut next = next_frame_at(idle.is_parked(), t0, refresh, t0);
        while let Some(at) = next.filter(|at| *at < end) {
            wakeups += 1;
            last_wakeup = at;
            idle.frame(false, at);
            next = next_frame_at(idle.is_parked(), at, refresh, at);
        }
        assert!(idle.is_parked());
        assert_eq!(wakeups, 16, "250 ms of 16 ms frames, then none");
        assert!(last_wakeup <= t0 + FRAME_IDLE_AFTER + refresh);
    }

    #[test]
    fn the_vsync_thread_sleeps_only_when_every_window_is_parked() {
        assert!(!vsync_has_work([], false));
        assert!(!vsync_has_work([true, true], false));
        assert!(vsync_has_work([true, false], false));
        assert!(vsync_has_work([false], false));
        // Asked to stop or to check its device: runs even with no windows.
        assert!(vsync_has_work([], true));
        assert!(vsync_has_work([true], true));
    }

    #[test]
    fn a_touchpad_contact_runs_frames_then_polls_then_stops() {
        assert_eq!(contact_frames(None, false), ContactFrames::Idle);
        assert_eq!(contact_frames(None, true), ContactFrames::Frames);
        assert_eq!(contact_frames(Some(ms(0)), false), ContactFrames::Frames);
        assert_eq!(
            contact_frames(Some(ms(4_999)), false),
            ContactFrames::Frames
        );
        assert_eq!(contact_frames(Some(ms(5_000)), false), ContactFrames::Poll);
        assert_eq!(contact_frames(Some(ms(64_999)), false), ContactFrames::Poll);
        assert_eq!(contact_frames(Some(ms(65_000)), false), ContactFrames::Idle);
        // A gesture keeps frames running however old the contact is.
        assert_eq!(
            contact_frames(Some(ms(90_000)), true),
            ContactFrames::Frames
        );
    }

    /// Drives the state the way a platform does: frames at a refresh rate
    /// while running and none while parked, demands at random times, frames
    /// active at random. Checks the invariants the platforms rely on.
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
        let mut next_frame = Some(t0);
        let mut last_active_or_demand = t0;
        let mut parks = 0;
        let mut resumes = 0;
        for _ in 0..200_000 {
            let roll = next() % 100;
            // While parked nothing happens until a demand arrives.
            if roll < 3 || next_frame.is_none() {
                now += Duration::from_micros(next() % 2_000_000);
                last_active_or_demand = now;
                if idle.demand(now) {
                    assert!(parked, "resume requested while running");
                    parked = false;
                    resumes += 1;
                    next_frame = Some(now);
                }
                assert!(!idle.is_parked());
                assert!(next_frame.is_some(), "a demand left the source stopped");
                continue;
            }
            let Some(due) = next_frame else {
                unreachable!("handled above");
            };
            now = now.max(due);
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
                Transition::Resume => unreachable!("a parked source gets no frames"),
            }
            assert_eq!(parked, idle.is_parked());
            next_frame = next_frame_at(parked, due, refresh, now);
            assert_eq!(next_frame.is_none(), parked);
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
