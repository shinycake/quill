//! Batch 6: Settings → Data & Storage → local storage limits (tdesktop
//! `Settings::LocalStorage`: "Total size limit" and "Clear files older
//! than"). TDLib keeps the cache itself: the limits are the options its
//! storage optimizer reads (`td/telegram/files/FileGcParameters.cpp`),
//! and the optimizer only runs while `use_storage_optimizer` is on.

use crate::telegram::envelope::OptionValue;

const KIB: i64 = 1024;
const MIB: i64 = 1024 * KIB;
const GIB: i64 = 1024 * MIB;
const DAY: i64 = 86_400;

/// Total size limits offered, in bytes. A subset of tdesktop's list
/// (`TotalSizeLimit`: 200 MB-900 MB, then 1-10 GB), as chips.
pub const SIZE_LIMITS: [i64; 6] = [200 * MIB, 500 * MIB, GIB, 2 * GIB, 5 * GIB, 10 * GIB];

/// "Clear files older than" choices, in seconds. tdesktop's
/// `TimeLimitInDays`: 1 week, then months of 31/59/90/.../181/.../365 days.
pub const KEEP_LIMITS: [i64; 5] = [7 * DAY, 31 * DAY, 90 * DAY, 181 * DAY, 365 * DAY];

/// `storage_max_files_size` is in KiB; this reads as "no size limit"
/// (`FileGcParameters` shifts it left by 10 bits).
pub const UNLIMITED_KIB: i64 = 1 << 40;
/// `storage_max_time_from_last_access` that reads as "never".
pub const KEEP_FOREVER_SECS: i64 = 2_000_000_000;
/// `storage_max_file_count` that reads as "no limit" (TDLib's default of
/// 40000 would otherwise cap the cache silently).
pub const UNLIMITED_FILE_COUNT: i64 = 2_000_000_000;

/// The TDLib options behind the limits, as last reported by `updateOption`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StorageLimits {
    pub optimizer_on: bool,
    max_files_size_kib: Option<i64>,
    max_time_secs: Option<i64>,
}

impl StorageLimits {
    /// Fold one `updateOption`; unrelated names are ignored.
    pub fn apply_option(&mut self, name: &str, value: &OptionValue) {
        match (name, value) {
            ("use_storage_optimizer", OptionValue::Boolean(on)) => self.optimizer_on = *on,
            ("storage_max_files_size", OptionValue::Integer(kib)) => {
                self.max_files_size_kib = Some(*kib)
            }
            ("storage_max_time_from_last_access", OptionValue::Integer(secs)) => {
                self.max_time_secs = Some(*secs)
            }
            _ => {}
        }
    }

    /// The enforced total size limit in bytes, `None` = no limit.
    pub fn size_limit(&self) -> Option<i64> {
        if !self.optimizer_on {
            return None;
        }
        self.max_files_size_kib
            .filter(|kib| *kib > 0 && *kib < UNLIMITED_KIB)
            .map(|kib| kib.saturating_mul(KIB))
    }

    /// How long a file stays after its last access, `None` = forever.
    pub fn keep_for(&self) -> Option<i64> {
        if !self.optimizer_on {
            return None;
        }
        self.max_time_secs
            .filter(|secs| *secs > 0 && *secs < KEEP_FOREVER_SECS)
    }
}

/// A `setOption` to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageOptionValue {
    Boolean(bool),
    Integer(i64),
}

/// The options that make TDLib enforce exactly `size` bytes and `keep`
/// seconds (`None` = no limit of that kind). With neither limit the
/// optimizer is switched off. Sent in this order so the limits are in
/// place before the optimizer is enabled.
pub fn options_for(
    size: Option<i64>,
    keep: Option<i64>,
) -> [(&'static str, StorageOptionValue); 4] {
    [
        (
            "storage_max_files_size",
            StorageOptionValue::Integer(size.map_or(UNLIMITED_KIB, |bytes| bytes / KIB)),
        ),
        (
            "storage_max_time_from_last_access",
            StorageOptionValue::Integer(keep.unwrap_or(KEEP_FOREVER_SECS)),
        ),
        (
            "storage_max_file_count",
            StorageOptionValue::Integer(UNLIMITED_FILE_COUNT),
        ),
        (
            "use_storage_optimizer",
            StorageOptionValue::Boolean(size.is_some() || keep.is_some()),
        ),
    ]
}

/// tdesktop `SizeLimitText`.
pub fn size_limit_label(bytes: i64) -> String {
    let mb = bytes / MIB;
    let gb = mb / 1024;
    if gb > 0 {
        format!("{gb} GB")
    } else {
        format!("{mb} MB")
    }
}

/// tdesktop `TimeLimitText` (`lng_weeks` / `lng_months`).
pub fn keep_label(secs: i64) -> String {
    let days = secs / DAY;
    let months = days / 29;
    let weeks = days / 7;
    if months > 0 {
        format!("{months} month{}", if months == 1 { "" } else { "s" })
    } else if secs > 0 {
        format!("{weeks} week{}", if weeks == 1 { "" } else { "s" })
    } else {
        "Never".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_read_from_tdlib_options() {
        let mut limits = StorageLimits::default();
        assert_eq!(limits.size_limit(), None);
        limits.apply_option("storage_max_files_size", &OptionValue::Integer(2 * 1024 * 1024));
        limits.apply_option(
            "storage_max_time_from_last_access",
            &OptionValue::Integer(31 * DAY),
        );
        // Values alone do nothing while the optimizer is off.
        assert_eq!(limits.size_limit(), None);
        assert_eq!(limits.keep_for(), None);
        limits.apply_option("use_storage_optimizer", &OptionValue::Boolean(true));
        assert_eq!(limits.size_limit(), Some(2 * GIB));
        assert_eq!(limits.keep_for(), Some(31 * DAY));
    }

    #[test]
    fn sentinel_values_read_as_unlimited() {
        let mut limits = StorageLimits::default();
        for (name, value) in options_for(None, Some(7 * DAY)) {
            let value = match value {
                StorageOptionValue::Boolean(on) => OptionValue::Boolean(on),
                StorageOptionValue::Integer(n) => OptionValue::Integer(n),
            };
            limits.apply_option(name, &value);
        }
        assert!(limits.optimizer_on);
        assert_eq!(limits.size_limit(), None);
        assert_eq!(limits.keep_for(), Some(7 * DAY));
    }

    #[test]
    fn no_limits_switches_the_optimizer_off() {
        let options = options_for(None, None);
        assert_eq!(options[3], ("use_storage_optimizer", StorageOptionValue::Boolean(false)));
        let on = options_for(Some(GIB), None);
        assert_eq!(on[0], ("storage_max_files_size", StorageOptionValue::Integer(1024 * 1024)));
        assert_eq!(on[3], ("use_storage_optimizer", StorageOptionValue::Boolean(true)));
    }

    #[test]
    fn labels_follow_tdesktop() {
        assert_eq!(size_limit_label(200 * MIB), "200 MB");
        assert_eq!(size_limit_label(5 * GIB), "5 GB");
        assert_eq!(keep_label(7 * DAY), "1 week");
        assert_eq!(keep_label(31 * DAY), "1 month");
        assert_eq!(keep_label(181 * DAY), "6 months");
        assert_eq!(keep_label(365 * DAY), "12 months");
        assert_eq!(keep_label(0), "Never");
    }
}
