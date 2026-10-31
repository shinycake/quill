//! The content grid: the display frames animated content draws on.
//!
//! Animated content asks for frames on its own schedule: the frame clock
//! (`frame_clock`) ticks custom emoji, stickers, GIFs, spoiler specks,
//! typing dots and clock-driven motion at the highest rate asked, and each
//! playing video asks for the vsync its next picture is due on
//! (`request_video_frame`). On a display faster than 60 Hz those land on
//! different vsyncs, so the window draws their union: a 30 fps clip and
//! 30 fps emoji cost up to 60 full window frames a second.
//!
//! Telegram Desktop drives every animation from one shared tick
//! (`Ui::Animations::Manager`). Here, on a display refreshing a whole
//! number of times 60 a second (every other vsync at 120 Hz, every fourth
//! at 240 Hz), content shares the display's grid vsyncs while more than
//! one source animates on it (the clock and a clip, or two clips):
//!
//! - The clock ticks on grid vsyncs when its rate divides the grid's (60,
//!   30, 20, 15 fps...), so each tick still comes one content frame after
//!   the last.
//! - A clip whose pictures have lately come a whole number of grid ticks
//!   apart (30 or 60 fps, not 24, 25 or 50) draws each on the next grid
//!   vsync, at most one display frame later. Each picture is then still
//!   held as long as the one before.
//!
//! Everything else keeps its own vsyncs: a source animating alone, content
//! the grid would hold unevenly, content faster than the grid, and anything
//! drawn for input (scrolling, typing, hover). At 60 Hz every vsync is on
//! the grid, and other rates (90, 144 Hz) have none, so nothing changes
//! there.
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
/// A vsync this far (in display frames) off the grid's phase is still on
/// it: the display link's present times carry some noise.
const PHASE_SLACK_DIV: u32 = 4;
/// A picture due this little after a grid vsync still draws on it: a
/// clip's due times are display link vsyncs, as exact as the grid's. A
/// picture due any later is not ready on that vsync (FFmpeg's due times
/// are its pictures' own), so it draws on the next one.
const SNAP_SLACK: Duration = Duration::from_micros(250);

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
        if residual.abs() * i128::from(PHASE_SLACK_DIV) > period {
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

    /// Grid ticks between two frames of content at `fps`, when a whole
    /// number of them make one of its frames (30 fps: 2 at 120 Hz), so it
    /// ticks at its own rate (give or take a 59.94 Hz grid). `None` for
    /// other rates (24, 25, 40 fps), which the grid would tick faster or
    /// unevenly, and for content faster than the grid.
    pub(crate) fn every(&self, fps: u32) -> Option<u32> {
        let grid_fps = 1.0 / self.tick().as_secs_f64();
        let ratio = grid_fps / f64::from(fps.max(1));
        let every = ratio.round();
        (every >= 1.0 && (ratio - every).abs() <= 0.05).then_some(every as u32)
    }

    /// Whether `gap` is a whole number of grid ticks, give or take a
    /// quarter display frame.
    pub(crate) fn whole_ticks(&self, gap: Duration) -> bool {
        let (gap, tick) = (nanos(gap), nanos(self.tick()));
        let ticks = (gap + tick / 2) / tick;
        ticks >= 1 && (gap - ticks * tick).abs() * i128::from(PHASE_SLACK_DIV) <= nanos(self.period)
    }

    /// The grid vsync a picture due at `due` is drawn on: the first at or
    /// after it (or within [`SNAP_SLACK`] before it).
    pub(crate) fn snap(&self, due: Instant) -> Instant {
        let slack = nanos(SNAP_SLACK);
        let tick = nanos(self.tick());
        let offset = nanos_between(self.anchor, due) - slack;
        // The ceiling of `offset / tick`.
        let index = -(-offset).div_euclid(tick);
        shifted(self.anchor, index * tick)
    }

    /// The last grid vsync at or before `at`, give or take a quarter
    /// display frame (the grid may have moved that much since `at`).
    fn at_or_before(&self, at: Instant) -> Instant {
        let slack = nanos(self.period) / i128::from(PHASE_SLACK_DIV);
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

/// The grid vsync one frame of content at `fps` after `last`, a vsync it
/// drew on. `None` when the grid does not fit `fps` ([`ContentGrid::every`]).
pub(crate) fn content_due(grid: &ContentGrid, last: Instant, fps: u32) -> Option<Instant> {
    let every = grid.every(fps)?;
    Some(
        grid.at_or_before(last.checked_add(grid.tick() * every)?)
            .max(grid.first_after(last.checked_add(grid.period() / 2)?)),
    )
}

/// The frame clock's next tick of content at `fps`, as `(wake, vsync)`:
/// the grid vsync one content frame after `last` (the vsync the last tick
/// drew on), never one whose frame has started by `now`, and the time to
/// wake for it: three quarters of a display frame before that frame starts
/// drawing (`lead` before it is shown). That is as early as a tick can come
/// without being drawn on the frame before, so a timer up to three
/// quarters of a frame late (6 ms at 120 Hz) still makes it. A tick and a
/// picture due on the same vsync share one window frame. `None` when the
/// grid does not fit `fps`.
pub(crate) fn clock_tick(
    grid: &ContentGrid,
    lead: Duration,
    last: Instant,
    now: Instant,
    fps: u32,
) -> Option<(Instant, Instant)> {
    let due = content_due(grid, last, fps)?;
    // A frame starts drawing `lead` before it is shown: one starting
    // within a millisecond is too close to make.
    let earliest = grid.first_after(now.checked_add(lead + Duration::from_millis(1))?);
    let at = due.max(earliest);
    let wake = at
        .checked_sub(lead + grid.period() * 3 / 4)
        .unwrap_or(now)
        .max(now);
    Some((wake, at))
}

/// How a clip's pictures are spaced, learned from their due times.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Cadence {
    last: Option<Instant>,
    /// The latest gaps in a row that were a whole number of grid ticks.
    even: u8,
}

/// Two due times this close are the same picture's.
const SAME_PICTURE: Duration = Duration::from_millis(2);
/// Gaps in a row a clip's pictures must come a whole number of grid ticks
/// apart before it draws on the grid: one now and then (a 25 fps clip at
/// 120 Hz alternates 4 and 5 vsyncs, a 50 fps one 2 and 3) is not enough.
const EVEN_GAPS: u8 = 3;

impl Cadence {
    /// The next picture is due at `due` (asked again on every draw until
    /// it shows), on a display with `grid`.
    pub(crate) fn note(&mut self, due: Instant, grid: Option<&ContentGrid>) {
        match self.last {
            Some(last) if due > last + SAME_PICTURE => {
                let even = grid.is_some_and(|grid| grid.whole_ticks(due - last));
                self.even = if even { self.even.saturating_add(1) } else { 0 };
                self.last = Some(due);
            }
            // Back to the start (a loop, a seek): the spacing stays.
            Some(last) if due + SAME_PICTURE < last => self.last = Some(due),
            Some(_) => {}
            None => self.last = Some(due),
        }
    }

    /// Whether the clip's pictures have lately come a whole number of grid
    /// ticks apart, so on the grid each is still held as long as the one
    /// before.
    pub(crate) fn even(&self) -> bool {
        self.even >= EVEN_GAPS
    }

    /// The vsync to draw the picture due at `due` on: the next vsync of
    /// `grid` (the display's, while other content animates on it) when the
    /// clip's pictures come [`Self::even`]ly; else its own.
    pub(crate) fn on_grid(&self, due: Instant, grid: Option<&ContentGrid>) -> Instant {
        match grid {
            Some(grid) if self.even() => grid.snap(due),
            _ => due,
        }
    }
}

/// Something that animates on a display: the frame clock, or a clip (by
/// player id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Clock,
    Clip(u64),
}

/// A source still counts as animating this long after it said when it
/// draws next.
const PLAYING_SLACK: Duration = Duration::from_millis(100);

thread_local! {
    /// Each display's grid, so the frame clock and every video drawn on
    /// that display keep one phase.
    static GRIDS: RefCell<Vec<(u64, ContentGrid)>> = const { RefCell::new(Vec::new()) };
    /// What animates on each display, and until when.
    static PLAYING: RefCell<Vec<(u64, Source, Instant)>> = const { RefCell::new(Vec::new()) };
}

/// `source` animated on `display` at `now` and draws again by `next`.
pub(crate) fn note_playing(display: u64, source: Source, now: Instant, next: Instant) {
    let until = next.max(now).checked_add(PLAYING_SLACK).unwrap_or(now);
    PLAYING.with(|playing| {
        let mut playing = playing.borrow_mut();
        playing.retain(|(id, other, until)| *until >= now && (*id, *other) != (display, source));
        playing.push((display, source, until));
    });
}

/// Whether something besides `source` animates on `display` at `now`: only
/// then does `source` draw on the grid, sharing window frames with it.
pub(crate) fn shared(display: u64, source: Source, now: Instant) -> bool {
    PLAYING.with(|playing| {
        playing
            .borrow()
            .iter()
            .any(|(id, other, until)| *id == display && *other != source && *until >= now)
    })
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
