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

/// Which sweep of the current text comes next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sweeps {
    First,
    Later,
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
    start_width: f32,
    target_width: f32,
    width_duration: Duration,
    width: Option<Tween>,
    shown_height: f32,
    target_height: f32,
    height: Option<Tween>,
    /// `startedForText`: whether this text's first sweep (at least
    /// [`FIRST_SWEEP`]) has started.
    sweeps: Sweeps,
    finalizing: bool,
    /// The text changed since the last frame: its lines show up in the
    /// next one, which has to be drawn.
    fresh: bool,
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
            start_width: 0.,
            target_width: 0.,
            width_duration: Duration::ZERO,
            width: None,
            shown_height: 0.,
            target_height: 0.,
            height: None,
            sweeps: Sweeps::First,
            finalizing: false,
            fresh: true,
        }
    }

    /// A new draft's text, or the final message's (`finalizing`): the
    /// next sweep is a first one again (`countOptimalSize`).
    pub(super) fn text_changed(&mut self, finalizing: bool) {
        self.sweeps = Sweeps::First;
        self.finalizing = finalizing;
        self.fresh = true;
    }

    /// Step to `now` with the lines laid out last frame and the room the
    /// text leaves for the footer (`skip`).
    pub(super) fn frame(&mut self, lines: Vec<RevealLine>, skip: f32, now: Instant) -> Frame {
        if lines != self.lines {
            self.lines = lines;
        }
        if let Some(width) = self.width {
            self.revealed = width.value(now);
            if width.done(now) {
                self.width = None;
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
        if let Some(height) = self.height {
            self.shown_height = height.value(now);
            if height.done(now) {
                self.height = None;
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
        Frame {
            cut,
            height: self.shown_height,
            width,
            moving: std::mem::take(&mut self.fresh)
                || (using && (self.width.is_some() || self.height.is_some())),
        }
    }

    /// `textAppearValidate`: on to the next line while nothing animates.
    fn validate(&mut self, now: Instant) -> bool {
        // Each pass moves one line down; the lines bound the loop.
        for _ in 0..=self.lines.len() {
            if !self.check_line(now) {
                return false;
            }
            if self.width.is_some() || self.height.is_some() {
                return true;
            }
            self.shown_line += 1;
            self.revealed = 0.;
            self.start_width = 0.;
            self.target_width = 0.;
        }
        false
    }

    /// `textAppearCheckLine`: start what the current line needs; false
    /// once everything shows.
    fn check_line(&mut self, now: Instant) -> bool {
        let count = self.lines.len();
        let shown = self.shown_line;
        let Some(line) = self.lines.get(shown).copied() else {
            self.width = None;
            self.height = None;
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
        if self.target_width != line.width {
            if self.revealed >= line.width {
                self.width = None;
                self.revealed = line.width;
                self.target_width = line.width;
            } else {
                self.start_width(line, last, now);
            }
        }
        let target = self.target_height_now();
        if self.target_height != target {
            if shown == 0
                && let Some(first) = self.lines.first()
            {
                self.shown_height = first.bottom;
            }
            if self.shown_height >= target {
                self.height = None;
                self.shown_height = target;
                self.target_height = target;
            } else {
                let progress = if self.revealed >= self.target_width {
                    1.
                } else {
                    (self.revealed - self.start_width)
                        / (self.target_width - self.start_width).max(f32::EPSILON)
                };
                let left = self.width_duration.mul_f32((1. - progress).clamp(0., 1.));
                // The footer sits past the revealed edge; once the edge
                // runs into it, the next line opens to make room below.
                let crowded =
                    self.revealed + crate::ui::text_reveal::GRADIENT + self.skip > self.real;
                if self.height.is_some()
                    || self.width.is_none()
                    || left <= self.height_duration()
                    || crowded
                {
                    self.height = Some(Tween {
                        from: self.shown_height,
                        to: target,
                        start: now,
                        duration: self.height_duration(),
                        ease: ease_out_cubic,
                    });
                    self.target_height = target;
                }
            }
        }
        true
    }

    /// `textAppearStartWidthAnimation`.
    fn start_width(&mut self, line: RevealLine, last: bool, now: Instant) {
        let full = if self.finalizing { LINE_FINAL } else { LINE };
        let computed = if last {
            full
        } else {
            full.mul_f32((line.width / MAX_WIDTH).max(0.))
                .max(SHORTEST_SWEEP)
        };
        let duration = match std::mem::replace(&mut self.sweeps, Sweeps::Later) {
            Sweeps::Later => computed,
            Sweeps::First => computed.max(FIRST_SWEEP),
        };
        self.width_duration = duration;
        self.start_width = self.revealed;
        self.target_width = line.width;
        self.width = Some(Tween {
            from: self.revealed,
            to: line.width,
            start: now,
            duration,
            ease: linear,
        });
    }

    fn height_duration(&self) -> Duration {
        if self.finalizing {
            HEIGHT_FINAL
        } else {
            HEIGHT
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
