//! Telegram Business profile details: opening hours and the business
//! location (`userFullInfo.business_info`, TDLib 1.8.68). Pure data and
//! time math, so the profile rows and the tests share one implementation.
//! Behaviour follows tdesktop's `info_profile_actions.cpp`
//! (`CreateWorkingHours`, `OpensIn`, `FormatDayHours`); see
//! `docs/decisions/codex-profile-business-rows.md`.

use serde_json::Value;

const DAY: u32 = 24 * 60;
const WEEK: u32 = 7 * DAY;

/// `businessLocation`: the address and, when the business set a map pin,
/// its coordinates in millionths of a degree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BusinessLocation {
    pub address: String,
    pub point_e6: Option<(i32, i32)>,
}

impl BusinessLocation {
    /// The map link the location row opens (the same OpenStreetMap form
    /// the message location rows use).
    pub fn map_url(&self) -> Option<String> {
        let (lat, lon) = self.point_e6?;
        Some(format!(
            "https://www.openstreetmap.org/?mlat={:.6}&mlon={:.6}",
            f64::from(lat) / 1e6,
            f64::from(lon) / 1e6
        ))
    }
}

/// What a weekday shows in the schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayHours {
    Closed,
    /// Open the whole day.
    Full,
    /// Opening intervals as minutes of the day; `next_day_end` is set when
    /// the last one runs past midnight into the next day.
    Ranges {
        ranges: Vec<(u32, u32)>,
        next_day_end: Option<u32>,
    },
}

/// `businessOpeningHours`: intervals in minutes of the week (Monday 0),
/// merged, sorted and wrapped so every end is within the week.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpeningHours {
    /// Empty for the local-time variant.
    pub time_zone_id: String,
    pub intervals: Vec<(u32, u32)>,
}

impl OpeningHours {
    pub fn new(time_zone_id: String, raw: Vec<(u32, u32)>) -> Self {
        let mut wrapped = Vec::new();
        for (start, end) in raw {
            if start >= end || start >= WEEK {
                continue;
            }
            if end > WEEK {
                wrapped.push((start, WEEK));
                wrapped.push((0, (end - WEEK).min(WEEK)));
            } else {
                wrapped.push((start, end));
            }
        }
        wrapped.sort_unstable();
        let mut intervals: Vec<(u32, u32)> = Vec::new();
        for (start, end) in wrapped {
            match intervals.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => intervals.push((start, end)),
            }
        }
        Self {
            time_zone_id,
            intervals,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    /// Minutes until the business opens at `minute` of the week; 0 when
    /// it is open, `None` when there are no hours.
    pub fn minutes_until_open(&self, minute: u32) -> Option<u32> {
        let minute = minute % WEEK;
        self.intervals
            .iter()
            .map(|&(start, end)| {
                if start <= minute && minute < end {
                    0
                } else if start > minute {
                    start - minute
                } else {
                    start + WEEK - minute
                }
            })
            .min()
    }

    /// The seven rows of the expanded schedule, Monday first.
    pub fn days(&self) -> [DayHours; 7] {
        let mut out: [DayHours; 7] = std::array::from_fn(|_| DayHours::Closed);
        for (day, slot) in out.iter_mut().enumerate() {
            let (lo, hi) = (day as u32 * DAY, (day as u32 + 1) * DAY);
            let mut ranges: Vec<(u32, u32)> = self
                .intervals
                .iter()
                .filter(|&&(s, e)| s < hi && e > lo)
                .map(|&(s, e)| (s.max(lo) - lo, e.min(hi) - lo))
                .collect();
            if ranges.is_empty() {
                continue;
            }
            if ranges == [(0, DAY)] {
                *slot = DayHours::Full;
                continue;
            }
            // An interval that crosses midnight is shown on its first row as
            // "(next day)" and dropped from the next one.
            let crosses = |at: u32| {
                self.intervals
                    .iter()
                    .find(|&&(s, e)| s < at && e > at && at < WEEK)
                    .map(|&(_, e)| e - at)
            };
            let wraps = |next_day: usize| {
                // Sunday night into Monday: the week boundary splits it.
                next_day == 0
                    && self.intervals.iter().any(|&(_, e)| e == WEEK)
                    && self.intervals.first().is_some_and(|&(s, _)| s == 0)
            };
            let continues = ranges.first().is_some_and(|r| r.0 == 0)
                && (if day == 0 {
                    wraps(0)
                } else {
                    crosses(lo).is_some()
                });
            if continues {
                ranges.remove(0);
            }
            if ranges.is_empty() {
                continue;
            }
            let next_day_end = ranges.last().filter(|r| r.1 == DAY).and_then(|_| {
                if day == 6 {
                    wraps(0).then(|| self.intervals[0].1).filter(|&e| e < DAY)
                } else {
                    crosses(hi).filter(|&e| e < DAY)
                }
            });
            *slot = DayHours::Ranges {
                ranges,
                next_day_end,
            };
        }
        out
    }
}

/// `HH:MM` for a minute of the day (24:00 prints as 00:00).
pub fn format_minute(minute: u32) -> String {
    let minute = minute % DAY;
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

/// tdesktop's `FormatDayHours`: "open 24 hours", "closed" or the ranges,
/// one per line.
pub fn format_day(hours: &DayHours) -> String {
    match hours {
        DayHours::Closed => "closed".into(),
        DayHours::Full => "open 24 hours".into(),
        DayHours::Ranges {
            ranges,
            next_day_end,
        } => {
            let last = ranges.len().saturating_sub(1);
            ranges
                .iter()
                .enumerate()
                .map(|(i, &(s, e))| match (i == last, next_day_end) {
                    (true, Some(end)) => {
                        format!("{} - {} (next day)", format_minute(s), format_minute(*end))
                    }
                    _ => format!("{} - {}", format_minute(s), format_minute(e)),
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// "opens in 5 minutes" / "1 hour" / "2 days" (tdesktop's thresholds).
pub fn opens_in_text(minutes: u32) -> String {
    let plural =
        |n: u32, unit: &str| format!("opens in {n} {unit}{}", if n == 1 { "" } else { "s" });
    if minutes >= DAY {
        plural(minutes / DAY, "day")
    } else if minutes >= 60 {
        plural(minutes / 60, "hour")
    } else {
        plural(minutes.max(1), "minute")
    }
}

/// `userFullInfo.business_info`, the parts the profile shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BusinessInfo {
    pub location: Option<BusinessLocation>,
    /// In the business's time zone.
    pub opening_hours: Option<OpeningHours>,
    /// The same hours converted to the viewer's time zone by TDLib.
    pub local_opening_hours: Option<OpeningHours>,
    /// TDLib's snapshot, used when no local hours are known.
    pub next_open_in: i32,
    pub next_close_in: i32,
}

impl BusinessInfo {
    pub fn is_empty(&self) -> bool {
        self.location.is_none() && self.opening_hours.is_none()
    }

    /// Whether the two schedules differ, which is when the "local time" /
    /// "business time" switch is offered.
    pub fn has_distinct_local_hours(&self) -> bool {
        match (&self.opening_hours, &self.local_opening_hours) {
            (Some(a), Some(b)) => a.intervals != b.intervals,
            _ => false,
        }
    }

    /// `Some(0)` open now, `Some(n)` opens in `n` minutes, `None` unknown.
    /// `local_minute` is the viewer's minute of the week now.
    pub fn minutes_until_open(&self, local_minute: u32) -> Option<u32> {
        if let Some(local) = self.local_opening_hours.as_ref().filter(|h| !h.is_empty()) {
            return local.minutes_until_open(local_minute);
        }
        self.opening_hours.as_ref()?;
        let open = self.next_close_in > 0
            && (self.next_open_in == 0 || self.next_close_in < self.next_open_in);
        if open {
            Some(0)
        } else if self.next_open_in > 0 {
            Some((self.next_open_in as u32).div_ceil(60))
        } else {
            None
        }
    }
}

fn parse_hours(value: Option<&Value>) -> Option<OpeningHours> {
    let value = value.filter(|v| !v.is_null())?;
    let raw = value
        .get("opening_hours")?
        .as_array()?
        .iter()
        .filter_map(|i| {
            let start = u32::try_from(i.get("start_minute")?.as_i64()?).ok()?;
            let end = u32::try_from(i.get("end_minute")?.as_i64()?).ok()?;
            Some((start, end))
        })
        .collect();
    let hours = OpeningHours::new(
        value
            .get("time_zone_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        raw,
    );
    (!hours.is_empty()).then_some(hours)
}

fn parse_location(value: Option<&Value>) -> Option<BusinessLocation> {
    let value = value.filter(|v| !v.is_null())?;
    let address = value
        .get("address")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let point_e6 = value
        .get("location")
        .filter(|l| !l.is_null())
        .and_then(|l| {
            let lat = l.get("latitude")?.as_f64()?;
            let lon = l.get("longitude")?.as_f64()?;
            ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
                .then(|| ((lat * 1e6).round() as i32, (lon * 1e6).round() as i32))
        });
    (!address.is_empty() || point_e6.is_some()).then_some(BusinessLocation { address, point_e6 })
}

/// Parse `userFullInfo.business_info`; `None` when absent or empty.
pub fn parse_business_info(value: Option<&Value>) -> Option<BusinessInfo> {
    let value = value.filter(|v| !v.is_null())?;
    let int = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0)
    };
    let info = BusinessInfo {
        location: parse_location(value.get("location")),
        opening_hours: parse_hours(value.get("opening_hours")),
        local_opening_hours: parse_hours(value.get("local_opening_hours")),
        next_open_in: int("next_open_in"),
        next_close_in: int("next_close_in"),
    };
    (!info.is_empty()).then_some(info)
}

/// Anonymous Fragment numbers start with 888 (tdesktop `IsCollectiblePhone`,
/// the default of the `fragment_prefixes` app config).
pub fn is_fragment_number(phone: &str) -> bool {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    digits.starts_with("888")
}

/// Where the Fragment note links to.
pub const FRAGMENT_URL: &str = "https://fragment.com";

/// Minute of the week (Monday 0) of a local civil time.
pub fn week_minute(weekday: u8, hour: u8, minute: u8) -> u32 {
    u32::from(weekday.min(6)) * DAY + u32::from(hour) * 60 + u32::from(minute)
}

#[cfg(test)]
mod tests {
    use super::{
        BusinessInfo, DayHours, OpeningHours, format_day, is_fragment_number, opens_in_text,
        parse_business_info, week_minute,
    };

    const SAMPLE: &str = r#"{"@type":"businessInfo","location":{"@type":"businessLocation","location":{"@type":"location","latitude":32.0853,"longitude":34.7818,"horizontal_accuracy":0},"address":"1 Rothschild Blvd, Tel Aviv"},"opening_hours":{"@type":"businessOpeningHours","time_zone_id":"Asia/Jerusalem","opening_hours":[{"start_minute":540,"end_minute":1020},{"start_minute":1980,"end_minute":2460}]},"local_opening_hours":{"@type":"businessOpeningHours","time_zone_id":"","opening_hours":[{"start_minute":480,"end_minute":960},{"start_minute":1920,"end_minute":2400}]},"next_open_in":600,"next_close_in":0}"#;

    fn sample() -> BusinessInfo {
        let value: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        parse_business_info(Some(&value)).unwrap()
    }

    #[test]
    fn parses_location_and_both_schedules() {
        let info = sample();
        let location = info.location.as_ref().unwrap();
        assert_eq!(location.address, "1 Rothschild Blvd, Tel Aviv");
        assert_eq!(location.point_e6, Some((32_085_300, 34_781_800)));
        assert_eq!(
            location.map_url().unwrap(),
            "https://www.openstreetmap.org/?mlat=32.085300&mlon=34.781800"
        );
        assert_eq!(info.opening_hours.unwrap().time_zone_id, "Asia/Jerusalem");
    }

    #[test]
    fn distinct_local_hours_are_detected() {
        assert!(sample().has_distinct_local_hours());
    }

    #[test]
    fn empty_or_null_business_info_is_none() {
        let null = serde_json::Value::Null;
        assert!(parse_business_info(Some(&null)).is_none());
        assert!(parse_business_info(None).is_none());
        let empty: serde_json::Value =
            serde_json::from_str(r#"{"location":null,"opening_hours":null}"#).unwrap();
        assert!(parse_business_info(Some(&empty)).is_none());
    }

    #[test]
    fn open_and_closed_for_a_given_time() {
        let info = sample();
        // Monday 09:00 local (local hours 08:00-16:00 Monday).
        assert_eq!(info.minutes_until_open(week_minute(0, 9, 0)), Some(0));
        // Monday 16:30: next opening is Tuesday 08:00 (1920 = Tue 08:00).
        assert_eq!(
            info.minutes_until_open(week_minute(0, 16, 30)),
            Some(1920 - 990)
        );
        // Wrap around the week from Sunday 23:00 to Monday 08:00.
        assert_eq!(info.minutes_until_open(week_minute(6, 23, 0)), Some(9 * 60));
    }

    #[test]
    fn falls_back_to_the_snapshot_without_local_hours() {
        let mut info = sample();
        info.local_opening_hours = None;
        assert_eq!(info.minutes_until_open(0), Some(10));
        info.next_open_in = 3600;
        info.next_close_in = 600;
        assert_eq!(info.minutes_until_open(0), Some(0));
    }

    #[test]
    fn intervals_wrap_and_merge() {
        let hours = OpeningHours::new(
            String::new(),
            vec![(540, 1020), (1020, 1100), (10_000, 10_300)],
        );
        assert_eq!(
            hours.intervals,
            vec![(0, 220), (540, 1100), (10_000, 10_080)]
        );
        assert_eq!(format_day(&hours.days()[1]), "closed");
    }

    #[test]
    fn a_full_day_reads_open_24_hours() {
        let full = OpeningHours::new(String::new(), vec![(2 * 1440, 3 * 1440)]);
        assert_eq!(full.days()[2], DayHours::Full);
        assert_eq!(format_day(&full.days()[2]), "open 24 hours");
    }

    #[test]
    fn night_shift_shows_next_day() {
        // Monday 22:00 to Tuesday 02:00.
        let hours = OpeningHours::new(String::new(), vec![(1320, 1560)]);
        let days = hours.days();
        assert_eq!(format_day(&days[0]), "22:00 - 02:00 (next day)");
        assert_eq!(days[1], DayHours::Closed);
    }

    #[test]
    fn opens_in_wording() {
        assert_eq!(opens_in_text(0), "opens in 1 minute");
        assert_eq!(opens_in_text(45), "opens in 45 minutes");
        assert_eq!(opens_in_text(60), "opens in 1 hour");
        assert_eq!(opens_in_text(180), "opens in 3 hours");
        assert_eq!(opens_in_text(2 * 1440 + 5), "opens in 2 days");
    }

    #[test]
    fn detects_fragment_numbers() {
        assert!(is_fragment_number("88812345678"));
        assert!(is_fragment_number("+888 1234 5678"));
        assert!(!is_fragment_number("972502002287"));
        assert!(!is_fragment_number(""));
    }
}
