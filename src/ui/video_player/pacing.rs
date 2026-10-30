//! Frame pacing: when the frame being drawn will be on screen, and whether
//! to ask for the very next frame or sleep until the next picture is due.
//! Pure (no GPUI), so the decisions are unit-tested.
//!
//! On macOS the display link reports the refresh period and the vsync the
//! current frame targets (the present "hint"). On Windows and Wayland GPUI
//! reports the display's refresh period (`Window::refresh_interval`).
//! Without a period (X11) the element's draw starts stand in: the typical
//! spacing of the closest draws is the frame interval (a player that sleeps
//! between pictures draws at whole multiples of it, so only the shortest
//! gaps count). Without a hint the next vsync is predicted from the last
//! start.
#![deny(clippy::indexing_slicing)]

use super::super::frame_grid::{Cadence, ContentGrid};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Draw starts kept for the interval estimate.
const HISTORY: usize = 8;
/// The fastest and slowest display the estimate accepts (240 Hz, 20 Hz).
const MIN_INTERVAL: Duration = Duration::from_micros(4_167);
const MAX_INTERVAL: Duration = Duration::from_millis(50);
/// Two draw starts closer than this are the same frame (a tile drawn by
/// its layer and by its slice). Well under half a 240 Hz frame, so a late
/// frame followed by an early one still counts as two; GPUI exposes no
/// frame counter to tell draws apart by identity.
const SAME_FRAME: Duration = Duration::from_millis(2);
/// Until two draws were seen: a 60 Hz display.
const DEFAULT_INTERVAL: Duration = Duration::from_micros(16_667);
/// A wake this close to now is not worth a timer: ask for the next frame.
const MIN_SLEEP: Duration = Duration::from_millis(1);

/// Predicts the present time of the frame being drawn.
#[derive(Clone, Debug, Default)]
pub(crate) struct PresentEstimator {
    starts: VecDeque<Instant>,
    /// The platform's present time for the frame being drawn, if any.
    hint: Option<Instant>,
    /// The present time predicted for the last draw.
    present: Option<Instant>,
    /// The platform's refresh period, if it reports one.
    period: Option<Duration>,
    /// The vsyncs content draws on, on a display faster than 60 Hz
    /// (`frame_grid`).
    grid: Option<ContentGrid>,
    /// Something else animates on that display: the clip may share its
    /// grid vsyncs.
    shared: bool,
    /// How the clip's pictures are spaced.
    cadence: Cadence,
    /// The picture asked for last draws on the grid.
    on_grid: bool,
}

impl PresentEstimator {
    /// A draw started at `at`.
    pub(crate) fn record_draw(&mut self, at: Instant) {
        if self.starts.back().is_some_and(|last| *last >= at) {
            return;
        }
        // Drawn twice in one frame (an inline tile: its row in the
        // conversation slice, then its copy in the animation layer): one
        // start per frame, or the spacing would read as a 240 Hz display.
        // Under half a 240 Hz interval: a late frame followed by an early
        // one still counts as two.
        if self
            .starts
            .back()
            .is_some_and(|last| at.saturating_duration_since(*last) < SAME_FRAME)
        {
            return;
        }
        if self.starts.len() == HISTORY {
            self.starts.pop_front();
        }
        self.starts.push_back(at);
    }

    /// The draw that started last will be on screen at `at` (what
    /// [`Self::predict`] said for it).
    pub(crate) fn note_present(&mut self, at: Instant) {
        self.present = Some(at);
    }

    /// How long before its present time a frame starts drawing: the last
    /// draw's predicted present time minus its start (about one interval
    /// without a platform hint; on macOS the display link's output time
    /// runs further ahead).
    pub(crate) fn lead(&self) -> Duration {
        match (self.present, self.starts.back()) {
            (Some(present), Some(start)) => present
                .saturating_duration_since(*start)
                .clamp(self.interval() / 2, self.interval() * 3),
            _ => self.interval(),
        }
    }

    /// The platform's refresh period (macOS display link, else the
    /// window's display as GPUI reports it), or `None`.
    pub(crate) fn set_period(&mut self, period: Option<Duration>) {
        self.period = period.filter(|period| *period >= MIN_INTERVAL && *period <= MAX_INTERVAL);
    }

    /// The platform's present time for the current frame (macOS display
    /// link output time), or `None`.
    pub(crate) fn set_hint(&mut self, hint: Option<Instant>) {
        self.hint = hint;
    }

    /// The content grid of the window's display, or `None` (60 Hz, or no
    /// vsync times), and whether something else animates there.
    pub(crate) fn set_grid(&mut self, grid: Option<ContentGrid>, shared: bool) {
        self.grid = grid;
        self.shared = shared;
    }

    /// When to show the next picture, due at `due`: while something else
    /// animates on the display, on the next grid vsync, so it shares a
    /// window frame with that, if the clip's pictures come a whole number
    /// of grid vsyncs apart (`Cadence::on_grid`); else when it is due.
    pub(crate) fn on_grid(&mut self, due: Instant) -> Instant {
        self.cadence.note(due, self.grid.as_ref());
        let grid = self.grid.as_ref().filter(|_| self.shared);
        self.on_grid = grid.is_some() && self.cadence.even();
        self.cadence.on_grid(due, grid)
    }

    /// How long before a picture is shown to have asked for its frame
    /// (`request_policy`): the lead, and on the grid a quarter frame more,
    /// so that a late timer still makes the grid vsync, as the frame
    /// clock's does (`frame_grid::clock_tick`). Each wakes three quarters
    /// of a frame before its frame starts, never before the frame ahead of
    /// it started: the lead is measured from the draw's start, after the
    /// display link's call.
    pub(crate) fn request_lead(&self) -> Duration {
        if self.on_grid {
            self.lead() + self.interval() / 4
        } else {
            self.lead()
        }
    }

    /// The display's frame interval: the platform's refresh period, else
    /// the median of the recent draw spacings within half again of the
    /// shortest (longer ones are skipped frames or a player sleeping between
    /// pictures), clamped to 20..240 Hz.
    pub(crate) fn interval(&self) -> Duration {
        if let Some(period) = self.period {
            return period;
        }
        let mut gaps: Vec<Duration> = self
            .starts
            .iter()
            .zip(self.starts.iter().skip(1))
            .map(|(a, b)| b.saturating_duration_since(*a))
            .filter(|gap| *gap <= MAX_INTERVAL * 2)
            .collect();
        if gaps.is_empty() {
            return DEFAULT_INTERVAL;
        }
        gaps.sort_unstable();
        let smallest = gaps.first().copied().unwrap_or(DEFAULT_INTERVAL);
        let close: Vec<Duration> = gaps
            .into_iter()
            .filter(|gap| *gap <= smallest + smallest / 2)
            .collect();
        close
            .get(close.len() / 2)
            .copied()
            .unwrap_or(smallest)
            .clamp(MIN_INTERVAL, MAX_INTERVAL)
    }

    /// When the frame drawn at `now` will be on screen: the platform hint
    /// when it is fresh (moved forward a whole number of intervals when it
    /// is just behind), else the vsync after the last draw start, stepped
    /// past `now`. A hint more than two intervals old is stale (the link
    /// stopped) and ignored.
    pub(crate) fn predict(&self, now: Instant) -> Instant {
        let interval = self.interval();
        // Intervals are at most 50 ms, so these sums can't overflow.
        if let Some(hint) = self.hint
            && now.saturating_duration_since(hint) <= interval * 2
        {
            return step_past(hint, now, interval);
        }
        match self.starts.back() {
            Some(last) if now.saturating_duration_since(*last) <= interval * 4 => {
                step_past(last.checked_add(interval).unwrap_or(*last), now, interval)
            }
            _ => now.checked_add(interval).unwrap_or(now),
        }
    }
}

/// `at`, or `at` plus whole intervals, whichever is first at or after `now`.
fn step_past(at: Instant, now: Instant, interval: Duration) -> Instant {
    if at >= now || interval.is_zero() {
        return at;
    }
    let behind = now.saturating_duration_since(at).as_secs_f64();
    let steps = (behind / interval.as_secs_f64()).ceil();
    Duration::try_from_secs_f64(interval.as_secs_f64() * steps)
        .ok()
        .and_then(|ahead| at.checked_add(ahead))
        .unwrap_or(now)
}

/// How the next frame is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameRequest {
    /// Draw again on the next display frame.
    NextFrame,
    /// Sleep until this instant, then ask for the next display frame.
    SleepUntil(Instant),
}

/// The request policy. A frame that starts drawing at `t` is on screen at
/// `t + lead`, so the picture due at `next_due` needs the frame that
/// starts at `next_due - lead`. Sleep until half an interval before that
/// frame's start (so a late timer still makes it, and an early one does
/// not catch the frame before), then ask for the next frame; when that is
/// already now, or the due time is unknown, ask for the next frame at once.
pub(crate) fn request_policy(
    now: Instant,
    next_due: Option<Instant>,
    interval: Duration,
    lead: Duration,
) -> FrameRequest {
    let Some(due) = next_due else {
        return FrameRequest::NextFrame;
    };
    match due.checked_sub(lead.saturating_add(interval / 2)) {
        Some(wake) if wake.saturating_duration_since(now) > MIN_SLEEP => {
            FrameRequest::SleepUntil(wake)
        }
        _ => FrameRequest::NextFrame,
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameRequest, PresentEstimator, request_policy};
    use std::time::{Duration, Instant};

    fn hz(rate: f64) -> Duration {
        Duration::from_secs_f64(1.0 / rate)
    }

    fn close(a: Duration, b: Duration) -> bool {
        a.abs_diff(b) < Duration::from_micros(150)
    }

    fn steady(rate: f64, count: u32, jitter_us: &[i64]) -> (PresentEstimator, Instant) {
        let t0 = Instant::now();
        let mut estimator = PresentEstimator::default();
        let mut last = t0;
        for i in 0..count {
            let jitter = jitter_us
                .get(i as usize % jitter_us.len().max(1))
                .copied()
                .unwrap_or(0);
            let base = t0 + hz(rate) * i;
            last = if jitter >= 0 {
                base + Duration::from_micros(jitter as u64)
            } else {
                base.checked_sub(Duration::from_micros((-jitter) as u64))
                    .unwrap()
            };
            estimator.record_draw(last);
        }
        (estimator, last)
    }

    #[test]
    fn the_interval_follows_60_120_and_144_hz_with_jitter() {
        for rate in [60.0, 120.0, 144.0] {
            let (estimator, last) = steady(rate, 12, &[0, 300, -250, 120, -80]);
            assert!(
                close(estimator.interval(), hz(rate)),
                "{rate}: {:?}",
                estimator.interval()
            );
            // No hint: the vsync after the last draw.
            let predicted = estimator.predict(last + Duration::from_micros(500));
            assert!(close(predicted.duration_since(last), hz(rate)), "{rate}");
        }
    }

    #[test]
    fn a_missed_frame_still_predicts_the_next_vsync() {
        let t0 = Instant::now();
        let mut estimator = PresentEstimator::default();
        let period = hz(60.0);
        for i in [0u32, 1, 2, 4, 5, 6, 8, 9] {
            estimator.record_draw(t0 + period * i);
        }
        assert!(close(estimator.interval(), period));
        // Drawing late, half way through the frame after the last draw.
        let now = t0 + period * 9 + period * 3 / 2;
        let predicted = estimator.predict(now);
        assert!(close(predicted.duration_since(t0), period * 11));
        // Only one-frame gaps missing: the doubled gaps are divided back.
        let mut sparse = PresentEstimator::default();
        for i in [0u32, 2, 4, 6, 7, 9, 11] {
            sparse.record_draw(t0 + period * i);
        }
        assert!(close(sparse.interval(), period), "{:?}", sparse.interval());
    }

    #[test]
    fn the_platform_period_wins_over_a_player_that_sleeps_between_pictures() {
        // 60 fps content on a 120 Hz display draws every other vsync once
        // it sleeps (and slow frames while opening): the spacing says
        // 60 Hz or worse, the display link says 120.
        let t0 = Instant::now();
        let mut estimator = PresentEstimator::default();
        for ms in [0u64, 48, 146, 200, 241, 337, 374] {
            estimator.record_draw(t0 + Duration::from_millis(ms));
        }
        assert!(estimator.interval() > hz(30.0));
        estimator.set_period(Some(hz(120.0)));
        assert!(close(estimator.interval(), hz(120.0)));
        // A nonsense period is ignored.
        estimator.set_period(Some(Duration::from_secs(1)));
        assert!(estimator.interval() > hz(30.0));
    }

    #[test]
    fn a_reported_period_paces_the_first_frames_instead_of_60hz() {
        // A clip starting on a 144 Hz display: before two draws were seen
        // the estimate says 60 Hz; the display's reported period does not.
        let now = Instant::now();
        let mut estimator = PresentEstimator::default();
        estimator.set_period(Some(hz(144.0)));
        assert!(close(estimator.predict(now).duration_since(now), hz(144.0)));
        assert!(close(estimator.lead(), hz(144.0)));
        // A 60 fps clip: its next picture is over two 144 Hz frames away,
        // so the player sleeps between pictures. With the 60 Hz guess it
        // asks for the very next frame instead.
        let due = now + hz(60.0);
        assert!(matches!(
            request_policy(now, Some(due), estimator.interval(), estimator.lead()),
            FrameRequest::SleepUntil(_)
        ));
        let sixty = PresentEstimator::default();
        assert_eq!(
            request_policy(now, Some(due), sixty.interval(), sixty.lead()),
            FrameRequest::NextFrame
        );
        // A 360 Hz display is past what the estimate accepts: ignored.
        estimator.set_period(Some(hz(360.0)));
        assert!(close(estimator.interval(), hz(60.0)));
    }

    #[test]
    fn without_a_period_only_the_closest_draws_set_the_interval() {
        let t0 = Instant::now();
        let mut estimator = PresentEstimator::default();
        let tick = hz(120.0);
        // Mostly two- and three-vsync gaps with a couple of single ones.
        for i in [0u32, 2, 4, 5, 7, 10, 11, 13] {
            estimator.record_draw(t0 + tick * i);
        }
        assert!(close(estimator.interval(), tick));
    }

    #[test]
    fn a_fresh_hint_wins_and_a_stale_one_is_ignored() {
        let (mut estimator, last) = steady(120.0, 8, &[0]);
        let hint = last + Duration::from_millis(12);
        estimator.set_hint(Some(hint));
        assert_eq!(estimator.predict(last + Duration::from_millis(1)), hint);
        // Just behind the hint: one interval on.
        let late = hint + Duration::from_millis(2);
        assert!(close(
            estimator.predict(late).duration_since(hint),
            hz(120.0)
        ));
        // A hint from long ago (the link stopped): the draw estimate.
        estimator.set_hint(last.checked_sub(Duration::from_millis(100)));
        let now = last + Duration::from_millis(1);
        assert!(close(
            estimator.predict(now).duration_since(last),
            hz(120.0)
        ));
    }

    #[test]
    fn with_no_history_the_next_frame_is_one_60hz_frame_away() {
        let estimator = PresentEstimator::default();
        let now = Instant::now();
        assert!(close(estimator.predict(now).duration_since(now), hz(60.0)));
    }

    #[test]
    fn sleep_until_due_only_when_the_picture_is_more_than_a_frame_and_a_half_away() {
        let now = Instant::now();
        for (display, content, sleeps) in [
            (120.0, 24.0, true),
            (120.0, 25.0, true),
            (120.0, 30.0, true),
            (120.0, 60.0, true),
            (60.0, 24.0, true),
            (60.0, 25.0, true),
            (60.0, 30.0, true),
            (60.0, 60.0, false),
        ] {
            let interval = hz(display);
            // Just after a picture: the next is one content frame away.
            let due = now + hz(content);
            match request_policy(now, Some(due), interval, interval) {
                FrameRequest::NextFrame => assert!(!sleeps, "{display} Hz / {content} fps"),
                FrameRequest::SleepUntil(wake) => {
                    assert!(sleeps, "{display} Hz / {content} fps");
                    // Wakes in time for the frame before the due one.
                    assert!(wake + interval < due, "{display} Hz / {content} fps");
                    assert!(wake > now);
                }
            }
        }
        // Unknown or already due: the next frame.
        let interval = hz(120.0);
        assert_eq!(
            request_policy(now, None, interval, interval),
            FrameRequest::NextFrame
        );
        assert_eq!(
            request_policy(now, Some(now), interval, interval),
            FrameRequest::NextFrame
        );
    }

    #[test]
    fn the_wake_lands_on_the_frame_that_starts_one_lead_before_due() {
        // macOS: the display link's output time runs ~1.5 frames ahead of
        // the callback that starts the draw.
        let now = Instant::now();
        let interval = hz(120.0);
        let lead = interval * 3 / 2;
        let due = now + hz(24.0);
        let FrameRequest::SleepUntil(wake) = request_policy(now, Some(due), interval, lead) else {
            panic!("a 24 fps picture is several frames away");
        };
        // The draw that shows it starts at `due - lead`: the wake is half a
        // frame before that, after the frame before it.
        let start = due.checked_sub(lead).unwrap();
        assert!(close(start.duration_since(wake), interval / 2));
        // Due on the frame after next with that lead: no time to sleep.
        let soon = now + interval * 2;
        assert_eq!(
            request_policy(now, Some(soon), interval, lead),
            FrameRequest::NextFrame
        );
    }

    #[test]
    fn the_lead_is_the_last_predicted_present_minus_its_draw_start() {
        let (mut estimator, last) = steady(120.0, 8, &[0]);
        assert!(close(estimator.lead(), hz(120.0)));
        estimator.note_present(last + Duration::from_millis(12));
        assert!(close(estimator.lead(), Duration::from_millis(12)));
        // Never more than three frames (a stale prediction).
        estimator.note_present(last + Duration::from_millis(500));
        assert!(close(estimator.lead(), hz(120.0) * 3));
    }

    #[test]
    fn two_draws_in_one_frame_count_once() {
        // Two tiles of one clip draw in the same frame (a layer copy and a
        // slice paint): the second start must not halve the interval.
        let t0 = Instant::now();
        let mut estimator = PresentEstimator::default();
        let period = hz(120.0);
        for i in 0u32..12 {
            let at = t0 + period * i;
            estimator.record_draw(at);
            estimator.record_draw(at + Duration::from_micros(400));
        }
        assert!(
            close(estimator.interval(), period),
            "{:?}",
            estimator.interval()
        );
    }

    #[test]
    fn jittered_240_hz_frames_are_not_merged() {
        // Gaps down to 2.97 ms (a late frame, then an early one) are still
        // separate frames at 240 Hz.
        let (estimator, _) = steady(240.0, 16, &[0, 600, -600, 400, -400]);
        assert!(
            close(estimator.interval(), hz(240.0)),
            "{:?}",
            estimator.interval()
        );
    }
}
