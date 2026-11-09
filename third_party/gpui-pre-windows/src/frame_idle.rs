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
//! ([`FrameIdle::demand`]) asks a parked source for one frame at once. That
//! frame parks it again unless GPUI asked for another frame during it, or it
//! came within [`BURST_WITHIN`] of the last woken frame that drew, so a caret
//! blink costs one frame and not [`FRAME_IDLE_AFTER`] of them. The macOS
//! display link follows the same rule (`link_pace.rs`).
//!
//! Fail-safe by construction: anything that counts as activity keeps the
//! source running, and a demand always gets a frame.
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

/// Two woken frames that drew closer than this are a burst, and the source
/// runs at the refresh rate for what follows. Under the 500 ms of a caret
/// blink, and over the 83 ms between frames of a 12 fps animation on a timer.
/// The macOS display link uses the same value.
pub(crate) const BURST_WITHIN: Duration = Duration::from_millis(100);

/// The refresh interval used when the display reports none (a zero interval
/// would make a frame source spin).
pub(crate) const FALLBACK_REFRESH: Duration = Duration::from_micros(16_667);

/// A frame request the source delivered, as [`FrameIdle::frame`] sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Frame {
    /// It drew or presented.
    pub(crate) drew: bool,
    /// GPUI asked for another frame during it, or the platform has another
    /// reason to keep frames coming.
    pub(crate) asked: bool,
}

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
    /// A demand asked the parked source for one frame, which has not run.
    woken: bool,
    last_woken_draw: Option<Instant>,
}

impl FrameIdle {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            last_activity: now,
            parked: false,
            woken: false,
            last_woken_draw: None,
        }
    }

    // Windows tracks the parked state in its vsync gate.
    #[allow(dead_code)]
    pub(crate) fn is_parked(&self) -> bool {
        self.parked
    }

    /// GPUI demanded a frame (the platform's frame waker ran: the window
    /// became dirty or queued a next-frame callback). Counts as activity.
    /// Returns `true` when the source was parked, or a frame an earlier
    /// demand asked for has not run yet: the caller must deliver one frame
    /// now, and pass it to [`FrameIdle::frame`], which decides whether the
    /// source keeps running.
    pub(crate) fn demand(&mut self, now: Instant) -> bool {
        self.last_activity = now;
        if std::mem::replace(&mut self.parked, false) {
            self.woken = true;
        }
        self.woken
    }

    /// Whether the next frame is one a demand asked a parked source for (or
    /// one the platform delivers while parked). GPUI's requests for another
    /// frame from before it must not count for it, so the platform clears
    /// them first.
    pub(crate) fn awaits_woken_frame(&self) -> bool {
        self.woken || self.parked
    }

    /// The source was asked to run at the refresh rate whatever its frames
    /// do (a draw the platform deferred, which only a periodic frame
    /// retries). Returns `true` when it was parked or waiting for a woken
    /// frame: the caller must resume refresh-rate pacing now.
    // Only Windows defers draws.
    #[allow(dead_code)]
    pub(crate) fn keep_running(&mut self, now: Instant) -> bool {
        self.last_activity = now;
        let resume = self.awaits_woken_frame();
        self.parked = false;
        self.woken = false;
        resume
    }

    /// A frame request from the source was handled.
    pub(crate) fn frame(&mut self, frame: Frame, now: Instant) -> Transition {
        if self.awaits_woken_frame() {
            return self.woken_frame(frame, now);
        }
        if frame.drew || frame.asked {
            self.last_activity = now;
            Transition::Keep
        } else if now.saturating_duration_since(self.last_activity) >= FRAME_IDLE_AFTER {
            self.parked = true;
            Transition::Park
        } else {
            Transition::Keep
        }
    }

    fn woken_frame(&mut self, frame: Frame, now: Instant) -> Transition {
        let was_parked = self.parked;
        self.woken = false;
        let burst = frame.drew
            && self
                .last_woken_draw
                .is_some_and(|at| now.saturating_duration_since(at) < BURST_WITHIN);
        if frame.drew {
            self.last_woken_draw = Some(now);
        }
        if frame.asked || burst {
            self.last_activity = now;
            self.parked = false;
            Transition::Resume
        } else {
            self.parked = true;
            if was_parked {
                Transition::Keep
            } else {
                Transition::Park
            }
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
    /// again): start from running, with a fresh grace period. A woken frame
    /// still to come keeps its rule.
    // Windows tracks its restarts through `demand`.
    #[allow(dead_code)]
    pub(crate) fn reset(&mut self, now: Instant) {
        if !self.woken {
            *self = Self {
                last_woken_draw: self.last_woken_draw,
                ..Self::new(now)
            };
        }
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

    const NOTHING: Frame = Frame {
        drew: false,
        asked: false,
    };
    const DRAW: Frame = Frame {
        drew: true,
        asked: false,
    };
    const ASKED: Frame = Frame {
        drew: true,
        asked: true,
    };

    /// A platform's frame source: the frame waker runs at each of `demands`,
    /// each frame draws, and the source delivers frames at `refresh` while
    /// it runs. Returns how many times it resumed and parked, and how many
    /// frames ran.
    fn woken_draws(
        demands: impl IntoIterator<Item = Instant>,
        refresh: Duration,
    ) -> (u32, u32, u32) {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(NOTHING, t0 + FRAME_IDLE_AFTER), Transition::Park);
        let (mut resumes, mut parks, mut frames) = (0, 0, 0);
        let mut frame = |idle: &mut FrameIdle, step, at| {
            frames += 1;
            match idle.frame(step, at) {
                Transition::Keep => {}
                Transition::Park => parks += 1,
                Transition::Resume => resumes += 1,
            }
        };
        let mut tick = t0 + FRAME_IDLE_AFTER;
        for at in demands {
            while !idle.is_parked() && tick + refresh < at {
                tick += refresh;
                frame(&mut idle, NOTHING, tick);
            }
            if idle.demand(at) || !idle.is_parked() {
                frame(&mut idle, DRAW, at);
            }
            tick = at;
        }
        (resumes, parks, frames)
    }

    fn every(period: Duration, count: u32) -> impl Iterator<Item = Instant> {
        let t0 = Instant::now() + Duration::from_secs(1);
        (1..=count).map(move |n| t0 + period * n)
    }

    #[test]
    fn a_caret_blink_costs_one_frame_per_blink() {
        let (resumes, parks, frames) = woken_draws(every(ms(500), 120), ms(16));
        assert_eq!((resumes, parks, frames), (0, 120, 120));
    }

    #[test]
    fn a_timed_animation_keeps_one_source_running() {
        let (resumes, parks, frames) = woken_draws(every(ms(83), 100), ms(16));
        assert_eq!((resumes, parks), (1, 1), "the first frame parks");
        assert!(frames > 400, "{frames}");
    }

    #[test]
    fn a_lone_demand_draws_once_and_parks() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(NOTHING, t0 + ms(250)), Transition::Park);
        assert!(idle.demand(t0 + ms(1000)));
        assert!(!idle.is_parked());
        assert!(idle.awaits_woken_frame());
        assert_eq!(idle.frame(DRAW, t0 + ms(1002)), Transition::Park);
        assert!(idle.is_parked());
        assert_eq!(
            next_frame_at(idle.is_parked(), t0, ms(16), t0 + ms(1002)),
            None
        );
    }

    #[test]
    fn two_woken_draws_close_together_resume_the_source() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        assert!(idle.demand(t0));
        assert_eq!(idle.frame(DRAW, t0), Transition::Park);
        assert!(idle.demand(t0 + ms(99)));
        assert_eq!(idle.frame(DRAW, t0 + ms(99)), Transition::Resume);
        assert!(!idle.is_parked());
    }

    #[test]
    fn two_woken_draws_further_apart_do_not() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        for at in [0, 100, 200] {
            assert!(idle.demand(t0 + ms(at)));
            assert_eq!(idle.frame(DRAW, t0 + ms(at)), Transition::Park);
        }
    }

    #[test]
    fn a_woken_frame_that_asks_for_another_resumes_at_once() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        assert!(idle.demand(t0));
        assert_eq!(idle.frame(ASKED, t0), Transition::Resume);
        // Then the grace period applies as before.
        assert_eq!(idle.frame(NOTHING, t0 + ms(249)), Transition::Keep);
        assert_eq!(idle.frame(NOTHING, t0 + ms(250)), Transition::Park);
    }

    /// The frame waker running again before the woken frame did (or after it
    /// was lost) asks for it again, so no demand is ever stranded.
    #[test]
    fn a_woken_frame_still_to_come_is_asked_for_again() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        assert!(idle.demand(t0));
        assert!(idle.demand(t0 + ms(1)));
        assert_eq!(idle.frame(DRAW, t0 + ms(2)), Transition::Park);
    }

    #[test]
    fn a_deferred_draw_keeps_the_source_running() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        assert!(idle.demand(t0));
        assert!(idle.keep_running(t0));
        assert!(!idle.awaits_woken_frame());
        assert_eq!(idle.frame(NOTHING, t0 + ms(16)), Transition::Keep);
        assert!(!idle.keep_running(t0 + ms(17)), "already running");
    }

    /// The platform restarts a source for a woken frame the way it starts one
    /// for a window being shown; the woken frame's rule still holds.
    #[test]
    fn a_reset_keeps_a_woken_frame_to_come() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        idle.park();
        assert!(idle.demand(t0));
        idle.reset(t0);
        assert_eq!(idle.frame(DRAW, t0 + ms(1)), Transition::Park);
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
            assert_eq!(idle.frame(NOTHING, t0 + ms(16 * i)), Transition::Keep);
        }
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(NOTHING, t0 + FRAME_IDLE_AFTER), Transition::Park);
        assert!(idle.is_parked());
        // A frame while parked (forced by the platform) keeps it parked
        // without transitions, and so does one that only draws.
        assert_eq!(idle.frame(NOTHING, t0 + ms(1250)), Transition::Keep);
        assert_eq!(idle.frame(DRAW, t0 + ms(1500)), Transition::Keep);
        assert!(idle.is_parked());
    }

    #[test]
    fn activity_restarts_the_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(DRAW, t0 + ms(200)), Transition::Keep);
        assert_eq!(idle.frame(NOTHING, t0 + ms(440)), Transition::Keep);
        assert_eq!(idle.frame(NOTHING, t0 + ms(450)), Transition::Park);
    }

    #[test]
    fn demand_on_a_running_source_asks_for_nothing_more() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert!(!idle.demand(t0 + ms(100)));
        assert!(!idle.awaits_woken_frame());
    }

    #[test]
    fn demand_while_running_delays_parking() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert!(!idle.demand(t0 + ms(240)));
        assert_eq!(idle.frame(NOTHING, t0 + ms(260)), Transition::Keep);
        assert_eq!(idle.frame(NOTHING, t0 + ms(490)), Transition::Park);
    }

    #[test]
    fn a_frame_while_parked_that_asks_for_another_resumes() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(NOTHING, t0 + ms(250)), Transition::Park);
        assert_eq!(idle.frame(ASKED, t0 + ms(1250)), Transition::Resume);
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(DRAW, t0 + ms(1266)), Transition::Keep);
    }

    #[test]
    fn reset_starts_running_with_a_fresh_grace_period() {
        let t0 = Instant::now();
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(NOTHING, t0 + ms(250)), Transition::Park);
        idle.reset(t0 + ms(5000));
        assert!(!idle.is_parked());
        assert_eq!(idle.frame(NOTHING, t0 + ms(5100)), Transition::Keep);
    }

    #[test]
    fn a_clock_that_goes_backwards_does_not_park() {
        let t0 = Instant::now() + Duration::from_secs(10);
        let mut idle = FrameIdle::new(t0);
        assert_eq!(idle.frame(NOTHING, t0 - ms(500)), Transition::Keep);
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
        assert_eq!(idle.frame(DRAW, t0 + ms(16)), Transition::Keep);
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
        assert_eq!(idle.frame(DRAW, t0), Transition::Keep);
        let mut wakeups = 0;
        let mut last_wakeup = t0;
        let mut next = next_frame_at(idle.is_parked(), t0, refresh, t0);
        while let Some(at) = next.filter(|at| *at < end) {
            wakeups += 1;
            last_wakeup = at;
            idle.frame(NOTHING, at);
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
    /// while running, one frame for a demand on a parked source, demands at
    /// random times, frames that draw and ask for more at random. Checks the
    /// invariants the platforms rely on.
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
        // Mirrors the platform: when the next frame is, none while parked.
        let mut next_frame = Some(t0);
        let mut last_activity = t0;
        let (mut parks, mut resumes, mut woken) = (0, 0, 0);
        for _ in 0..200_000 {
            let roll = next() % 100;
            // While parked nothing happens until a demand arrives.
            if roll < 3 || next_frame.is_none() {
                now += Duration::from_micros(next() % 2_000_000);
                last_activity = now;
                if idle.demand(now) {
                    woken += 1;
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
            let was_woken = idle.awaits_woken_frame();
            let frame = Frame {
                drew: roll < 3 + (next() % 30),
                asked: next() % 10 == 0,
            };
            if frame.drew || frame.asked {
                last_activity = now;
            }
            match idle.frame(frame, now) {
                Transition::Keep => {}
                Transition::Park => {
                    assert!(
                        was_woken || now - last_activity >= FRAME_IDLE_AFTER,
                        "parked a running source inside its grace period"
                    );
                    assert!(!frame.asked, "parked a frame that asked for another");
                    parks += 1;
                }
                Transition::Resume => {
                    assert!(was_woken);
                    resumes += 1;
                }
            }
            if frame.asked {
                assert!(!idle.is_parked(), "a frame that asked for another parked");
            }
            next_frame = next_frame_at(idle.is_parked(), due, refresh, now);
            assert!(!idle.awaits_woken_frame() || idle.is_parked());
            // While running, the source never sits idle past the grace period
            // plus one refresh without parking.
            if !idle.is_parked() {
                assert!(now - last_activity < FRAME_IDLE_AFTER + refresh);
            }
        }
        assert!(
            parks > 100 && resumes > 100 && woken > 100,
            "parks {parks}, resumes {resumes}, woken {woken}"
        );
    }
}
