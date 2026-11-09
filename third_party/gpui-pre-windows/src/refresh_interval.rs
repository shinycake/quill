// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

use std::time::Duration;

pub(crate) const VSYNC_INTERVAL_THRESHOLD: Duration = Duration::from_millis(1);
pub(crate) const DEFAULT_VSYNC_INTERVAL: Duration = Duration::from_micros(16_666); // ~60Hz
const MAX_VSYNC_INTERVAL: Duration = Duration::from_millis(100);

/// A 29 microsecond period was seen (from a `qpcRefreshPeriod` of 60).
pub(crate) fn refresh_interval(
    qpc_period: u64,
    qpc_frequency: u64,
    rate_numerator: u32,
    rate_denominator: u32,
) -> Duration {
    [
        interval_from_counts(qpc_period, qpc_frequency),
        interval_from_counts(rate_denominator.into(), rate_numerator.into()),
    ]
    .into_iter()
    .flatten()
    .find(|interval| (VSYNC_INTERVAL_THRESHOLD..=MAX_VSYNC_INTERVAL).contains(interval))
    .unwrap_or(DEFAULT_VSYNC_INTERVAL)
}

fn interval_from_counts(counts: u64, counts_per_second: u64) -> Option<Duration> {
    let micros = (u128::from(counts) * 1_000_000).checked_div(counts_per_second.into())?;
    u64::try_from(micros).ok().map(Duration::from_micros)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QPC_10_MHZ: u64 = 10_000_000;
    const SIXTY_HZ: Duration = Duration::from_micros(16_666);

    fn micros(n: u64) -> Duration {
        Duration::from_micros(n)
    }

    #[test]
    fn a_normal_display_uses_the_qpc_period() {
        assert_eq!(refresh_interval(166_666, QPC_10_MHZ, 60, 1), micros(16_666));
        assert_eq!(
            refresh_interval(166_666, QPC_10_MHZ, 144, 1),
            micros(16_666)
        );
    }

    #[test]
    fn a_qpc_clock_of_a_fractional_megahertz_stays_accurate() {
        assert_eq!(refresh_interval(238_636, 14_318_180, 60, 1), micros(16_666));
    }

    #[test]
    fn the_seen_period_of_60_counts_falls_back_to_the_refresh_rate() {
        assert_eq!(refresh_interval(60, QPC_10_MHZ, 60, 1), micros(16_666));
    }

    #[test]
    fn a_zero_period_falls_back_to_a_fractional_refresh_rate() {
        assert_eq!(
            refresh_interval(0, QPC_10_MHZ, 60_000, 1001),
            micros(16_683)
        );
    }

    #[test]
    fn the_refresh_rate_fallback_reads_every_rate_shape() {
        for (numerator, denominator, expected) in [
            (59_940, 1000, 16_683),
            (144_000, 1000, 6_944),
            (148_500_000, 2_475_000, 16_666),
        ] {
            assert_eq!(
                refresh_interval(0, QPC_10_MHZ, numerator, denominator),
                micros(expected),
                "{numerator}/{denominator}"
            );
        }
    }

    #[test]
    fn nothing_usable_gives_the_default() {
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 0, 0), SIXTY_HZ);
    }

    #[test]
    fn a_qpc_clock_under_1_mhz_gives_the_period() {
        assert_eq!(refresh_interval(546, 32_768, 60, 1), micros(16_662));
    }

    #[test]
    fn a_zero_qpc_frequency_falls_back_to_the_refresh_rate() {
        assert_eq!(refresh_interval(166_666, 0, 120, 1), micros(8_333));
    }

    #[test]
    fn a_zero_rate_denominator_gives_the_default() {
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 60_000_000, 0), SIXTY_HZ);
    }

    #[test]
    fn a_huge_period_never_becomes_a_sleep() {
        assert_eq!(
            refresh_interval(u64::MAX, QPC_10_MHZ, 120, 1),
            micros(8_333)
        );
        assert_eq!(refresh_interval(u64::MAX, 1, 0, 0), SIXTY_HZ);
    }

    #[test]
    fn a_period_too_long_to_count_does_not_wrap_into_a_usable_one() {
        let wraps_to_16_640 = (((15_460u128 << 64) + 16_640) / 1_000_000) as u64;
        assert_eq!(wraps_to_16_640, 285_186_663_379_549_668);
        assert_eq!(refresh_interval(wraps_to_16_640, 1, 0, 0), SIXTY_HZ);
    }

    #[test]
    fn a_rate_outside_the_usable_range_gives_the_default() {
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 5, 1), SIXTY_HZ);
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 2_000, 1), SIXTY_HZ);
    }

    #[test]
    fn the_usable_range_includes_both_ends() {
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 1_000, 1), micros(1_000));
        assert_eq!(refresh_interval(0, QPC_10_MHZ, 10, 1), micros(100_000));
    }

    #[test]
    fn every_input_gives_a_usable_interval() {
        let wide = [0, 1, 59, 60, 999, 1_000_000, u32::MAX as u64, u64::MAX];
        let narrow = [0, 1, 10, 60, 1_000, 999_999, 1_000_000, u32::MAX];
        for qpc_period in wide {
            for qpc_frequency in wide {
                for rate_numerator in narrow {
                    for rate_denominator in narrow {
                        let interval = refresh_interval(
                            qpc_period,
                            qpc_frequency,
                            rate_numerator,
                            rate_denominator,
                        );
                        assert!(
                            (micros(1_000)..=micros(100_000)).contains(&interval),
                            "{interval:?} from {qpc_period}, {qpc_frequency}, \
                             {rate_numerator}/{rate_denominator}"
                        );
                    }
                }
            }
        }
    }
}
