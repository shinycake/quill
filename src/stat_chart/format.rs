//! Number and date labels of the statistics screens, as Telegram Desktop
//! prints them (`lang_tag.cpp`, `statistics_format_values.cpp`).

use super::{ONE_DAY_MS, StatChart};
use crate::local_time::{CivilTime, civil_at};

/// `FormatCountToShort`: `9999`, `12.3K`, `4M`, `1.2B` (one decimal,
/// truncated, dropped when zero).
pub fn format_short(number: i64) -> String {
    let abs = number.unsigned_abs();
    let shorten = |divider: u64, suffix: char| {
        let rounded = abs / (divider / 10);
        let sign = if number < 0 { "-" } else { "" };
        if !rounded.is_multiple_of(10) {
            format!("{sign}{}.{}{suffix}", rounded / 10, rounded % 10)
        } else {
            format!("{sign}{}{suffix}", rounded / 10)
        }
    };
    if abs >= 1_000_000_000 {
        shorten(1_000_000_000, 'B')
    } else if abs >= 1_000_000 {
        shorten(1_000_000, 'M')
    } else if abs >= 10_000 {
        shorten(1_000, 'K')
    } else {
        number.to_string()
    }
}

/// `FormatCountDecimal`: `12,345`.
pub fn format_decimal(number: i64) -> String {
    let digits = number.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 + 1);
    if number < 0 {
        out.push('-');
    }
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A ruler caption: short from 10,000 up, grouped below.
pub fn ruler_caption(value: i64) -> String {
    if value >= 10_000 {
        format_short(value)
    } else {
        format_decimal(value)
    }
}

/// The colored change next to an overview value: `+1,234 (5.6%)` or
/// `−87 (2%)`; `None` with no change or no previous value. The bool is
/// true for a decrease.
pub fn overview_delta(value: f64, previous: f64, growth_percentage: f64) -> Option<(String, bool)> {
    let diff = value - previous;
    if diff == 0.0 || previous == 0.0 {
        return None;
    }
    let abs = diff.abs();
    let diff_text = if abs > 1_000_000.0 {
        format_short(abs as i64)
    } else {
        trim_number(abs)
    };
    let percentage = growth_percentage.abs();
    let percentage = if percentage == percentage.trunc() {
        format!("{percentage:.0}")
    } else {
        format!("{percentage:.1}")
    };
    let sign = if diff < 0.0 { '\u{2212}' } else { '+' };
    Some((format!("{sign}{diff_text} ({percentage}%)"), diff < 0.0))
}

/// `QString::number(double)`: no trailing zeros, at most six
/// significant digits after the point.
fn trim_number(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let text = format!("{value:.6}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The overview's big number (`FormatCountToShort` of the value).
pub fn overview_value(value: f64) -> String {
    format_short(value.round() as i64)
}

/// "Enabled Notifications": `61.5%` (two decimals at most).
pub fn percent_label(value: f64) -> String {
    format!("{}%", trim_number((value * 100.0).round() / 100.0))
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn month(date: &CivilTime) -> &'static str {
    MONTHS[usize::from(date.month.clamp(1, 12) - 1)]
}

/// Days are UTC days in Telegram's statistics; read them in UTC so a
/// western time zone does not shift every label back a day.
fn utc_day(seconds: i64) -> CivilTime {
    civil_at(seconds, 0)
}

/// `12 Oct`.
pub fn day_month(seconds: i64) -> String {
    let date = utc_day(seconds);
    format!("{} {}", date.day, month(&date))
}

/// `12 Oct 2024`.
pub fn day_month_year(seconds: i64) -> String {
    let date = utc_day(seconds);
    format!("{} {} {}", date.day, month(&date), date.year)
}

/// The tooltip's date: `Sat, 12 Oct 2024` for a day, `Sat, 12 Oct 14:00`
/// for an hour (`LangDetailedDayMonth`); the time reads in the viewer's
/// zone, `offset_secs` east of UTC.
pub fn detailed_day(seconds: i64, offset_secs: i32) -> String {
    let utc = utc_day(seconds);
    if utc.hour != 0 || utc.minute != 0 {
        let local = civil_at(seconds, offset_secs);
        format!(
            "{}, {} {} {:02}:{:02}",
            WEEKDAYS[usize::from(local.weekday.min(6))],
            local.day,
            month(&local),
            local.hour,
            local.minute
        )
    } else {
        format!(
            "{}, {} {} {}",
            WEEKDAYS[usize::from(utc.weekday.min(6))],
            utc.day,
            month(&utc),
            utc.year
        )
    }
}

const SEVEN_DAYS: i64 = 7 * 86_400;

impl StatChart {
    /// The x-axis label of point `i` (`getDayString`): `00:00`-style
    /// hours for an hourly series, `14:30` within a day, `12 Oct` for days.
    pub fn day_label(&self, i: usize, offset_secs: i32) -> String {
        let Some(&x) = self.x.get(i) else {
            return String::new();
        };
        if self.time_step == 1.0 {
            let index = ((x - self.x[0]) / self.time_step) as i64;
            format!("{index:02}:00")
        } else if self.time_step < ONE_DAY_MS {
            let time = civil_at((x / 1000.0) as i64, offset_secs);
            format!("{:02}:{:02}", time.hour, time.minute)
        } else {
            day_month((x / 1000.0) as i64)
        }
    }

    /// The chart header's date range for points `min..=max`
    /// (`HeaderSubTitle`); empty for series that are not dates.
    pub fn range_title(&self, min: usize, max: usize) -> String {
        let (Some(&left), Some(&right)) = (self.x.get(min), self.x.get(max)) else {
            return String::new();
        };
        if left < ONE_DAY_MS {
            return String::new();
        }
        let left_text = day_month_year((left / 1000.0) as i64);
        if min == max && !self.week_format {
            return left_text;
        }
        let right = (right / 1000.0) as i64 + if self.week_format { SEVEN_DAYS } else { 0 };
        format!("{left_text} \u{2014} {}", day_month_year(right))
    }

    /// The tooltip header for point `i`.
    pub fn point_title(&self, i: usize, offset_secs: i32) -> String {
        let Some(&x) = self.x.get(i) else {
            return String::new();
        };
        let seconds = (x / 1000.0) as i64;
        if x < ONE_DAY_MS {
            self.day_label(i, offset_secs)
        } else if self.week_format {
            format!(
                "{} \u{2014} {}",
                day_month(seconds),
                day_month_year(seconds + SEVEN_DAYS)
            )
        } else {
            detailed_day(seconds, offset_secs)
        }
    }
}

/// Each line's share of the total at one point, rounded to whole
/// percents that still add up to 100 (`PiePartsPercentage`): the
/// rounding error goes to the part rounded the most. Disabled lines
/// count as zero.
pub fn percentages(values: &[i64], enabled: &[bool]) -> Vec<String> {
    let sums: Vec<f64> = values
        .iter()
        .enumerate()
        .map(|(k, v)| {
            if enabled.get(k).copied().unwrap_or(true) {
                *v as f64
            } else {
                0.0
            }
        })
        .collect();
    super::pie_parts(&sums, true)
        .into_iter()
        .map(|part| part.text)
        .collect()
}
