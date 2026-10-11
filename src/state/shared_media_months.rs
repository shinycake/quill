//! Shared media by month: the month headers of the gallery list and the
//! jump-to-date window (tdesktop `Info::Media::ListWidget` month sections
//! and the calendar's "jump to date").
use super::*;
use crate::search_filters::{SearchMediaKind, YearMonth, local_day_number};
use crate::telegram::envelope::CalendarDay;

/// A run of gallery items sent in the same local month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthSection {
    pub month: YearMonth,
    /// Index of the section's first item.
    pub start: usize,
    /// One past the section's last item.
    pub end: usize,
}

/// Groups newest-first items into month sections. Items without a date
/// (`0`) join the section before them so no header reads "January 1970".
pub fn month_sections(items: &[SharedMediaItem]) -> Vec<MonthSection> {
    let mut sections: Vec<MonthSection> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let month = (item.date > 0).then(|| YearMonth::of(i64::from(item.date)));
        match (sections.last_mut(), month) {
            (Some(last), Some(month)) if last.month == month => last.end = index + 1,
            (Some(last), None) => last.end = index + 1,
            (_, Some(month)) => sections.push(MonthSection {
                month,
                start: index,
                end: index + 1,
            }),
            (None, None) => {}
        }
    }
    sections
}

/// The calendar filter that matches a gallery tab; GIFs have none.
pub fn calendar_media(tab: SharedMediaTab) -> Option<SearchMediaKind> {
    match tab {
        SharedMediaTab::Media => Some(SearchMediaKind::Media),
        SharedMediaTab::Files => Some(SearchMediaKind::Files),
        SharedMediaTab::Music => Some(SearchMediaKind::Music),
        SharedMediaTab::Links => Some(SearchMediaKind::Links),
        SharedMediaTab::Voice => Some(SearchMediaKind::Voice),
        SharedMediaTab::Gifs => None,
    }
}

/// Newest-first `items` cut so the list starts at `day_number`: items sent
/// after that local day are dropped. Returns the input when that would
/// leave nothing.
pub fn trim_to_day(items: Vec<SharedMediaItem>, day_number: i64) -> Vec<SharedMediaItem> {
    let skip = items
        .iter()
        .take_while(|item| item.date > 0 && local_day_number(i64::from(item.date)) > day_number)
        .count();
    if skip == items.len() {
        return items;
    }
    items.into_iter().skip(skip).collect()
}

/// Negative `searchChatMessages` offset that also returns the newer
/// messages of a calendar day (`day_total` of them) above the day's first
/// message; `limit` must stay above `-offset`.
pub fn day_window_offset(day_total: i32, limit: i32) -> i32 {
    -day_total.clamp(0, limit.saturating_sub(1).max(0))
}

/// The calendar day a pick lands on: the day itself, else the nearest
/// earlier day with media, else the nearest later one.
pub fn nearest_media_day(
    days: &BTreeMap<i64, CalendarDay>,
    picked: i64,
) -> Option<(i64, &CalendarDay)> {
    days.range(..=picked)
        .next_back()
        .or_else(|| days.range(picked..).next())
        .map(|(day, entry)| (*day, entry))
}

impl Session {
    /// Open the calendar box for the gallery's active tab. `None` when the
    /// gallery is closed or the tab has no calendar (GIFs).
    pub fn open_shared_media_calendar(&mut self, now: i64) -> Option<&HistoryCalendar> {
        let gallery = &self.media.shared_media;
        let (chat_id, tab) = (
            gallery.chat_id.filter(|_| gallery.open)?,
            gallery.active_tab,
        );
        let media = calendar_media(tab)?;
        let generation = self
            .search
            .history_calendar
            .as_ref()
            .map_or(1, |c| c.generation.saturating_add(1));
        self.search.date_jump_note = None;
        let mut calendar = HistoryCalendar::new(chat_id, media, now, generation);
        calendar.shared_tab = Some(tab);
        self.search.history_calendar = Some(calendar);
        self.search.history_calendar.as_ref()
    }
}

impl SharedMediaState {
    /// Like [`Self::begin_fetch`], for a page that starts at `day_number`.
    pub fn begin_fetch_at_day(&mut self, tab: SharedMediaTab, day_number: i64) -> u64 {
        let generation = self.begin_fetch(tab);
        self.tab_state(tab).anchor_day = Some(day_number);
        generation
    }

    /// Whether the tab shows a window from a picked date instead of the
    /// newest media.
    pub fn is_at_date(&self, tab: SharedMediaTab) -> bool {
        self.tabs[tab.index()].anchor_day.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_filters::day_number;
    use std::collections::BTreeMap;

    fn item(id: i64, date: i32) -> SharedMediaItem {
        SharedMediaItem {
            message_id: MessageId(id),
            glyph: "",
            label: String::new(),
            message: None,
            date,
        }
    }

    fn noon(year: i64, month: u8, day: u8) -> i32 {
        (day_number(year, month, day) * 86_400 + 43_200) as i32
    }

    #[test]
    fn groups_by_month_newest_first() {
        let items = vec![
            item(5, noon(2026, 10, 9)),
            item(4, noon(2026, 10, 2)),
            item(3, noon(2026, 9, 30)),
            item(2, 0),
            item(1, noon(2025, 12, 1)),
        ];
        let sections = month_sections(&items);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].month.title(), "October 2026");
        assert_eq!((sections[0].start, sections[0].end), (0, 2));
        assert_eq!((sections[1].start, sections[1].end), (2, 4));
        assert_eq!(sections[2].month.title(), "December 2025");
        assert_eq!((sections[2].start, sections[2].end), (4, 5));
        assert!(month_sections(&[]).is_empty());
    }

    #[test]
    fn a_leading_undated_item_gets_no_section_until_a_dated_one() {
        let items = vec![item(2, 0), item(1, noon(2026, 1, 5))];
        let sections = month_sections(&items);
        assert_eq!(sections.len(), 1);
        assert_eq!((sections[0].start, sections[0].end), (1, 2));
    }

    #[test]
    fn trims_items_newer_than_the_picked_day() {
        let items = vec![
            item(4, noon(2026, 10, 10)),
            item(3, noon(2026, 10, 9)),
            item(2, noon(2026, 10, 9)),
            item(1, noon(2026, 10, 8)),
        ];
        let trimmed = trim_to_day(items, local_day_number(i64::from(noon(2026, 10, 9))));
        let ids: Vec<i64> = trimmed.iter().map(|i| i.message_id.0).collect();
        assert_eq!(ids, vec![3, 2, 1]);
    }

    #[test]
    fn trimming_never_empties_the_list() {
        let items = vec![item(2, noon(2026, 10, 10)), item(1, noon(2026, 10, 9))];
        let day = local_day_number(i64::from(noon(2026, 1, 1)));
        assert_eq!(trim_to_day(items, day).len(), 2);
    }

    #[test]
    fn window_offset_stays_inside_the_page() {
        assert_eq!(day_window_offset(3, 50), -3);
        assert_eq!(day_window_offset(500, 50), -49);
        assert_eq!(day_window_offset(0, 50), 0);
    }

    fn calendar_day(id: i64) -> CalendarDay {
        CalendarDay {
            total_count: 1,
            message_id: MessageId(id),
            date: 0,
        }
    }

    #[test]
    fn a_picked_day_lands_on_the_nearest_day_with_media() {
        let mut days = BTreeMap::new();
        days.insert(100, calendar_day(1));
        days.insert(110, calendar_day(2));
        assert_eq!(nearest_media_day(&days, 110).map(|d| d.0), Some(110));
        assert_eq!(nearest_media_day(&days, 105).map(|d| d.0), Some(100));
        // Before the first media day: the oldest one after it.
        assert_eq!(nearest_media_day(&days, 50).map(|d| d.0), Some(100));
        assert_eq!(nearest_media_day(&days, 200).map(|d| d.0), Some(110));
        assert!(nearest_media_day(&BTreeMap::new(), 1).is_none());
    }

    #[test]
    fn gifs_have_no_calendar() {
        assert_eq!(calendar_media(SharedMediaTab::Gifs), None);
        assert_eq!(
            calendar_media(SharedMediaTab::Media),
            Some(SearchMediaKind::Media)
        );
    }

    #[test]
    fn jump_marks_the_tab_and_a_plain_fetch_clears_it() {
        let mut state = SharedMediaState::default();
        state.open_for(ChatId(1));
        state.begin_fetch_at_day(SharedMediaTab::Media, 20_000);
        assert!(state.is_at_date(SharedMediaTab::Media));
        state.begin_fetch(SharedMediaTab::Media);
        assert!(!state.is_at_date(SharedMediaTab::Media));
    }
}
