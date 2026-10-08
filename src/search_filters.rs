//! Search filters and the jump-to-date calendar math (pure, no UI).
//!
//! Telegram Desktop's global search narrows by chat type (`lng_search_filter_*`:
//! All / Private / Groups / Channels); Telegram's mobile clients add the
//! media tabs (Media / Links / Files / Music / Voice) and a date window.
//! TDLib expresses all three on `searchMessages` / `searchChatMessages`
//! (`chat_type_filter`, `filter`, `min_date`). The calendar behind "Jump to
//! date" is a Monday-first month grid computed from civil dates.

use crate::local_time::{CivilTime, civil_local, utc_offset_at};

/// `searchMessagesChatTypeFilter*` (schema 1.8.67, lines 6335-6341) — the
/// chat-type filter of global message search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchChatType {
    #[default]
    All,
    Private,
    Groups,
    Channels,
}

impl SearchChatType {
    pub const ALL: [Self; 4] = [Self::All, Self::Private, Self::Groups, Self::Channels];

    /// tdesktop `lng_search_filter_*`.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All chats",
            Self::Private => "Private chats",
            Self::Groups => "Group chats",
            Self::Channels => "Channels",
        }
    }

    /// The `SearchMessagesChatTypeFilter` constructor; `None` = all chats.
    pub fn constructor(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Private => Some("searchMessagesChatTypeFilterPrivate"),
            Self::Groups => Some("searchMessagesChatTypeFilterGroup"),
            Self::Channels => Some("searchMessagesChatTypeFilterChannel"),
        }
    }
}

/// The media tabs of a search (`SearchMessagesFilter`): Media (photos and
/// videos), Links, Files, Music, Voice (voice and video notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMediaKind {
    #[default]
    All,
    Media,
    Links,
    Files,
    Music,
    Voice,
}

impl SearchMediaKind {
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Media,
        Self::Links,
        Self::Files,
        Self::Music,
        Self::Voice,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Media => "Media",
            Self::Links => "Links",
            Self::Files => "Files",
            Self::Music => "Music",
            Self::Voice => "Voice",
        }
    }

    /// `searchMessagesFilter*` constructor (schema lines 6299-6311); `None`
    /// for "All" (the request carries `filter: null`).
    pub fn constructor(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Media => Some("searchMessagesFilterPhotoAndVideo"),
            Self::Links => Some("searchMessagesFilterUrl"),
            Self::Files => Some("searchMessagesFilterDocument"),
            Self::Music => Some("searchMessagesFilterAudio"),
            Self::Voice => Some("searchMessagesFilterVoiceAndVideoNote"),
        }
    }
}

/// The date window of global search (`min_date`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchDateRange {
    #[default]
    Any,
    Week,
    Month,
    Year,
}

impl SearchDateRange {
    pub const ALL: [Self; 4] = [Self::Any, Self::Week, Self::Month, Self::Year];

    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "Any time",
            Self::Week => "Past week",
            Self::Month => "Past month",
            Self::Year => "Past year",
        }
    }

    /// `min_date` for `searchMessages` at `now` (0 = no lower bound).
    pub fn min_date(self, now: i64) -> i32 {
        let days = match self {
            Self::Any => return 0,
            Self::Week => 7,
            Self::Month => 30,
            Self::Year => 365,
        };
        (now - days * 86_400).clamp(0, i64::from(i32::MAX)) as i32
    }
}

/// Everything the global-search filter bar can narrow by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GlobalSearchFilters {
    pub chat_type: SearchChatType,
    pub media: SearchMediaKind,
    pub date: SearchDateRange,
}

impl GlobalSearchFilters {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// A displayed month of the calendar box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct YearMonth {
    pub year: i64,
    /// 1-12.
    pub month: u8,
}

impl YearMonth {
    pub fn of(unix: i64) -> Self {
        let civil = civil_local(unix);
        Self {
            year: civil.year,
            month: civil.month,
        }
    }

    pub fn prev(self) -> Self {
        if self.month == 1 {
            Self {
                year: self.year - 1,
                month: 12,
            }
        } else {
            Self {
                month: self.month - 1,
                ..self
            }
        }
    }

    pub fn next(self) -> Self {
        if self.month == 12 {
            Self {
                year: self.year + 1,
                month: 1,
            }
        } else {
            Self {
                month: self.month + 1,
                ..self
            }
        }
    }

    /// "October 2026" (tdesktop's calendar caption).
    pub fn title(self) -> String {
        format!(
            "{} {}",
            crate::local_time::month_name(self.month),
            self.year
        )
    }

    /// Days since 1970-01-01 of the first of this month.
    pub fn first_day_number(self) -> i64 {
        day_number(self.year, self.month, 1)
    }

    /// Monday-first weeks; `None` pads the days outside the month.
    pub fn grid(self) -> Vec<[Option<u8>; 7]> {
        let lead = (self.first_day_number() + 3).rem_euclid(7) as usize;
        let count = usize::from(days_in_month(self.year, self.month));
        let mut weeks = Vec::new();
        let mut cell = 0usize;
        while cell < lead + count {
            let mut week = [None; 7];
            for slot in &mut week {
                if cell >= lead && cell < lead + count {
                    *slot = Some((cell - lead + 1) as u8);
                }
                cell += 1;
            }
            weeks.push(week);
        }
        weeks
    }
}

/// Days since 1970-01-01 of a civil date.
pub fn day_number(year: i64, month: u8, day: u8) -> i64 {
    CivilTime {
        year,
        month,
        day,
        hour: 0,
        minute: 0,
        weekday: 0,
    }
    .day_number()
}

/// Unix time of local midnight starting `day_number` (DST-aware: the offset
/// is looked up at the resulting instant).
pub fn local_midnight(day_number: i64) -> i64 {
    let utc_guess = day_number * 86_400;
    let first = utc_guess - i64::from(utc_offset_at(utc_guess));
    utc_guess - i64::from(utc_offset_at(first))
}

/// The local day number of a unix timestamp.
pub fn local_day_number(unix: i64) -> i64 {
    civil_local(unix).day_number()
}

/// The `getChatMessageByDate` argument that makes "the last message no
/// later than `date`" the one just before `day_number` begins; the message
/// after it is the first of the day.
pub fn before_day_date(day_number: i64) -> i32 {
    (local_midnight(day_number) - 1).clamp(0, i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_monday_first_and_covers_the_month() {
        // October 2026 starts on a Thursday and has 31 days.
        let grid = YearMonth {
            year: 2026,
            month: 10,
        }
        .grid();
        assert_eq!(grid[0][..3], [None, None, None]);
        assert_eq!(grid[0][3], Some(1));
        assert_eq!(grid.iter().flatten().flatten().count(), 31);
        assert_eq!(grid.len(), 5);
    }

    #[test]
    fn february_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2100, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
    }

    #[test]
    fn month_navigation_wraps_the_year() {
        let jan = YearMonth {
            year: 2026,
            month: 1,
        };
        assert_eq!(
            jan.prev(),
            YearMonth {
                year: 2025,
                month: 12
            }
        );
        assert_eq!(jan.prev().next(), jan);
        assert_eq!(jan.title(), "January 2026");
    }

    #[test]
    fn min_date_windows() {
        assert_eq!(SearchDateRange::Any.min_date(1_000_000), 0);
        assert_eq!(
            SearchDateRange::Week.min_date(1_000_000),
            1_000_000 - 604_800
        );
        assert_eq!(SearchDateRange::Year.min_date(100), 0);
    }

    #[test]
    fn media_kinds_map_to_td_filters() {
        assert_eq!(SearchMediaKind::All.constructor(), None);
        assert_eq!(
            SearchMediaKind::Voice.constructor(),
            Some("searchMessagesFilterVoiceAndVideoNote")
        );
        assert_eq!(
            SearchChatType::Channels.constructor(),
            Some("searchMessagesChatTypeFilterChannel")
        );
    }

    #[test]
    fn midnight_is_a_day_boundary() {
        let dn = day_number(2026, 3, 14);
        let midnight = local_midnight(dn);
        assert_eq!(local_day_number(midnight), dn);
        assert_eq!(local_day_number(midnight - 1), dn - 1);
        assert_eq!(local_day_number(before_day_date(dn).into()), dn - 1);
    }
}
