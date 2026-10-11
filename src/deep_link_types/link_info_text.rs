//! Wording for the gift code and language pack boxes. Pure, so it is tested
//! without a window. Gift wording follows tdesktop `boxes/gift_premium_box.cpp`
//! (`lng_gift_link_*`).

/// "3 months", "1 year", "10 days": the gift's length. tdesktop shows days
/// below 30, whole months below a year and years after that.
pub fn gift_duration(month_count: i64, day_count: i64) -> String {
    let days = if day_count > 0 {
        day_count
    } else {
        month_count * 30
    };
    let (count, unit) = if days < 30 {
        (days.max(1), "day")
    } else if days < 30 * 12 {
        (days / 30, "month")
    } else {
        (days / (30 * 12), "year")
    };
    if count == 1 {
        format!("1 {unit}")
    } else {
        format!("{count} {unit}s")
    }
}

/// The "Reason" row: `lng_gift_link_reason_*`.
pub fn gift_reason(is_from_giveaway: bool, used: bool) -> &'static str {
    match (is_from_giveaway, used) {
        (true, true) => "Giveaway",
        (true, false) => "Incomplete Giveaway",
        (false, _) => "You were selected by the channel",
    }
}

/// How much of a language pack is translated, e.g. "90% translated".
pub fn language_progress(total: i64, translated: i64) -> Option<String> {
    (total > 0).then(|| format!("{}% translated", (translated.clamp(0, total) * 100) / total))
}

#[cfg(test)]
mod tests {
    use super::{gift_duration, gift_reason, language_progress};

    #[test]
    fn duration_uses_days_months_and_years() {
        assert_eq!(gift_duration(0, 7), "7 days");
        assert_eq!(gift_duration(1, 30), "1 month");
        assert_eq!(gift_duration(3, 90), "3 months");
        assert_eq!(gift_duration(12, 360), "1 year");
        assert_eq!(gift_duration(24, 720), "2 years");
        assert_eq!(gift_duration(3, 0), "3 months");
    }

    #[test]
    fn reason_names_giveaways() {
        assert_eq!(gift_reason(true, true), "Giveaway");
        assert_eq!(gift_reason(true, false), "Incomplete Giveaway");
        assert_eq!(
            gift_reason(false, false),
            "You were selected by the channel"
        );
    }

    #[test]
    fn progress_is_a_clamped_percentage() {
        assert_eq!(language_progress(0, 0), None);
        assert_eq!(
            language_progress(200, 50).as_deref(),
            Some("25% translated")
        );
        assert_eq!(
            language_progress(10, 99).as_deref(),
            Some("100% translated")
        );
    }
}
