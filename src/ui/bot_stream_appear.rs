//! The timing of a streamed reply's appearance, ported from Telegram
//! Desktop (`Message::textAppearValidate` / `textAppearCheckLine` and the
//! width and height animations in history_view_message.cpp).
//!
//! The text is revealed one laid-out line at a time. The current line's
//! revealed width sweeps to the line's width linearly: a line with lines
//! below it in [`LINE`] × its width ÷ [`MAX_WIDTH`], the last line (still
//! growing) in [`LINE`], and the first sweep of a new text in at least
//! [`FIRST_SWEEP`]. The bubble's height opens to the next line over
//! [`HEIGHT`] (ease-out cubic), starting once the sweep has [`HEIGHT`] or
//! less to go. Once the final message arrived, the rest goes faster
//! ([`LINE_FINAL`], [`HEIGHT_FINAL`]).
//!
//! Lines come from the last frame's layout (`text_reveal`), so a new
//! draft's lines are seen one frame after its text.
#![deny(clippy::indexing_slicing)]

use crate::ui::sidebar_reveal::ease_out_cubic;
use crate::ui::text_reveal::{RevealCut, RevealLine};
use std::time::{Duration, Instant};

/// `kFullLineAppearDuration`.
const LINE: Duration = Duration::from_millis(300);
/// `kFullLineAppearFinalDuration`.
const LINE_FINAL: Duration = Duration::from_millis(120);
/// `kLineHeightAppearDuration`.
const HEIGHT: Duration = Duration::from_millis(100);
/// `kLineHeightAppearFinalDuration`.
const HEIGHT_FINAL: Duration = Duration::from_millis(60);
/// `kMinWidthAppearDuration`.
const FIRST_SWEEP: Duration = Duration::from_millis(160);
/// The shortest sweep of a line with lines below it.
const SHORTEST_SWEEP: Duration = Duration::from_millis(10);
/// `st::msgMaxWidth`.
const MAX_WIDTH: f32 = 430.;

/// One running animation (`Ui::Animations::Simple`).
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
    ease: fn(f32) -> f32,
}

impl Tween {
    fn progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.;
        }
        let elapsed = now.saturating_duration_since(self.start).as_secs_f32();
        (elapsed / self.duration.as_secs_f32()).clamp(0., 1.)
    }

    fn value(&self, now: Instant) -> f32 {
        self.from + (self.to - self.from) * (self.ease)(self.progress(now))
    }

    fn done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.
    }
}

fn linear(t: f32) -> f32 {
    t
}

/// What a frame of the reveal shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame {
    pub(super) cut: RevealCut,
    /// How tall the text block is drawn (the bubble's open part).
    pub(super) height: f32,
    /// How wide it is drawn, while narrower than the text.
    pub(super) width: Option<f32>,
    /// Whether anything still moves: the clock has to keep ticking.
    pub(super) moving: bool,
}

/// Where the current text's reveal stands between texts and sweeps: one
/// value, advanced by `text_changed`, the next frame and the first sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sweeps {
    /// The text changed since the last frame: its lines show up in the next
    /// one, which has to be drawn. The first sweep (at least
    /// [`FIRST_SWEEP`], `startedForText`) has not started.
    Fresh,
    /// Drawn once; the first sweep has not started.
    First,
    /// The first sweep has started.
    Later,
}

/// Whether the text is a draft or the final message (the rest then goes
/// faster).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pace {
    Draft,
    Final,
}

/// The current line's width sweep.
#[derive(Clone, Copy, Debug)]
enum WidthSweep {
    /// No sweep runs. `target` is the width the current line was settled at
    /// (0 before a line starts).
    Settled { target: f32 },
    Running {
        start: f32,
        target: f32,
        duration: Duration,
        tween: Tween,
    },
}

impl WidthSweep {
    fn target(&self) -> f32 {
        match self {
            Self::Settled { target } | Self::Running { target, .. } => *target,
        }
    }

    fn running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// The bubble's height opening to the next line.
#[derive(Clone, Copy, Debug)]
enum HeightOpen {
    /// Nothing opens. `target` is the height last settled at.
    Settled {
        target: f32,
    },
    Running {
        target: f32,
        tween: Tween,
    },
}

impl HeightOpen {
    fn target(&self) -> f32 {
        match self {
            Self::Settled { target } | Self::Running { target, .. } => *target,
        }
    }

    fn running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

/// `TextAppearing`: where the reveal of one text is.
#[derive(Clone, Debug)]
pub(super) struct Appearing {
    lines: Vec<RevealLine>,
    shown_line: usize,
    revealed: f32,
    /// `shownWidth`: the revealed edge plus the footer's room, never
    /// narrowing, at most the text's width.
    shown_width: f32,
    /// The footer's room and the text's width, as of this frame.
    skip: f32,
    real: f32,
    width: WidthSweep,
    shown_height: f32,
    height: HeightOpen,
    sweeps: Sweeps,
    pace: Pace,
}

impl Appearing {
    pub(super) fn new() -> Self {
        Self {
            lines: Vec::new(),
            shown_line: 0,
            revealed: 0.,
            shown_width: 0.,
            skip: 0.,
            real: 0.,
            width: WidthSweep::Settled { target: 0. },
            shown_height: 0.,
            height: HeightOpen::Settled { target: 0. },
            sweeps: Sweeps::Fresh,
            pace: Pace::Draft,
        }
    }

    /// A new draft's text, or the final message's (`finalizing`): the
    /// next sweep is a first one again (`countOptimalSize`).
    pub(super) fn text_changed(&mut self, finalizing: bool) {
        self.sweeps = Sweeps::Fresh;
        self.pace = if finalizing { Pace::Final } else { Pace::Draft };
    }

    /// Step to `now` with the lines laid out last frame and the room the
    /// text leaves for the footer (`skip`).
    pub(super) fn frame(&mut self, lines: Vec<RevealLine>, skip: f32, now: Instant) -> Frame {
        let fresh = self.sweeps == Sweeps::Fresh;
        if lines != self.lines {
            self.lines = lines;
        }
        if let WidthSweep::Running { target, tween, .. } = self.width {
            self.revealed = tween.value(now);
            if tween.done(now) {
                self.width = WidthSweep::Settled { target };
            }
        }
        let real = self.lines.iter().map(|line| line.width).fold(0., f32::max);
        self.skip = skip;
        self.real = real;
        let rtl = self.lines.get(self.shown_line).is_some_and(|line| line.rtl);
        // `textAppearWidthCallback`.
        self.shown_width = if rtl {
            real
        } else {
            self.shown_width.max(self.revealed + skip).min(real)
        };
        if let HeightOpen::Running { target, tween } = self.height {
            self.shown_height = tween.value(now);
            if tween.done(now) {
                self.height = HeightOpen::Settled { target };
            }
        }
        let using = self.validate(now);
        let cut = match self.lines.get(self.shown_line) {
            Some(line) if using => RevealCut::Line {
                line: *line,
                revealed: self.revealed,
            },
            _ => RevealCut::All,
        };
        let width = (using && self.shown_width < real).then_some(self.shown_width);
        if self.sweeps == Sweeps::Fresh {
            self.sweeps = Sweeps::First;
        }
        Frame {
            cut,
            height: self.shown_height,
            width,
            moving: fresh || (using && (self.width.running() || self.height.running())),
        }
    }

    /// `textAppearValidate`: on to the next line while nothing animates.
    fn validate(&mut self, now: Instant) -> bool {
        // Each pass moves one line down; the lines bound the loop.
        for _ in 0..=self.lines.len() {
            if !self.check_line(now) {
                return false;
            }
            if self.width.running() || self.height.running() {
                return true;
            }
            self.shown_line += 1;
            self.revealed = 0.;
            self.width = WidthSweep::Settled { target: 0. };
        }
        false
    }

    /// `textAppearCheckLine`: start what the current line needs; false
    /// once everything shows.
    fn check_line(&mut self, now: Instant) -> bool {
        let count = self.lines.len();
        let shown = self.shown_line;
        let Some(line) = self.lines.get(shown).copied() else {
            self.width = WidthSweep::Settled {
                target: self.width.target(),
            };
            self.height = HeightOpen::Settled {
                target: self.height.target(),
            };
            // Everything shows: the bubble at the text's full height.
            if let Some(last) = self.lines.last() {
                self.shown_height = last.bottom;
            }
            return false;
        };
        let last = shown + 1 == count;
        let using = !last || self.revealed < line.width || self.shown_height < line.bottom;
        if !using {
            return false;
        }
        if self.width.target() != line.width {
            if self.revealed >= line.width {
                self.revealed = line.width;
                self.width = WidthSweep::Settled { target: line.width };
            } else {
                self.start_width(line, last, now);
            }
        }
        let target = self.target_height_now();
        if self.height.target() != target {
            if shown == 0
                && let Some(first) = self.lines.first()
            {
                self.shown_height = first.bottom;
            }
            if self.shown_height >= target {
                self.shown_height = target;
                self.height = HeightOpen::Settled { target };
            } else if self.opens_now() {
                self.height = HeightOpen::Running {
                    target,
                    tween: Tween {
                        from: self.shown_height,
                        to: target,
                        start: now,
                        duration: self.height_duration(),
                        ease: ease_out_cubic,
                    },
                };
            }
        }
        true
    }

    /// Whether the next line opens now: no sweep runs, one opens already,
    /// the sweep has the height's time or less to go, or the revealed edge
    /// runs into the footer's room.
    fn opens_now(&self) -> bool {
        let WidthSweep::Running {
            start,
            target,
            duration,
            ..
        } = self.width
        else {
            return true;
        };
        let progress = if self.revealed >= target {
            1.
        } else {
            (self.revealed - start) / (target - start).max(f32::EPSILON)
        };
        let left = duration.mul_f32((1. - progress).clamp(0., 1.));
        // The footer sits past the revealed edge; once the edge runs into
        // it, the next line opens to make room below.
        let crowded = self.revealed + crate::ui::text_reveal::GRADIENT + self.skip > self.real;
        self.height.running() || left <= self.height_duration() || crowded
    }

    /// `textAppearStartWidthAnimation`.
    fn start_width(&mut self, line: RevealLine, last: bool, now: Instant) {
        let full = match self.pace {
            Pace::Final => LINE_FINAL,
            Pace::Draft => LINE,
        };
        let computed = if last {
            full
        } else {
            full.mul_f32((line.width / MAX_WIDTH).max(0.))
                .max(SHORTEST_SWEEP)
        };
        let duration = match std::mem::replace(&mut self.sweeps, Sweeps::Later) {
            Sweeps::Later => computed,
            Sweeps::Fresh | Sweeps::First => computed.max(FIRST_SWEEP),
        };
        self.width = WidthSweep::Running {
            start: self.revealed,
            target: line.width,
            duration,
            tween: Tween {
                from: self.revealed,
                to: line.width,
                start: now,
                duration,
                ease: linear,
            },
        };
    }

    fn height_duration(&self) -> Duration {
        match self.pace {
            Pace::Final => HEIGHT_FINAL,
            Pace::Draft => HEIGHT,
        }
    }

    /// `textAppearTargetHeight`: open to the bottom of the next line.
    fn target_height_now(&self) -> f32 {
        let next = self.shown_line + 1;
        if next + 1 >= self.lines.len() {
            return self.lines.last().map_or(0., |line| line.bottom);
        }
        self.lines.get(next).map_or(0., |line| line.bottom)
    }
}

#[cfg(test)]
#[path = "bot_stream_appear_tests.rs"]
mod tests;
