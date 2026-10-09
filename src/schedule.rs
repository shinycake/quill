//! Scheduling rules shared by the composer's schedule picker and the
//! scheduled-messages dialog. Mirrors tdesktop's `ChooseDateTimeBox`
//! (`ui/boxes/choose_date_time.cpp`) and `ScheduleBox`
//! (`history/view/history_view_schedule_box.cpp`): a send time must be at
//! least ten seconds ahead and at most a year away, the picker opens ten
//! minutes from now, and a chat with yourself says "reminder" instead of
//! "schedule".

use crate::local_time::{CivilTime, utc_offset_at};

/// `kMinimalSchedule` in `choose_date_time.cpp`.
pub const MIN_SCHEDULE_SECS: i64 = 10;
/// tdesktop allows one year ahead (`QDateTime::currentDateTime().addYears(1)`).
pub const MAX_SCHEDULE_SECS: i64 = 366 * 86_400;
/// `DefaultScheduleTime` in `history_view_schedule_box.cpp`.
pub const DEFAULT_SCHEDULE_SECS: i64 = 600;

/// The picker's initial send time.
pub fn default_schedule_time(now: i64) -> i64 {
    now + DEFAULT_SCHEDULE_SECS
}

/// Why a picked time cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleError {
    /// The date or time is missing or unreadable.
    Invalid,
    TooSoon,
    TooFar,
}

impl ScheduleError {
    pub fn message(self) -> &'static str {
        match self {
            ScheduleError::Invalid => "Pick a date and a time.",
            ScheduleError::TooSoon => "Pick a time in the future.",
            ScheduleError::TooFar => "Pick a time within the next year.",
        }
    }
}

/// `collect()` in `choose_date_time.cpp`: the time must lie within
/// `[now + 10s, now + 1 year]`.
pub fn validate_send_date(send_date: i64, now: i64) -> Result<i64, ScheduleError> {
    if send_date < now + MIN_SCHEDULE_SECS {
        Err(ScheduleError::TooSoon)
    } else if send_date > now + MAX_SCHEDULE_SECS {
        Err(ScheduleError::TooFar)
    } else {
        Ok(send_date)
    }
}

/// Unix time of a local wall-clock moment, given the zone as a function
/// from a unix instant to its UTC offset. Two fixed-point steps settle
/// every real zone (offsets differ by at most a couple of hours across a
/// transition); a moment inside a DST gap lands just after the gap.
pub fn civil_to_unix_with(civil: &CivilTime, offset_at: impl Fn(i64) -> i32) -> i64 {
    let naive =
        civil.day_number() * 86_400 + i64::from(civil.hour) * 3600 + i64::from(civil.minute) * 60;
    let mut guess = naive - i64::from(offset_at(naive));
    for _ in 0..2 {
        guess = naive - i64::from(offset_at(guess));
    }
    guess
}

/// [`civil_to_unix_with`] in the OS zone.
pub fn civil_to_unix(civil: &CivilTime) -> i64 {
    civil_to_unix_with(civil, utc_offset_at)
}

/// Parses the `%Y-%m-%d %H:%M` text the date picker formats.
pub fn parse_picker_stamp(text: &str) -> Option<CivilTime> {
    let (date, time) = text.split_once(' ')?;
    let mut d = date.split('-');
    let year = d.next()?.parse::<i64>().ok()?;
    let month = d.next()?.parse::<u8>().ok()?;
    let day = d.next()?.parse::<u8>().ok()?;
    let mut t = time.split(':');
    let hour = t.next()?.parse::<u8>().ok()?;
    let minute = t.next()?.parse::<u8>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    Some(CivilTime {
        year,
        month,
        day,
        hour,
        minute,
        weekday: 0,
    })
}

/// How a send time is worded: a chat with yourself sets reminders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleKind {
    Schedule,
    Reminder,
}

impl ScheduleKind {
    pub fn for_saved_messages(is_saved: bool) -> Self {
        if is_saved {
            ScheduleKind::Reminder
        } else {
            ScheduleKind::Schedule
        }
    }

    /// Send-menu entry (`lng_reminder_message` / `lng_schedule_message`).
    pub fn menu_label(self, already_set: bool) -> &'static str {
        match (self, already_set) {
            (ScheduleKind::Reminder, false) => "Set a reminder",
            (ScheduleKind::Reminder, true) => "Change reminder\u{2026}",
            (ScheduleKind::Schedule, false) => "Schedule message\u{2026}",
            (ScheduleKind::Schedule, true) => "Change schedule\u{2026}",
        }
    }

    /// Picker title (`lng_remind_title` / `lng_schedule_title`).
    pub fn picker_title(self) -> &'static str {
        match self {
            ScheduleKind::Reminder => "Remind me on\u{2026}",
            ScheduleKind::Schedule => "Send this message on\u{2026}",
        }
    }

    /// Picker confirm button.
    pub fn submit_label(self) -> &'static str {
        match self {
            ScheduleKind::Reminder => "Remind",
            ScheduleKind::Schedule => "Schedule",
        }
    }

    /// Title of the list dialog (`lng_scheduled_messages` /
    /// `lng_reminder_messages`).
    pub fn list_title(self) -> &'static str {
        match self {
            ScheduleKind::Reminder => "Reminders",
            ScheduleKind::Schedule => "Scheduled messages",
        }
    }

    pub fn empty_list(self) -> &'static str {
        match self {
            ScheduleKind::Reminder => "No reminders here yet.",
            ScheduleKind::Schedule => "No scheduled messages here yet.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_time::civil_at;

    const NOW: i64 = 1_800_000_000;

    #[test]
    fn default_time_is_ten_minutes_out() {
        assert_eq!(default_schedule_time(NOW), NOW + 600);
    }

    #[test]
    fn send_date_window_matches_tdesktop() {
        assert_eq!(
            validate_send_date(NOW + 9, NOW),
            Err(ScheduleError::TooSoon)
        );
        assert_eq!(validate_send_date(NOW + 10, NOW), Ok(NOW + 10));
        assert_eq!(
            validate_send_date(NOW + MAX_SCHEDULE_SECS, NOW),
            Ok(NOW + MAX_SCHEDULE_SECS)
        );
        assert_eq!(
            validate_send_date(NOW + MAX_SCHEDULE_SECS + 1, NOW),
            Err(ScheduleError::TooFar)
        );
    }

    #[test]
    fn civil_round_trips_in_fixed_zones() {
        for offset in [0, 3600 * 5 + 1800, -3600 * 8] {
            let civil = civil_at(NOW, offset);
            let unix = civil_to_unix_with(&civil, |_| offset);
            assert_eq!(unix, NOW - NOW % 60);
        }
    }

    #[test]
    fn civil_resolves_across_a_dst_switch() {
        // UTC+1 before `switch`, UTC+2 from it on.
        let switch = NOW;
        let zone = |t: i64| if t < switch { 3600 } else { 7200 };
        let early = switch - 7200;
        let before = civil_at(early, 3600);
        assert_eq!(civil_to_unix_with(&before, zone), early - early % 60);
        let late = switch + 7200;
        let after = civil_at(late, 7200);
        assert_eq!(civil_to_unix_with(&after, zone), late - late % 60);
    }

    #[test]
    fn picker_stamp_parses_and_rejects() {
        let civil = parse_picker_stamp("2026-10-09 14:05").unwrap();
        assert_eq!(
            (civil.year, civil.month, civil.day, civil.hour, civil.minute),
            (2026, 10, 9, 14, 5)
        );
        assert!(parse_picker_stamp("2026-13-09 14:05").is_none());
        assert!(parse_picker_stamp("2026-10-09 24:00").is_none());
        assert!(parse_picker_stamp("garbage").is_none());
    }

    #[test]
    fn saved_messages_say_reminder() {
        let saved = ScheduleKind::for_saved_messages(true);
        let other = ScheduleKind::for_saved_messages(false);
        assert_eq!(saved.menu_label(false), "Set a reminder");
        assert_eq!(saved.picker_title(), "Remind me on\u{2026}");
        assert_eq!(saved.list_title(), "Reminders");
        assert_eq!(other.menu_label(false), "Schedule message\u{2026}");
        assert_eq!(other.picker_title(), "Send this message on\u{2026}");
        assert_eq!(other.list_title(), "Scheduled messages");
    }
}
