//! The story cover picker's rules: where the strip's frames are cut, how a
//! pointer position maps to a time, and the seconds `editStoryCover` takes.
#![deny(clippy::indexing_slicing)]

/// Frames in the strip.
pub const STRIP_FRAMES: usize = 8;

/// The cover cannot sit on the very last instant: the decoder has no
/// frame there.
const END_GUARD_MS: i64 = 100;

/// The latest time a cover can be at.
pub fn last_ms(duration_ms: i64) -> i64 {
    (duration_ms - END_GUARD_MS).max(0)
}

/// Where the strip's frames are cut: the middle of each equal slice.
pub fn frame_times(duration_ms: i64, count: usize) -> Vec<i64> {
    let count = count.max(1);
    let last = last_ms(duration_ms);
    (0..count)
        .map(|index| {
            let middle = (index as f64 + 0.5) / count as f64;
            ((last as f64 * middle) as i64).clamp(0, last)
        })
        .collect()
}

/// The time under a position `fraction` (0 to 1) along the strip.
pub fn time_at(fraction: f32, duration_ms: i64) -> i64 {
    let fraction = if fraction.is_finite() {
        f64::from(fraction).clamp(0.0, 1.0)
    } else {
        0.0
    };
    ((last_ms(duration_ms) as f64) * fraction).round() as i64
}

/// The position along the strip of a time.
pub fn fraction_of(at_ms: i64, duration_ms: i64) -> f32 {
    let last = last_ms(duration_ms);
    if last <= 0 {
        return 0.0;
    }
    (at_ms.clamp(0, last) as f64 / last as f64) as f32
}

/// `editStoryCover.main_frame_timestamp`.
pub fn timestamp_secs(at_ms: i64) -> f64 {
    at_ms.max(0) as f64 / 1000.0
}

/// `0:07` or `1:02:03`, tenths are left out.
pub fn time_label(at_ms: i64) -> String {
    let total = at_ms.max(0) / 1000;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_inside_the_clip_and_ordered() {
        let times = frame_times(12_000, STRIP_FRAMES);
        assert_eq!(times.len(), STRIP_FRAMES);
        assert!(times.is_sorted() && times.first() < times.last());
        assert!(times.iter().all(|at| (0..12_000).contains(at)));
    }

    #[test]
    fn short_clips_do_not_go_negative() {
        assert_eq!(frame_times(0, 3), vec![0, 0, 0]);
        assert_eq!(time_at(1.0, 50), 0);
    }

    #[test]
    fn position_and_time_round_trip() {
        let at = time_at(0.5, 10_000);
        assert_eq!(at, 4_950);
        assert!((fraction_of(at, 10_000) - 0.5).abs() < 0.001);
        assert_eq!(time_at(f32::NAN, 10_000), 0);
        assert_eq!(time_at(2.0, 10_000), 9_900);
    }

    #[test]
    fn labels() {
        assert_eq!(time_label(7_400), "0:07");
        assert_eq!(time_label(3_723_000), "1:02:03");
        assert_eq!(timestamp_secs(1_500), 1.5);
    }
}
