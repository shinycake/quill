//! The player's clock.

use std::time::Instant;

/// Whether time runs, and why not.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Run {
    Paused,
    /// Holding still after a seek until the first picture is shown;
    /// `playing` is whether playback was asked for.
    HeldAfterSeek { playing: bool },
    /// Time runs from `anchor`.
    Running { anchor: Instant },
}

/// A wall clock that can be paused, seeked and sped up. It holds still
/// after a seek until the first picture is shown, so decoding the first
/// frame doesn't eat into playback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    base: f64,
    run: Run,
    pub(super) rate: f64,
}

impl Clock {
    pub fn new() -> Self {
        Self {
            base: 0.0,
            run: Run::HeldAfterSeek { playing: false },
            rate: 1.0,
        }
    }

    pub fn position(&self, now: Instant) -> f64 {
        match self.run {
            Run::Running { anchor } => {
                self.base + now.saturating_duration_since(anchor).as_secs_f64() * self.rate
            }
            Run::Paused | Run::HeldAfterSeek { .. } => self.base,
        }
    }

    /// Playback was asked for (even while held).
    fn wants_play(&self) -> bool {
        matches!(
            self.run,
            Run::Running { .. } | Run::HeldAfterSeek { playing: true }
        )
    }

    /// Running when `playing` and not held, from `now`.
    fn running_if(playing: bool, now: Instant) -> Run {
        if playing {
            Run::Running { anchor: now }
        } else {
            Run::Paused
        }
    }

    pub fn play(&mut self, now: Instant) {
        self.base = self.position(now);
        self.run = match self.run {
            Run::HeldAfterSeek { .. } => Run::HeldAfterSeek { playing: true },
            Run::Paused | Run::Running { .. } => Run::Running { anchor: now },
        };
    }

    pub fn pause(&mut self, now: Instant) {
        self.base = self.position(now);
        self.run = match self.run {
            Run::HeldAfterSeek { .. } => Run::HeldAfterSeek { playing: false },
            Run::Paused | Run::Running { .. } => Run::Paused,
        };
    }

    pub fn is_playing(&self) -> bool {
        self.wants_play()
    }

    /// Held after a seek, waiting for the first picture.
    pub(super) fn is_held(&self) -> bool {
        matches!(self.run, Run::HeldAfterSeek { .. })
    }

    /// Jump to `secs` and hold there until [`Clock::release`].
    pub fn seek(&mut self, secs: f64, _now: Instant) {
        self.base = secs.max(0.0);
        self.run = Run::HeldAfterSeek {
            playing: self.wants_play(),
        };
    }

    /// Follow an external clock (the sound): jump without holding.
    pub fn follow(&mut self, secs: f64, now: Instant) {
        self.base = secs.max(0.0);
        self.run = Self::running_if(self.wants_play(), now);
    }

    /// The first picture after a seek is up: time starts running.
    pub fn release(&mut self, now: Instant) {
        if let Run::HeldAfterSeek { playing } = self.run {
            self.run = Self::running_if(playing, now);
        }
    }

    pub fn set_rate(&mut self, rate: f64, now: Instant) {
        self.base = self.position(now);
        if let Run::Running { .. } = self.run {
            self.run = Run::Running { anchor: now };
        }
        self.rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}
