//! Amount, date and program formatting for the revenue and affiliate
//! screens, after tdesktop `info/channel_statistics/earn/earn_format.cpp`
//! and `info/bot/starref/info_bot_starref_common.cpp`.

use crate::premium_hub::{StarAmount, group_digits};

/// Nanotons in one TON (`chatRevenueAmount` counts in the smallest unit).
pub const NANOS_PER_TON: i64 = 1_000_000_000;

/// tdesktop's minus sign in signed TON amounts.
pub const MINUS: char = '\u{2212}';

/// The duration slider's "forever" stop (tdesktop `kDurationForeverValue`).
pub const DURATION_FOREVER: i32 = 999;

/// The whole TON of `nanos` (tdesktop `MajorPart`): no grouping, the
/// fraction cut off, no sign.
pub fn ton_major(nanos: i64) -> String {
    (nanos / NANOS_PER_TON).unsigned_abs().to_string()
}

/// The fraction of `nanos` as a decimal point and two digits, cut off
/// rather than rounded (tdesktop `MinorPart`).
pub fn ton_minor(nanos: i64) -> String {
    let fraction = (nanos % NANOS_PER_TON).unsigned_abs();
    format!(".{:02}", fraction / 10_000_000)
}

/// `12.34` for `nanos`.
pub fn ton_label(nanos: i64) -> String {
    format!("{}{}", ton_major(nanos), ton_minor(nanos))
}

/// `+12.34` / `−12.34` for a history row.
pub fn ton_signed_label(nanos: i64, incoming: bool) -> String {
    let sign = if incoming { '+' } else { MINUS };
    format!("{sign}{}", ton_label(nanos))
}

/// `123,456.78` with `decimals` places (QLocale's grouped fixed format).
fn grouped_fixed(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let whole = whole
        .parse::<i64>()
        .map(group_digits)
        .unwrap_or_else(|_| whole.to_string());
    let sign = if value < 0.0 && text.chars().any(|c| matches!(c, '1'..='9')) {
        "-"
    } else {
        ""
    };
    if fraction.is_empty() {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{fraction}")
    }
}

/// `≈$1,234.56` (tdesktop `ToUsd`); `decimals` 0 means 2. TDLib's
/// `usd_rate` for TON is US cents per nanoton.
pub fn ton_to_usd(nanos: i64, cents_per_nano: f64, decimals: usize) -> String {
    let usd = nanos as f64 * cents_per_nano / 100.0;
    format!(
        "\u{2248}${}",
        grouped_fixed(usd, if decimals == 0 { 2 } else { decimals })
    )
}

/// Stars value as a float (whole plus nanostars).
pub fn star_value(amount: StarAmount) -> f64 {
    amount.stars as f64 + f64::from(amount.nanos) / NANOS_PER_TON as f64
}

/// `≈$12.34` for Stars; TDLib's `usd_rate` for Stars is US cents per Star.
pub fn stars_to_usd(amount: StarAmount, cents_per_star: f64, decimals: usize) -> String {
    let usd = star_value(amount) * cents_per_star / 100.0;
    format!(
        "\u{2248}${}",
        grouped_fixed(usd, if decimals == 0 { 2 } else { decimals })
    )
}

/// tdesktop `FormatCommission`: `commission / 10.` printed the shortest
/// way, then `%` (`200` → `20%`, `25` → `2.5%`).
pub fn format_commission(permille: i32) -> String {
    if permille % 10 == 0 {
        format!("{}%", permille / 10)
    } else {
        format!("{}.{}%", permille / 10, (permille % 10).abs())
    }
}

fn plural(count: i32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

/// tdesktop `FormatProgramDuration`: "Forever", "3 months", "2 years".
pub fn format_program_duration(months: i32) -> String {
    if months == 0 {
        "Forever".into()
    } else if months < 12 {
        plural(months, "month", "months")
    } else {
        plural(months / 12, "year", "years")
    }
}

/// tdesktop `FormatForProgramDuration` with the bold markers dropped:
/// "for lifetime", "for 3 months", "for 2 years".
pub fn format_for_program_duration(months: i32) -> String {
    if months == 0 {
        "for lifetime".into()
    } else if months < 12 {
        format!("for {}", plural(months, "month", "months"))
    } else {
        format!("for {}", plural(months / 12, "year", "years"))
    }
}

/// The duration slider's tick labels: `1m`, `2y`, `∞` (999 is forever).
pub fn duration_tick(months: i32) -> String {
    if months < 12 {
        format!("{months}m")
    } else if months < DURATION_FOREVER {
        format!("{}y", months / 12)
    } else {
        "\u{221E}".into()
    }
}

/// tdesktop `FormatTimeLeft`: `1:02:03` or `2:03`.
pub fn format_time_left(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// tdesktop's history dates (`lng_group_call_starts_short_date` with
/// `langDayOfMonth`): "Mar 3, 14:05".
pub fn earn_date(unix: i64) -> String {
    let time = crate::local_time::civil_local(unix);
    let month = crate::local_time::month_name(time.month);
    let short = month.get(..3).unwrap_or(month);
    format!("{short} {}, {}", time.day, crate::local_time::hhmm(&time))
}
