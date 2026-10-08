//! "Jump to date": the calendar box state (tdesktop `Ui::CalendarBox` opened
//! from the date pill and the search bar) and the pending date jump.
use super::*;
use crate::search_filters::{SearchMediaKind, YearMonth, local_day_number};
use crate::telegram::envelope::CalendarDay;

/// The first message id TDLib can hand out (server id 1); jumping "to the
/// start" when a date precedes the chat's first message.
pub const FIRST_MESSAGE_ID: MessageId = MessageId(1 << 20);

/// Where a date jump lands relative to the message TDLib resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateJumpMode {
    /// The message itself (a calendar day's first message).
    Exact,
    /// The message after it: `getChatMessageByDate` answers with the last
    /// message before the day, the day's first message follows.
    Next,
    /// The oldest loaded message: the date precedes the whole chat.
    Oldest,
}

/// One open calendar box.
#[derive(Debug, Clone)]
pub struct HistoryCalendar {
    pub chat_id: ChatId,
    /// `All` cannot be fetched (`getChatMessageCalendar` rejects the empty
    /// filter), so only a media filter highlights days.
    pub media: SearchMediaKind,
    pub month: YearMonth,
    /// Local day number -> the first message of that day.
    pub days: BTreeMap<i64, CalendarDay>,
    /// `from_message_id` for the next older page.
    pub oldest_loaded: MessageId,
    pub exhausted: bool,
    pub loading: bool,
    pub generation: u64,
}

impl HistoryCalendar {
    pub fn new(chat_id: ChatId, media: SearchMediaKind, now: i64, generation: u64) -> Self {
        Self {
            chat_id,
            media,
            month: YearMonth::of(now),
            days: BTreeMap::new(),
            oldest_loaded: MessageId(0),
            exhausted: media == SearchMediaKind::All,
            loading: false,
            generation,
        }
    }

    pub fn day(&self, day_number: i64) -> Option<&CalendarDay> {
        self.days.get(&day_number)
    }

    /// Whether the displayed month needs an older page: the data loaded so
    /// far does not reach back before the month begins.
    pub fn needs_older_page(&self) -> bool {
        if self.exhausted || self.loading {
            return false;
        }
        match self.days.keys().next() {
            None => true,
            Some(oldest) => *oldest > self.month.first_day_number(),
        }
    }

    /// Merge one page. `days` come newest first; an empty page (or the
    /// same cursor again) ends the list.
    pub fn accept(&mut self, days: Vec<CalendarDay>) {
        self.loading = false;
        let before = self.oldest_loaded;
        let mut oldest = self.oldest_loaded;
        for day in days {
            let key = local_day_number(i64::from(day.date));
            if oldest.0 == 0 || day.message_id.0 < oldest.0 {
                oldest = day.message_id;
            }
            // The page's oldest day may be partial: keep the earliest
            // message seen for a day.
            match self.days.get(&key) {
                Some(existing) if existing.message_id.0 <= day.message_id.0 => {}
                _ => {
                    self.days.insert(key, day);
                }
            }
        }
        self.oldest_loaded = oldest;
        if oldest == before {
            self.exhausted = true;
        }
    }

    pub fn fail(&mut self) {
        self.loading = false;
        self.exhausted = true;
    }

    pub fn show_month(&mut self, month: YearMonth) {
        self.month = month;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(id: i64, date: i32, count: i32) -> CalendarDay {
        CalendarDay {
            total_count: count,
            message_id: MessageId(id),
            date,
        }
    }

    fn calendar() -> HistoryCalendar {
        // 2026-10-08 12:00 UTC-ish; the exact zone does not matter below.
        HistoryCalendar::new(ChatId(1), SearchMediaKind::Media, 1_791_460_800, 1)
    }

    #[test]
    fn all_filter_never_fetches() {
        let all = HistoryCalendar::new(ChatId(1), SearchMediaKind::All, 1_791_460_800, 1);
        assert!(!all.needs_older_page());
    }

    #[test]
    fn pages_until_the_month_is_covered() {
        let mut c = calendar();
        assert!(c.needs_older_page());
        c.loading = true;
        // Early October: the loaded days do not reach back before the 1st.
        c.accept(vec![day(500 << 20, 1_791_000_000, 3)]);
        assert!(c.needs_older_page());
        c.loading = true;
        // A day in August covers the displayed month.
        c.accept(vec![day(100 << 20, 1_788_000_000, 1)]);
        assert_eq!(c.oldest_loaded, MessageId(100 << 20));
        assert!(!c.exhausted);
        assert!(!c.needs_older_page());
    }

    #[test]
    fn an_empty_page_ends_the_list() {
        let mut c = calendar();
        c.loading = true;
        c.accept(vec![day(5 << 20, 1_791_000_000, 1)]);
        c.loading = true;
        c.accept(Vec::new());
        assert!(c.exhausted);
        assert!(!c.needs_older_page());
    }

    #[test]
    fn partial_days_keep_the_earliest_message() {
        let mut c = calendar();
        c.accept(vec![day(50 << 20, 1_791_000_100, 1)]);
        c.accept(vec![day(40 << 20, 1_791_000_000, 2)]);
        let key = local_day_number(1_791_000_000);
        assert_eq!(c.day(key).map(|d| d.message_id), Some(MessageId(40 << 20)));
    }
}
