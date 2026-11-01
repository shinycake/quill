//! Local wall-clock dates for user-facing labels.
//!
//! Formatting is pure over an explicit UTC offset (tests stay
//! deterministic on any CI clock); the `*_local` entry points read the
//! offset the OS applies to that instant, so DST transitions are honored.
//! No chrono/time dependency: civil-date math is Howard Hinnant's
//! days-from-civil inverse.
#![deny(clippy::indexing_slicing)]

use std::time::{SystemTime, UNIX_EPOCH};

/// A broken-down wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilTime {
    pub year: i64,
    /// 1–12.
    pub month: u8,
    /// 1–31.
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u8,
}

impl CivilTime {
    /// Days since 1970-01-01 of this civil date; equal dates compare equal
    /// regardless of time of day.
    pub fn day_number(&self) -> i64 {
        let (y, m, d) = (self.year, i64::from(self.month), i64::from(self.day));
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }
}

/// Wall-clock time of `unix` seconds at `offset_secs` east of UTC.
pub fn civil_at(unix: i64, offset_secs: i32) -> CivilTime {
    let local = unix + i64::from(offset_secs);
    let days = local.div_euclid(86_400);
    let secs_of_day = local.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    CivilTime {
        year,
        month: month as u8,
        day: day as u8,
        hour: (secs_of_day / 3600) as u8,
        minute: ((secs_of_day % 3600) / 60) as u8,
        // 1970-01-01 was a Thursday (index 3).
        weekday: (days + 3).rem_euclid(7) as u8,
    }
}

/// The OS's UTC offset (seconds east) in effect at `unix`. Falls back to
/// UTC where the platform offers no zone lookup.
pub fn utc_offset_at(unix: i64) -> i32 {
    // Renders format every visible stamp each frame, so remember the
    // offset of the last UTC hour asked about.
    thread_local! {
        static BY_HOUR: std::cell::RefCell<HourOffset> = const { std::cell::RefCell::new(HourOffset::EMPTY) };
    }
    BY_HOUR.with(|cell| cell.borrow_mut().offset_at(unix, os_utc_offset_at))
}

/// The offset of one UTC hour in which the zone does not change.
struct HourOffset {
    hour: i64,
    offset: i32,
}

impl HourOffset {
    const EMPTY: Self = Self {
        hour: i64::MIN,
        offset: 0,
    };

    /// Some zones change in the middle of a UTC hour (Lord Howe at 15:30
    /// UTC), so an hour is kept only when it starts and ends on one offset.
    fn offset_at(&mut self, unix: i64, lookup: impl Fn(i64) -> i32) -> i32 {
        let hour = unix.div_euclid(3600);
        if hour == self.hour {
            return self.offset;
        }
        let start = lookup(hour * 3600);
        if lookup(hour * 3600 + 3599) != start {
            return lookup(unix);
        }
        *self = Self {
            hour,
            offset: start,
        };
        start
    }
}

fn os_utc_offset_at(unix: i64) -> i32 {
    #[cfg(unix)]
    {
        let t = unix as libc::time_t;
        // SAFETY: `libc::tm` is plain integers (and a pointer on some
        // platforms), so all-zero is a valid value.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: `localtime_r` writes only into the `tm` we own and reads
        // `t`; a null return means failure and leaves us on UTC.
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
    #[cfg(windows)]
    {
        windows_utc_offset_at(unix)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = unix;
        0
    }
}

/// The local offset (seconds east of UTC) at `unix` on Windows: convert
/// the instant to local time with the current time-zone rules (DST
/// included) and take the difference.
#[cfg(windows)]
fn windows_utc_offset_at(unix: i64) -> i32 {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{
        FileTimeToSystemTime, SystemTimeToFileTime, SystemTimeToTzSpecificLocalTime,
    };
    // FILETIME counts 100 ns ticks since 1601-01-01.
    let ticks = (unix + 11_644_473_600).max(0) as u64 * 10_000_000;
    let utc_file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: plain value conversions into structs we own; any failure
    // returns 0 and leaves us on UTC.
    unsafe {
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        let mut local: SYSTEMTIME = std::mem::zeroed();
        let mut local_file: FILETIME = std::mem::zeroed();
        if FileTimeToSystemTime(&utc_file, &mut utc) == 0
            || SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) == 0
            || SystemTimeToFileTime(&local, &mut local_file) == 0
        {
            return 0;
        }
        let local_ticks =
            (u64::from(local_file.dwHighDateTime) << 32) | u64::from(local_file.dwLowDateTime);
        ((local_ticks as i64 - ticks as i64) / 10_000_000) as i32
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Local wall-clock time of `unix`.
pub fn civil_local(unix: i64) -> CivilTime {
    civil_at(unix, utc_offset_at(unix))
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

pub fn month_name(month: u8) -> &'static str {
    MONTHS
        .get(usize::from(month.clamp(1, 12) - 1))
        .copied()
        .unwrap_or_default()
}

pub fn weekday_name(weekday: u8) -> &'static str {
    WEEKDAYS
        .get(usize::from(weekday.min(6)))
        .copied()
        .unwrap_or_default()
}

/// The time of day in the system's clock style: `21:42` or `9:42 PM`
/// (see [`crate::clock_style`]).
pub fn hhmm(time: &CivilTime) -> String {
    crate::clock_style::ClockStyle::current().format(time)
}

/// tdesktop `langDateMaybeWithYear`: the year shows only when the date is
/// more than a year from now, or in the neighbouring year but more than
/// three months away across the new year ("Dec 28" still reads without it
/// in January).
pub fn needs_year(date: &CivilTime, now: &CivilTime) -> bool {
    let much_greater = |year: i64, other: i64| year > other + 1;
    let month_much_greater = |year: i64, month: i64, other_year: i64, other_month: i64| {
        year == other_year + 1 && month + 12 > other_month + 3
    };
    let (month, now_month) = (i64::from(date.month), i64::from(now.month));
    date.year != now.year
        && (much_greater(date.year, now.year)
            || much_greater(now.year, date.year)
            || month_much_greater(date.year, month, now.year, now_month)
            || month_much_greater(now.year, now_month, date.year, month))
}

/// Day separator label for a message dated `date`, seen at `now`, as
/// Telegram Desktop prints it (`langDayOfMonthFull`): "October 6", with
/// the year by [`needs_year`] ("October 6, 2024") — always the date,
/// never "Today".
pub fn day_label(date: &CivilTime, now: &CivilTime) -> String {
    if !needs_year(date, now) {
        format!("{} {}", month_name(date.month), date.day)
    } else {
        format!("{} {}, {}", month_name(date.month), date.day, date.year)
    }
}

/// Compact chat-list timestamp: `HH:MM` today or within the last 20 hours
/// (Telegram Desktop's `FormatDialogsDate`), a short weekday within the
/// past week, `12 Mar` this year, `12.03.25` otherwise.
pub fn chat_list_stamp(date: &CivilTime, now: &CivilTime) -> String {
    const RECENT_MINUTES: i64 = 20 * 60;
    let minutes =
        |t: &CivilTime| t.day_number() * 1440 + i64::from(t.hour) * 60 + i64::from(t.minute);
    let age = now.day_number() - date.day_number();
    if (minutes(now) - minutes(date)).abs() < RECENT_MINUTES {
        return hhmm(date);
    }
    match age {
        i64::MIN..=0 => hhmm(date),
        1..=6 => weekday_name(date.weekday)
            .get(..3)
            .unwrap_or_default()
            .to_string(),
        _ if date.year == now.year => {
            format!(
                "{} {}",
                date.day,
                month_name(date.month).get(..3).unwrap_or_default()
            )
        }
        _ => short_date(date),
    }
}

/// The numeric short date, `12.03.25` (tdesktop prints
/// `QLocale().toString(date, ShortFormat)`).
pub fn short_date(date: &CivilTime) -> String {
    format!(
        "{:02}.{:02}.{:02}",
        date.day,
        date.month,
        date.year.rem_euclid(100)
    )
}

/// `12 March 2026, 21:42` — full local timestamp for tooltips and receipts.
pub fn full_stamp(time: &CivilTime) -> String {
    format!(
        "{} {} {}, {}",
        time.day,
        month_name(time.month),
        time.year,
        hhmm(time)
    )
}

/// The media viewer's date under the sender's name: "today at 14:05",
/// "yesterday at 09:30", "12 March at 21:42" this year and
/// "12 March 2025 at 21:42" earlier (Telegram Desktop `langDateTime`).
pub fn viewer_stamp(date: &CivilTime, now: &CivilTime) -> String {
    let time = hhmm(date);
    match now.day_number() - date.day_number() {
        0 => format!("today at {time}"),
        1 => format!("yesterday at {time}"),
        _ if !needs_year(date, now) => {
            format!("{} {} at {time}", date.day, month_name(date.month))
        }
        _ => format!(
            "{} {} {} at {time}",
            date.day,
            month_name(date.month),
            date.year
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_stamp_names_today_and_yesterday() {
        // 2026-09-28 21:42 UTC.
        let now = civil_at(1_790_631_720, 0);
        assert_eq!(
            viewer_stamp(&civil_at(1_790_631_720 - 3600, 0), &now),
            "today at 20:42"
        );
        assert_eq!(
            viewer_stamp(&civil_at(1_790_631_720 - 86_400, 0), &now),
            "yesterday at 21:42"
        );
        assert_eq!(
            viewer_stamp(&civil_at(1_790_631_720 - 5 * 86_400, 0), &now),
            "23 September at 21:42"
        );
        assert_eq!(
            viewer_stamp(&civil_at(951_782_400 + 3600, 0), &now),
            "29 February 2000 at 01:00"
        );
    }

    // 2026-09-28 21:42:00 UTC, a Monday.
    const T: i64 = 1_790_631_720;

    #[test]
    fn an_offset_change_inside_a_utc_hour_is_not_cached_across_it() {
        // Lord Howe: 02:00 +10:30 becomes 02:30 +11 at 15:30 UTC.
        let switch = 1_791_041_400;
        let lord_howe = |t: i64| if t < switch { 37_800 } else { 39_600 };
        let mut cache = HourOffset::EMPTY;
        assert_eq!(cache.offset_at(switch + 600, lord_howe), 39_600);
        assert_eq!(cache.offset_at(switch - 600, lord_howe), 37_800);
        assert_eq!(cache.offset_at(switch + 4000, lord_howe), 39_600);
        assert_eq!(cache.offset_at(switch + 3600, lord_howe), 39_600);
    }

    #[test]
    fn civil_matches_known_dates() {
        let utc = civil_at(T, 0);
        assert_eq!(
            (utc.year, utc.month, utc.day, utc.hour, utc.minute),
            (2026, 9, 28, 21, 42)
        );
        assert_eq!(weekday_name(utc.weekday), "Monday");
        let epoch = civil_at(0, 0);
        assert_eq!((epoch.year, epoch.month, epoch.day), (1970, 1, 1));
        assert_eq!(weekday_name(epoch.weekday), "Thursday");
        let leap = civil_at(951_782_400, 0); // 2000-02-29
        assert_eq!((leap.year, leap.month, leap.day), (2000, 2, 29));
    }

    #[test]
    fn offset_moves_across_midnight() {
        // UTC-5 (Chicago, CDT): 16:42 the same day; UTC+3: 00:42 next day.
        let chicago = civil_at(T, -5 * 3600);
        assert_eq!((chicago.day, chicago.hour), (28, 16));
        let east = civil_at(T, 3 * 3600);
        assert_eq!((east.day, east.hour, east.minute), (29, 0, 42));
        assert_eq!(weekday_name(east.weekday), "Tuesday");
    }

    #[test]
    fn day_number_round_trips() {
        for unix in [0, T, 951_782_400, -86_400] {
            let c = civil_at(unix, 0);
            assert_eq!(c.day_number(), unix.div_euclid(86_400));
        }
    }

    #[test]
    fn day_labels_relative_to_now() {
        let now = civil_at(T, 0);
        let label = |secs_ago: i64| day_label(&civil_at(T - secs_ago, 0), &now);
        assert_eq!(label(60), "September 28");
        assert_eq!(label(86_400), "September 27");
        assert_eq!(label(30 * 86_400), "August 29");
        assert_eq!(label(400 * 86_400), "August 24, 2025");
    }

    #[test]
    fn the_year_follows_tdesktops_three_month_rule() {
        let year_needed = |year: i64, month: u8, now_year: i64, now_month: u8| {
            let at = |year, month| CivilTime {
                year,
                month,
                day: 15,
                hour: 0,
                minute: 0,
                weekday: 0,
            };
            needs_year(&at(year, month), &at(now_year, now_month))
        };
        // Same year: never.
        assert!(!year_needed(2026, 1, 2026, 12));
        // Neighbouring year: only when more than three months apart.
        assert!(!year_needed(2025, 12, 2026, 1));
        assert!(!year_needed(2025, 12, 2026, 3));
        assert!(year_needed(2025, 12, 2026, 4));
        assert!(!year_needed(2027, 1, 2026, 12));
        assert!(year_needed(2027, 4, 2026, 12));
        assert!(year_needed(2025, 6, 2026, 2));
        // More than a year apart: always.
        assert!(year_needed(2024, 12, 2026, 1));
        assert!(year_needed(2028, 1, 2026, 12));
    }

    #[test]
    fn separators_drop_the_year_across_new_year_within_three_months() {
        // 2026-01-10 and a message from 2025-12-28: still "December 28".
        let now = civil_at(T - 261 * 86_400, 0);
        assert_eq!((now.year, now.month), (2026, 1));
        let december = civil_at(1_766_880_000, 0);
        assert_eq!(
            (december.year, december.month, december.day),
            (2025, 12, 28)
        );
        assert_eq!(day_label(&december, &now), "December 28");
        // Seen from September the same message carries its year.
        let later = civil_at(T, 0);
        assert_eq!(day_label(&december, &later), "December 28, 2025");
    }

    #[test]
    fn chat_list_stamps() {
        let now = civil_at(T, 0);
        let stamp = |secs_ago: i64| chat_list_stamp(&civil_at(T - secs_ago, 0), &now);
        assert_eq!(stamp(60), "21:41");
        assert_eq!(stamp(2 * 86_400), "Sat");
        assert_eq!(stamp(30 * 86_400), "29 Aug");
        assert_eq!(stamp(400 * 86_400), "24.08.25");
        // Last night, within 20 hours: still the time.
        let early = civil_at(T + 4 * 3600, 0);
        assert_eq!(chat_list_stamp(&civil_at(T, 0), &early), "21:42");
        assert_eq!(
            chat_list_stamp(&civil_at(T + 4 * 3600 - 21 * 3600, 0), &early),
            "Mon"
        );
    }

    #[test]
    fn full_stamp_reads_naturally() {
        assert_eq!(full_stamp(&civil_at(T, 0)), "28 September 2026, 21:42");
    }
}
