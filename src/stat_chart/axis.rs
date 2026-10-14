//! The value axis of a statistics chart: a domain whose even steps all
//! read as round numbers. tdesktop (`chart_rulers_data.cpp`) splits the
//! shown values' own minimum and maximum into five steps, which gives
//! rulers like 11,437; Quill rounds the ends out so every ruler reads
//! cleanly, with the same six rulers.

/// The step multipliers a ruler step can take, times a power of ten.
const NICE_STEPS: [f64; 12] = [
    1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 15.0, 20.0,
];

/// The domain `(low, high)` covering `min..=max` in `intervals` equal
/// steps of a round size (1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10, 15 or 20
/// times a power of ten). Statistics count whole things, so a step is a
/// whole number of at least 1. A non-negative series stays non-negative;
/// a flat one gets a step of room above it.
pub fn nice_domain(min: f64, max: f64, intervals: u32) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() || intervals == 0 {
        return (0.0, 1.0);
    }
    let (min, max) = if min <= max { (min, max) } else { (max, min) };
    let count = f64::from(intervals);
    let span = if max > min {
        max - min
    } else {
        (max.abs() * 0.2).max(count)
    };
    let raw = span / count;
    let power = 10f64.powf(raw.log10().floor()).max(1.0);
    for multiplier in NICE_STEPS {
        let step = multiplier * power;
        if step < 1.0 || step.fract() != 0.0 {
            continue;
        }
        let low = (min / step).floor() * step;
        let high = low + step * count;
        if high > max || (high >= max && max > min) {
            return (low, high);
        }
    }
    (min, min + span)
}

#[cfg(test)]
mod tests {
    use super::nice_domain;

    #[test]
    fn rounds_out_to_round_steps() {
        assert_eq!(nice_domain(11_020.0, 12_437.0, 5), (10_800.0, 12_800.0));
        assert_eq!(nice_domain(0.0, 97.0, 5), (0.0, 100.0));
        assert_eq!(nice_domain(0.0, 100.0, 5), (0.0, 100.0));
        assert_eq!(nice_domain(3.0, 7.0, 4), (3.0, 7.0));
        // A small count never gets fractional rulers.
        assert_eq!(nice_domain(0.0, 1.0, 5), (0.0, 5.0));
        assert_eq!(nice_domain(0.0, 3.0, 3), (0.0, 3.0));
    }

    #[test]
    fn stays_above_zero_and_handles_flat_and_odd_input() {
        let (low, high) = nice_domain(12.0, 12.0, 5);
        assert!((0.0..=12.0).contains(&low) && high > 12.0);
        assert_eq!(nice_domain(0.0, 0.0, 5), (0.0, 5.0));
        assert_eq!(nice_domain(f64::NAN, 3.0, 5), (0.0, 1.0));
        assert_eq!(nice_domain(5.0, 1.0, 4), nice_domain(1.0, 5.0, 4));
        assert_eq!(nice_domain(1.0, 2.0, 0), (0.0, 1.0));
    }
}
