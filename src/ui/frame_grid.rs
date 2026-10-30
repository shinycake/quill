//! The content grid: the display frames animated content draws on.
//!
//! Animated content asks for frames on its own schedule: the frame clock
//! (`frame_clock`) ticks custom emoji, stickers, GIFs, spoiler specks and
//! typing dots at the highest rate asked, and each playing video asks for
//! the vsync its next picture is due on (`request_video_frame`). On a
//! display faster than 60 Hz those land on different vsyncs, so the window
//! draws their union: a 30 fps clip and 30 fps emoji cost up to 60 full
//! window frames a second, faster content more.
//!
//! Telegram Desktop drives every animation from one shared tick
//! (`Ui::Animations::Manager`), so content at 60 fps or less never costs
//! more than about 60 repaints a second together. Here, on a display
//! refreshing a whole number of times 60 a second (every other vsync at
//! 120 Hz, every fourth at 240 Hz), content draws only on every `step`-th
//! vsync of the display: the frame clock ticks on those vsyncs, and a video
//! picture due on another one is drawn on the next grid vsync, at most one
//! display frame later. Each source still shows its own frame for the time
//! it is drawn at, so playback speed is exact. Content faster than the
//! grid (stickers on the 120 Hz fine grid, a 120 fps clip) keeps its own
//! vsyncs, and user-driven motion (scrolling, springs, the caret) never
//! goes through the grid. At 60 Hz every vsync is on the grid, and other
//! rates (90, 144 Hz) have none: nothing changes there.
//!
//! The grid needs the display's vsync times: the display link's present
//! time on macOS. Without them (Linux, Windows) nothing changes.
//! docs/decisions/anim-frame-coalesce.md.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::time::{Duration, Instant};

/// When one of a display's frames is shown, and the time between two.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vsync {
    pub(crate) at: Instant,
    pub(crate) period: Duration,
}

impl Vsync {
    /// Whether `at` is recent enough at `now` to place the display's
    /// vsyncs: the display link reports one per frame while it runs, and
    /// stops reporting while the window idles.
    pub(crate) fn fresh(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) <= self.period * 2
    }
}

/// Every `step`-th vsync of a display, counted from `anchor` (one of
/// them): the vsyncs content animations draw on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContentGrid {
    anchor: Instant,
    period: Duration,
    step: u32,
}

/// The fastest the grid runs, in frames a second.
const GRID_HZ: f64 = 60.0;
/// A due time this close past a grid vsync still counts as on it (the
/// display link's present time and a clip's due times carry some noise).
const SNAP_SLACK_DIV: u32 = 4;

impl ContentGrid {
    /// The grid of the display `vsync` belongs to; `None` at 60 Hz or
    /// below, where every vsync is on it.
    pub(crate) fn new(vsync: Vsync) -> Option<Self> {
        let step = grid_step(vsync.period);
        (step >= 2).then_some(Self {
            anchor: vsync.at,
            period: vsync.period,
            step,
        })
    }

    /// The same grid, anchored on its vsync at or before `vsync` (so drift
    /// between the reported period and the display's own never builds
    /// up), or a new grid when the display changed its rate or the vsync
    /// is off this grid's phase.
    pub(crate) fn follow(self, vsync: Vsync) -> Option<Self> {
        let period = nanos(vsync.period);
        if period == 0 || nanos(self.period.abs_diff(vsync.period)) * 100 > period {
            return Self::new(vsync);
        }
        let offset = nanos_between(self.anchor, vsync.at);
        let index = (2 * offset + period).div_euclid(2 * period);
        let residual = offset - index * period;
        if residual.abs() * i128::from(SNAP_SLACK_DIV) > period {
            return Self::new(vsync);
        }
        let back = index.rem_euclid(i128::from(self.step)) * period;
        Some(Self {
            anchor: shifted(vsync.at, -back),
            period: vsync.period,
            step: self.step,
        })
    }

    /// The display's refresh period.
    pub(crate) fn period(&self) -> Duration {
        self.period
    }

    /// The time between two grid vsyncs.
    pub(crate) fn tick(&self) -> Duration {
        self.period * self.step
    }

    /// Grid ticks between two frames of content at `fps`: as many as fit in
    /// one of its frames, so it is never drawn slower than `fps` (give or
    /// take a 59.94 Hz grid). `None` for content faster than the grid.
    pub(crate) fn every(&self, fps: u32) -> Option<u32> {
        let grid_fps = 1.0 / self.tick().as_secs_f64();
        let every = (grid_fps / f64::from(fps.max(1)) + 0.05).floor();
        (every >= 1.0).then_some(every as u32)
    }

    /// The grid vsync a picture due at `due` is drawn on: the first at or
    /// after it, give or take a quarter vsync of timing noise.
    pub(crate) fn snap(&self, due: Instant) -> Instant {
        let slack = nanos(self.period) / i128::from(SNAP_SLACK_DIV);
        let tick = nanos(self.tick());
        let offset = nanos_between(self.anchor, due) - slack;
        // The ceiling of `offset / tick`.
        let index = -(-offset).div_euclid(tick);
        shifted(self.anchor, index * tick)
    }

    /// The last grid vsync at or before `at` (give or take the same slack).
    fn at_or_before(&self, at: Instant) -> Instant {
        let slack = nanos(self.period) / i128::from(SNAP_SLACK_DIV);
        let tick = nanos(self.tick());
        let index = (nanos_between(self.anchor, at) + slack).div_euclid(tick);
        shifted(self.anchor, index * tick)
    }

    /// The first grid vsync strictly after `at`.
    fn first_after(&self, at: Instant) -> Instant {
        let tick = nanos(self.tick());
        let index = nanos_between(self.anchor, at).div_euclid(tick) + 1;
        shifted(self.anchor, index * tick)
    }
}

/// Vsyncs per grid tick on a display refreshing every `period`: 2 at
/// 120 Hz, 4 at 240 Hz. 1 (no grid) at 60 Hz, where every vsync is on it,
/// and at rates no whole number of vsyncs makes 60 a second of (90, 144,
/// 165 Hz), where a grid of its vsyncs would draw 30 and 60 fps content
/// more often than before.
fn grid_step(period: Duration) -> u32 {
    let hz = 1.0 / period.as_secs_f64();
    let step = (hz / GRID_HZ).round();
    // A couple of hertz of headroom: 119.88 Hz is a 120 Hz display.
    if hz.is_finite() && (2.0..=16.0).contains(&step) && (hz - step * GRID_HZ).abs() <= 2.0 {
        step as u32
    } else {
        1
    }
}

/// The frame clock's next tick of content at `fps`, as `(wake, vsync)`:
/// the grid vsync one content frame after `last` (the vsync the last tick
/// drew on), never one whose frame has started by `now`, and the time to
/// wake for it: half a vsync before that frame starts drawing (`lead`
/// before it is shown), like a video's wake (`pacing::request_policy`), so
/// a tick and a picture due on the same vsync share one window frame.
/// `None` for content faster than the grid.
pub(crate) fn clock_tick(
    grid: &ContentGrid,
    lead: Duration,
    last: Instant,
    now: Instant,
    fps: u32,
) -> Option<(Instant, Instant)> {
    let every = grid.every(fps)?;
    let due = grid
        .at_or_before(last.checked_add(grid.tick() * every)?)
        .max(grid.first_after(last.checked_add(grid.period() / 2)?));
    // A frame starts drawing `lead` before it is shown: one starting
    // within a millisecond is too close to make.
    let earliest = grid.first_after(now.checked_add(lead + Duration::from_millis(1))?);
    let at = due.max(earliest);
    let wake = at
        .checked_sub(lead + grid.period() / 2)
        .unwrap_or(now)
        .max(now);
    Some((wake, at))
}

/// How a clip's pictures are spaced, learned from their due times: a clip
/// faster than the grid keeps its own vsyncs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Cadence {
    last: Option<Instant>,
    gap: Option<Duration>,
}

/// Two due times this close are the same picture's.
const SAME_PICTURE: Duration = Duration::from_millis(2);

impl Cadence {
    /// The next picture is due at `due` (asked again on every draw until
    /// it shows).
    pub(crate) fn note(&mut self, due: Instant) {
        match self.last {
            Some(last) if due > last + SAME_PICTURE => {
                self.gap = Some(due - last);
                self.last = Some(due);
            }
            // Back to the start (a loop, a seek): the spacing stays.
            Some(last) if due + SAME_PICTURE < last => self.last = Some(due),
            Some(_) => {}
            None => self.last = Some(due),
        }
    }

    /// The vsync to draw the picture due at `due` on: the next grid vsync,
    /// unless pictures come faster than the grid.
    pub(crate) fn on_grid(&self, due: Instant, grid: Option<&ContentGrid>) -> Instant {
        match grid {
            Some(grid)
                if self
                    .gap
                    .is_none_or(|gap| gap + grid.period() / 2 >= grid.tick()) =>
            {
                grid.snap(due)
            }
            _ => due,
        }
    }
}

thread_local! {
    /// Each display's grid, so the frame clock and every video drawn on
    /// that display keep one phase.
    static GRIDS: RefCell<Vec<(u64, ContentGrid)>> = const { RefCell::new(Vec::new()) };
}

/// The content grid of `display`, kept in phase with `vsync`, its latest
/// one; `None` at 60 Hz or below, or with no recent vsync (`now`).
pub(crate) fn display_grid(display: u64, vsync: Vsync, now: Instant) -> Option<ContentGrid> {
    if !vsync.fresh(now) {
        return None;
    }
    GRIDS.with(|grids| {
        let mut grids = grids.borrow_mut();
        let known = grids.iter().position(|(id, _)| *id == display);
        let grid = match known.and_then(|index| grids.get(index)) {
            Some((_, grid)) => grid.follow(vsync),
            None => ContentGrid::new(vsync),
        };
        match (known, grid) {
            (Some(index), Some(grid)) => {
                if let Some(entry) = grids.get_mut(index) {
                    entry.1 = grid;
                }
            }
            (Some(index), None) => {
                grids.swap_remove(index);
            }
            (None, Some(grid)) => grids.push((display, grid)),
            (None, None) => {}
        }
        grid
    })
}

fn nanos(duration: Duration) -> i128 {
    i128::try_from(duration.as_nanos()).unwrap_or(i128::MAX)
}

/// `at - from` in nanoseconds, negative when `at` is earlier.
fn nanos_between(from: Instant, at: Instant) -> i128 {
    if at >= from {
        nanos(at.duration_since(from))
    } else {
        -nanos(from.duration_since(at))
    }
}

/// `from` moved by `by` nanoseconds (negative: earlier).
fn shifted(from: Instant, by: i128) -> Instant {
    let span = Duration::from_nanos(u64::try_from(by.unsigned_abs()).unwrap_or(u64::MAX));
    let moved = if by >= 0 {
        from.checked_add(span)
    } else {
        from.checked_sub(span)
    };
    moved.unwrap_or(from)
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "frame_grid_tests.rs"]
mod tests;
