//! Audio/voice-note playback clock (Phase 4.6).
//!
//! Pure seek-state machine behind the history-row seek bars: it tracks the
//! elapsed position of one track against the TDLib `duration`, with
//! pause/resume and clamped seeking. There is no GPUI and no subprocess
//! here — the UI layer (`src/ui/mod.rs`) owns the ffplay child, the
//! `SliderState` entity, and the tick that re-renders while playing.

use std::time::Instant;

/// Playback position for one audio/voice track.
///
/// The clock has two pieces: a frozen `base_secs` offset (the pause point,
/// seek target, or resume point) and an optional `started_at` instant while
/// the player is running. Elapsed time is `base + now - started`, always
/// clamped to `[0, duration]`.
#[derive(Debug, Clone)]
pub struct PlaybackClock {
    duration_secs: f64,
    base_secs: f64,
    started_at: Option<Instant>,
}

impl PlaybackClock {
    /// New clock for a track of `duration_secs` (negative clamped to 0),
    /// paused at position 0.
    pub fn new(duration_secs: f64) -> Self {
        Self {
            duration_secs: duration_secs.max(0.0),
            base_secs: 0.0,
            started_at: None,
        }
    }

    /// Total track length in seconds.
    pub fn duration_secs(&self) -> f64 {
        self.duration_secs
    }

    /// True while the player is running (elapsed advances).
    pub fn is_playing(&self) -> bool {
        self.started_at.is_some()
    }

    /// Seconds elapsed, clamped to `[0, duration]`.
    pub fn elapsed_secs(&self) -> f64 {
        let mut elapsed = self.base_secs;
        if let Some(started) = self.started_at {
            elapsed += started.elapsed().as_secs_f64();
        }
        elapsed.clamp(0.0, self.duration_secs)
    }

    /// Fraction of the track played, in `[0, 1]` (0 when duration is 0).
    pub fn fraction(&self) -> f64 {
        if self.duration_secs <= 0.0 {
            return 0.0;
        }
        (self.elapsed_secs() / self.duration_secs).clamp(0.0, 1.0)
    }

    /// Freeze the clock at the current elapsed position.
    pub fn pause(&mut self) {
        self.base_secs = self.elapsed_secs();
        self.started_at = None;
    }

    /// Continue advancing from the frozen position.
    pub fn resume(&mut self) {
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
    }

    /// Move the playhead, clamped to `[0, duration]`. Keeps the
    /// playing/paused state: seeking while playing resumes from the new
    /// position, seeking while paused just moves the frozen offset.
    pub fn seek(&mut self, secs: f64) {
        self.base_secs = secs.clamp(0.0, self.duration_secs);
        if self.started_at.is_some() {
            self.started_at = Some(Instant::now());
        }
    }

    /// True when a running clock has reached the end of the track.
    pub fn finished(&self) -> bool {
        self.is_playing() && self.elapsed_secs() >= self.duration_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_clock_starts_paused_at_zero() {
        let clock = PlaybackClock::new(120.0);
        assert!(!clock.is_playing());
        assert_eq!(clock.elapsed_secs(), 0.0);
        assert_eq!(clock.fraction(), 0.0);
        assert!(!clock.finished());
    }

    #[test]
    fn negative_duration_is_clamped_to_zero() {
        let clock = PlaybackClock::new(-5.0);
        assert_eq!(clock.duration_secs(), 0.0);
        assert_eq!(clock.fraction(), 0.0);
    }

    #[test]
    fn seek_clamps_to_zero_and_duration() {
        let mut clock = PlaybackClock::new(60.0);
        clock.seek(-10.0);
        assert_eq!(clock.elapsed_secs(), 0.0);
        clock.seek(10_000.0);
        assert_eq!(clock.elapsed_secs(), 60.0);
        assert_eq!(clock.fraction(), 1.0);
    }

    #[test]
    fn pause_freezes_elapsed_resume_continues() {
        let mut clock = PlaybackClock::new(60.0);
        clock.resume();
        assert!(clock.is_playing());
        clock.seek(12.5);
        // Elapsed keeps advancing while playing (never goes backwards).
        assert!(clock.elapsed_secs() >= 12.5);
        clock.pause();
        assert!(!clock.is_playing());
        let frozen = clock.elapsed_secs();
        assert_eq!(
            clock.elapsed_secs(),
            frozen,
            "paused clock must not advance"
        );
        clock.resume();
        assert!(clock.is_playing());
        assert!(clock.elapsed_secs() >= frozen);
    }

    #[test]
    fn seek_while_playing_keeps_playing_from_new_position() {
        let mut clock = PlaybackClock::new(60.0);
        clock.resume();
        clock.seek(30.0);
        assert!(clock.is_playing());
        assert!((clock.elapsed_secs() - 30.0).abs() < 0.5);
    }

    #[test]
    fn seek_while_paused_moves_frozen_offset() {
        let mut clock = PlaybackClock::new(60.0);
        clock.seek(20.0);
        assert!(!clock.is_playing());
        assert_eq!(clock.elapsed_secs(), 20.0);
        clock.pause();
        assert_eq!(clock.elapsed_secs(), 20.0);
    }

    #[test]
    fn finished_only_when_playing_past_the_end() {
        let mut clock = PlaybackClock::new(60.0);
        clock.seek(60.0);
        assert!(!clock.finished(), "paused at the end is not finished");
        clock.resume();
        assert!(clock.finished());
        clock.pause();
        assert!(!clock.finished());
    }

    #[test]
    fn pause_resume_are_idempotent() {
        let mut clock = PlaybackClock::new(60.0);
        clock.pause();
        clock.pause();
        assert!(!clock.is_playing());
        clock.resume();
        clock.resume();
        assert!(clock.is_playing());
    }
}
