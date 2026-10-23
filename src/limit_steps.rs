//! The discrete positions of the size and time sliders in tdesktop's
//! Data & Storage settings: the local storage limits
//! (`settings/sections/settings_local_storage.cpp`, `TotalSizeLimit`,
//! `TimeLimit`) and the auto-download size caps
//! (`export/view/export_view_settings.cpp`, `SizeLimitByIndex`, used by
//! `boxes/auto_download_box.cpp`'s `AddSizeLimitSlider`). A slider holds a
//! step index; these functions turn it into a limit and back.
#![deny(clippy::indexing_slicing)]

const KIB: i64 = 1024;
const MIB: i64 = 1024 * KIB;
const GIB: i64 = 1024 * MIB;
const DAY: i64 = 86_400;

/// Slider positions of "Total size limit": tdesktop's 18 `TotalSizeLimit`
/// steps (200 MB to 900 MB, then 1 GB to 10 GB) plus one last "No limit"
/// step, because TDLib's default (optimizer off) must stay reachable.
pub const SIZE_STEPS: usize = 19;

/// Slider positions of "Clear files older than": tdesktop's 15
/// `TimeLimitInDays` steps (1 to 3 weeks, then 1 to 12 months) plus
/// "Never".
pub const KEEP_STEPS: usize = 16;

/// Slider positions of an auto-download size cap (`kSizeValueCount`).
pub const CAP_STEPS: usize = 100;

/// tdesktop `TotalSizeLimit(index)`; the last step is "No limit"
/// (`None`), and so is any position past the end.
pub fn size_limit_at(index: usize) -> Option<i64> {
    let index = i64::try_from(index).ok()?;
    match index {
        0..=7 => Some((index + 2) * 100 * MIB),
        8..=17 => Some((index - 7) * GIB),
        _ => None,
    }
}

/// The slider position nearest to `limit` (`None` = "No limit").
pub fn size_limit_index(limit: Option<i64>) -> usize {
    let Some(limit) = limit else {
        return SIZE_STEPS - 1;
    };
    (0..SIZE_STEPS - 1)
        .min_by_key(|&ix| size_limit_at(ix).map_or(i64::MAX, |step| (step - limit).abs()))
        .unwrap_or(0)
}

/// tdesktop `TimeLimitInDays(index)` for the 15 finite steps.
fn keep_days_at(index: usize) -> Option<i64> {
    let index = i64::try_from(index).ok()?;
    match index {
        0..=2 => Some((index + 1) * 7),
        3..=14 => {
            let month = index - 2;
            let adjust = match month {
                12.. => 5,
                10..=11 => 4,
                8..=9 => 3,
                7 => 2,
                5..=6 => 1,
                3..=4 => 0,
                2 => -1,
                _ => 1,
            };
            Some(month * 30 + adjust)
        }
        _ => None,
    }
}

/// tdesktop `TimeLimit(index)` in seconds; the last step is "Never"
/// (`None`).
pub fn keep_limit_at(index: usize) -> Option<i64> {
    keep_days_at(index).map(|days| days * DAY)
}

/// The slider position nearest to `keep` (`None` = "Never").
pub fn keep_limit_index(keep: Option<i64>) -> usize {
    let Some(keep) = keep else {
        return KEEP_STEPS - 1;
    };
    (0..KEEP_STEPS - 1)
        .min_by_key(|&ix| keep_limit_at(ix).map_or(i64::MAX, |step| (step - keep).abs()))
        .unwrap_or(0)
}

/// tdesktop `SizeLimitByIndex(index)` in bytes: 1 to 10 MB in steps of 1,
/// then 2, 5, 10, 20, 50, 100 and 200 MB steps up to 4000 MB. Positions
/// past the end clamp to the last one.
pub fn cap_at(index: usize) -> i64 {
    let index = i64::try_from(index.min(CAP_STEPS - 1)).unwrap_or(0) + 1;
    let megabytes = match index {
        ..=10 => index,
        11..=30 => 10 + (index - 10) * 2,
        31..=40 => 50 + (index - 30) * 5,
        41..=60 => 100 + (index - 40) * 10,
        61..=70 => 300 + (index - 60) * 20,
        71..=80 => 500 + (index - 70) * 50,
        81..=90 => 1000 + (index - 80) * 100,
        _ => 2000 + (index - 90) * 200,
    };
    megabytes * MIB
}

/// The slider position nearest to `bytes`; values past the top step land
/// on the last position.
pub fn cap_index(bytes: i64) -> usize {
    (0..CAP_STEPS)
        .min_by_key(|&ix| (cap_at(ix) - bytes).abs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_steps_follow_tdesktop() {
        assert_eq!(size_limit_at(0), Some(200 * MIB));
        assert_eq!(size_limit_at(7), Some(900 * MIB));
        assert_eq!(size_limit_at(8), Some(GIB));
        assert_eq!(size_limit_at(17), Some(10 * GIB));
        assert_eq!(size_limit_at(SIZE_STEPS - 1), None);
        assert_eq!(size_limit_index(None), SIZE_STEPS - 1);
        assert_eq!(size_limit_index(Some(2 * GIB)), 9);
        assert_eq!(size_limit_index(Some(1)), 0);
        for ix in 0..SIZE_STEPS - 1 {
            assert_eq!(size_limit_index(size_limit_at(ix)), ix);
        }
    }

    #[test]
    fn keep_steps_follow_tdesktop() {
        assert_eq!(keep_limit_at(0), Some(7 * DAY));
        assert_eq!(keep_limit_at(2), Some(21 * DAY));
        assert_eq!(keep_limit_at(3), Some(31 * DAY));
        assert_eq!(keep_limit_at(4), Some(59 * DAY));
        assert_eq!(keep_limit_at(5), Some(90 * DAY));
        assert_eq!(keep_limit_at(14), Some(365 * DAY));
        assert_eq!(keep_limit_at(KEEP_STEPS - 1), None);
        assert_eq!(keep_limit_index(None), KEEP_STEPS - 1);
        for ix in 0..KEEP_STEPS - 1 {
            assert_eq!(keep_limit_index(keep_limit_at(ix)), ix);
        }
    }

    #[test]
    fn cap_steps_follow_tdesktop() {
        assert_eq!(cap_at(0), MIB);
        assert_eq!(cap_at(9), 10 * MIB);
        assert_eq!(cap_at(10), 12 * MIB);
        assert_eq!(cap_at(29), 50 * MIB);
        assert_eq!(cap_at(39), 100 * MIB);
        assert_eq!(cap_at(59), 300 * MIB);
        assert_eq!(cap_at(79), 1000 * MIB);
        assert_eq!(cap_at(89), 2000 * MIB);
        assert_eq!(cap_at(CAP_STEPS - 1), 4000 * MIB);
        assert_eq!(cap_at(500), 4000 * MIB);
        for ix in 0..CAP_STEPS {
            assert_eq!(cap_index(cap_at(ix)), ix);
        }
        assert_eq!(cap_index(8000 * MIB), CAP_STEPS - 1);
        assert_eq!(cap_index(0), 0);
    }
}
