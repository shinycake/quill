//! Play heads: where one view of a clip is in its animation.
//!
//! Every view (a history row, a panel cell, an effect) has its own head, so
//! one row can play once while another loops the same sticker. A head only
//! moves while its view is drawn: a gap longer than [`PAUSE_GAP`] between
//! two draws (scrolled away, window in the background) resumes where it
//! stopped instead of jumping ahead, like tdesktop pausing off-screen
//! animations. It starts when its clip's first frame is shown, so the
//! first frames are never skipped while the clip opens.
#![deny(clippy::indexing_slicing)]

use std::time::{Duration, Instant};

/// Frames asked for ahead of the one shown (the ready queue).
pub const AHEAD: usize = 3;
/// How early, in grid frames, a paint may land and still show the frame
/// due on the next grid line, at most.
const GRID_SLACK: f64 = 0.25;
/// A view not drawn for longer than this paused meanwhile.
pub const PAUSE_GAP: Duration = Duration::from_millis(200);

/// What a view does at the end of its clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlayMode {
    /// Starts over.
    Loop,
    /// Plays once, then rests on a frame.
    Once(Rest),
    /// Plays this many times, then rests on the first frame (tdesktop
    /// `LimitedLoopsEmoji`, Premium emoji statuses).
    Loops(u32),
    /// Shows the frame this far in, without a head of its own; past the
    /// end there is nothing to show (a live story burst on its own clock).
    At(Duration),
}

/// Which frame a finished one-shot rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rest {
    /// The first frame: a lone animated emoji after its first play.
    First,
    /// The last frame: a dice showing its value, a sticker with looping off.
    Last,
}

/// A clip's frames: `frames` on a grid `step` times finer than the file's
/// own `file_fps`, so a 60 fps file on a 120 Hz screen has a frame between
/// each two of its own (ThorVG draws fractional frames).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timeline {
    pub frames: u32,
    pub file_fps: f64,
    pub step: u32,
}

impl Timeline {
    /// Frames a second on the grid.
    pub fn grid_fps(&self) -> f64 {
        self.file_fps * f64::from(self.step.max(1))
    }

    /// One play.
    pub fn duration(&self) -> Duration {
        let fps = self.grid_fps();
        if fps > 0.0 && fps.is_finite() {
            Duration::from_secs_f64(f64::from(self.frames) / fps)
        } else {
            Duration::ZERO
        }
    }

    /// The tick rate a playing view needs: the grid's rate at a high
    /// display rate, else the file's own. A `coarse` clip shows only its
    /// file's frames, so twice that rate keeps each picture up for the same
    /// number of ticks without waking for the grid frames it skips.
    pub fn tick_fps(&self, high_rate: bool, coarse: bool) -> f64 {
        match (high_rate, coarse) {
            (false, _) => self.file_fps,
            (true, false) => self.grid_fps(),
            (true, true) => self.grid_fps().min(self.file_fps * 2.0),
        }
    }
}

/// The frame a view shows now, and the next ones it will want.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pick {
    pub index: u32,
    /// The view still moves (ask for ticks); false once it rests.
    pub animating: bool,
    pub ahead: [Option<u32>; AHEAD],
}

/// One view's position in its clip.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayHead {
    pos: Duration,
    last: Option<Instant>,
    started: bool,
}

impl PlayHead {
    /// Whether its first frame was shown.
    pub fn started(&self) -> bool {
        self.started
    }

    /// How far it played (while its view was drawn).
    pub fn pos(&self) -> Duration {
        self.pos
    }

    /// The first frame is on screen: the clock starts now.
    pub fn start(&mut self, now: Instant) {
        if !self.started {
            self.started = true;
            self.pos = Duration::ZERO;
            self.last = Some(now);
        }
    }

    /// Play again from the first frame (a tap on a lone emoji).
    pub fn restart(&mut self) {
        *self = Self::default();
    }

    /// Move to `now` and pick the frame to show; `None` when nothing shows
    /// (a [`PlayMode::At`] past the end, or an empty clip).
    pub fn advance(
        &mut self,
        now: Instant,
        mode: PlayMode,
        timeline: Timeline,
        high_rate: bool,
        display_hz: f64,
    ) -> Option<Pick> {
        if self.started {
            if let Some(last) = self.last {
                let gap = now.saturating_duration_since(last);
                if gap <= PAUSE_GAP {
                    self.pos = self.pos.saturating_add(gap);
                }
            }
            self.last = Some(now);
        }
        let mut pick = pick(self.pos, mode, timeline, high_rate, display_hz)?;
        if !self.started {
            // Waiting for the first frame keeps the view ticking.
            pick.animating = !matches!(mode, PlayMode::At(_)) || pick.animating;
        }
        Some(pick)
    }
}

/// How early, in grid frames, a paint may land and still count as on the
/// next grid line: a quarter frame, but never half a refresh or more. Paints
/// land on refreshes, so a slack of a whole refresh would put a head that
/// started one refresh after its paints' beat exactly on a frame boundary,
/// and paint jitter would skip a frame and hold the next.
fn grid_slack(timeline: Timeline, display_hz: f64) -> f64 {
    let half_refresh = 0.5 * timeline.grid_fps() / display_hz;
    if half_refresh.is_finite() && half_refresh > 0.0 {
        GRID_SLACK.min(half_refresh)
    } else {
        GRID_SLACK
    }
}

/// The frame `pos` into a view playing `mode`, painted on a display
/// refreshing at `display_hz`.
pub fn pick(
    pos: Duration,
    mode: PlayMode,
    timeline: Timeline,
    high_rate: bool,
    display_hz: f64,
) -> Option<Pick> {
    let frames = timeline.frames;
    if frames == 0 {
        return None;
    }
    // At a low rate only every `step`-th grid frame is shown: the file's
    // own frames.
    let stride = if high_rate { 1 } else { timeline.step.max(1) };
    let pos = match mode {
        PlayMode::At(at) => at,
        _ => pos,
    };
    // A head starts on a paint, so later paints land near whole grid
    // frames, a little early or late. Counting a paint up to a quarter
    // frame early as on time keeps every frame up for the same number of
    // refreshes, instead of one, then three.
    let on_grid =
        (pos.as_secs_f64() * timeline.grid_fps() + grid_slack(timeline, display_hz)).floor();
    let shown = (on_grid / f64::from(stride) + 1e-9).floor();
    let along = if shown.is_finite() && shown >= 0.0 {
        (shown as u64).saturating_mul(u64::from(stride))
    } else {
        0
    };
    let total = u64::from(frames);
    let last = frames - 1;
    let wrap = |index: u64| (index % total) as u32;
    let aligned = |index: u32| index - index % stride;
    let ahead_from = |index: u32, looping: bool| {
        let mut ahead = [None; AHEAD];
        let mut next = u64::from(index);
        for slot in &mut ahead {
            next = next.saturating_add(u64::from(stride));
            *slot = if next < total {
                Some(next as u32)
            } else if looping {
                Some(aligned(wrap(next)))
            } else {
                None
            };
        }
        ahead
    };
    let resting = |index: u32| Pick {
        index,
        animating: false,
        ahead: [None; AHEAD],
    };
    let moving = |index: u32, looping: bool| Pick {
        index,
        animating: frames > 1,
        ahead: ahead_from(index, looping),
    };
    Some(match mode {
        PlayMode::Loop => moving(aligned(wrap(along)), true),
        PlayMode::Once(rest) if along >= total => resting(match rest {
            Rest::First => 0,
            Rest::Last => last,
        }),
        PlayMode::Once(rest) => {
            let mut pick = moving(along as u32, false);
            // Near the end, the frame it will rest on is drawn ahead too.
            if rest == Rest::First
                && let Some(slot) = pick.ahead.iter_mut().find(|slot| slot.is_none())
            {
                *slot = Some(0);
            }
            pick
        }
        PlayMode::Loops(count) if along >= total.saturating_mul(u64::from(count.max(1))) => {
            resting(0)
        }
        PlayMode::Loops(_) => moving(aligned(wrap(along)), true),
        PlayMode::At(_) if along >= total => return None,
        PlayMode::At(_) => moving(along as u32, false),
    })
}
