//! Local wall-clock dates for user-facing labels.
//!
//! Formatting is pure over an explicit UTC offset (tests stay
//! deterministic on any CI clock); the `*_local` entry points read the
//! offset the OS applies to that instant, so DST transitions are honored.
//! No chrono/time dependency: civil-date math is Howard Hinnant's
//! days-from-civil inverse.

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
    // Renders format every visible stamp each frame; zones only change
    // offset on hour boundaries, so remember the answer per hour.
    thread_local! {
        static BY_HOUR: std::cell::RefCell<(i64, i32)> = const { std::cell::RefCell::new((i64::MIN, 0)) };
    }
    let hour = unix.div_euclid(3600);
    if let Some(offset) = BY_HOUR.with(|cell| {
        let (cached_hour, offset) = *cell.borrow();
        (cached_hour == hour).then_some(offset)
    }) {
        return offset;
    }
    let offset = os_utc_offset_at(unix);
    BY_HOUR.with(|cell| *cell.borrow_mut() = (hour, offset));
    offset
}

fn os_utc_offset_at(unix: i64) -> i32 {
    #[cfg(unix)]
    {
        let t = unix as libc::time_t;
        // SAFETY: `localtime_r` writes only into the zeroed `tm` we own and
        // reads `t`; a null return means failure and leaves us on UTC.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return 0;
        }
        tm.tm_gmtoff as i32
    }
    #[cfg(not(unix))]
    {
        let _ = unix;
        0
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
    MONTHS[usize::from(month.clamp(1, 12) - 1)]
}

pub fn weekday_name(weekday: u8) -> &'static str {
    WEEKDAYS[usize::from(weekday.min(6))]
}

/// `HH:MM`.
pub fn hhmm(time: &CivilTime) -> String {
    format!("{:02}:{:02}", time.hour, time.minute)
}

/// Day separator label for a message dated `date`, seen at `now`, as
/// Telegram Desktop prints it (`langDayOfMonthFull`): "October 6" this
/// year, "October 6, 2025" otherwise — always the date, never "Today".
pub fn day_label(date: &CivilTime, now: &CivilTime) -> String {
    if date.year == now.year {
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
        1..=6 => weekday_name(date.weekday)[..3].to_string(),
        _ if date.year == now.year => {
            format!("{} {}", date.day, &month_name(date.month)[..3])
        }
        _ => format!(
            "{:02}.{:02}.{:02}",
            date.day,
            date.month,
            date.year.rem_euclid(100)
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-09-28 21:42:00 UTC, a Monday.
    const T: i64 = 1_790_631_720;

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
