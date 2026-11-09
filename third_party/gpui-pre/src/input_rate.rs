//! Quill: `InputRateTracker`, out of `window.rs` so `scripts/test-input-rate.sh`
//! can build it alone and drive it with its own instants.

use std::time::{Duration, Instant};

/// Tracks input event timestamps to determine if input is arriving at a high rate.
/// Used for selective VRR (Variable Refresh Rate) optimization.
#[derive(Clone, Debug)]
pub(crate) struct InputRateTracker {
    timestamps: Vec<Instant>,
    window: Duration,
    inputs_per_second: u32,
    sustain_until: Instant,
    sustain_duration: Duration,
}

impl Default for InputRateTracker {
    fn default() -> Self {
        Self {
            timestamps: Vec::new(),
            window: Duration::from_millis(100),
            inputs_per_second: 60,
            sustain_until: Instant::now(),
            // Quill: 1 s upstream, which re-presented the unchanged frame for a
            // second after every gesture. Gaps inside a gesture are far shorter.
            sustain_duration: Duration::from_millis(150),
        }
    }
}

impl InputRateTracker {
    pub fn record_input(&mut self, now: Instant) {
        self.timestamps.push(now);
        self.prune_old_timestamps(now);

        let min_events = self.inputs_per_second as u128 * self.window.as_millis() / 1000;
        if self.timestamps.len() as u128 >= min_events {
            self.sustain_until = now + self.sustain_duration;
        }
    }

    pub fn is_high_rate(&self, now: Instant) -> bool {
        now < self.sustain_until
    }

    fn prune_old_timestamps(&mut self, now: Instant) {
        self.timestamps
            .retain(|&t| now.duration_since(t) <= self.window);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(16);

    fn after_gesture(frames: u32) -> (InputRateTracker, Instant) {
        let mut tracker = InputRateTracker::default();
        let mut now = Instant::now();
        for _ in 0..frames {
            now += FRAME;
            tracker.record_input(now);
        }
        (tracker, now)
    }

    #[test]
    fn a_burst_of_input_is_high_rate() {
        let (tracker, last) = after_gesture(6);
        assert!(tracker.is_high_rate(last));
    }

    #[test]
    fn slow_input_is_not_high_rate() {
        let mut tracker = InputRateTracker::default();
        let mut now = Instant::now();
        for _ in 0..30 {
            now += Duration::from_millis(50);
            tracker.record_input(now);
            assert!(!tracker.is_high_rate(now));
        }
    }

    #[test]
    fn a_gap_inside_a_gesture_stays_high_rate() {
        let (tracker, last) = after_gesture(120);
        assert!(tracker.is_high_rate(last + Duration::from_millis(100)));
    }

    #[test]
    fn the_tail_ends_150_ms_after_the_last_input() {
        let (tracker, last) = after_gesture(120);
        assert!(!tracker.is_high_rate(last + Duration::from_millis(151)));
    }
}
