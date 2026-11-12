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

/// A jump an answer resolved, for the window that asked for it
/// (`PendingRequest::window`) and the chat it was asked in. The driver
/// runs it in that window's view, and drops it when the window closed
/// or shows another chat by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewJump {
    pub window: Option<ChatId>,
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub mode: DateJumpMode,
}

impl ViewJump {
    /// The jump for the request `pending`, or `None` when it named no chat.
    pub(crate) fn for_request(
        pending: Option<&PendingRequest>,
        message_id: MessageId,
        mode: DateJumpMode,
    ) -> Option<Self> {
        let pending = pending?;
        Some(Self {
            window: pending.asked_in.chat(),
            chat_id: pending.chat_id?,
            message_id,
            mode,
        })
    }
}

/// Where the calendar's paging back through the history is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarPaging {
    /// Nothing in flight; the next older page may be asked for.
    Idle,
    /// A page is on its way as `request`; only its answer or error leaves
    /// the stage (the calendar replaced by a reopened box matches nothing).
    Loading { request: RequestId },
    /// The last page failed (a timeout, a flood wait, offline): moving to
    /// another month asks for it again.
    Failed,
    /// An empty page: the history has no older days (or the `All` filter,
    /// which cannot be fetched).
    Exhausted,
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
    pub paging: CalendarPaging,
    /// Set when the box was opened from the shared-media gallery: a picked
    /// day reloads that tab instead of jumping in the history.
    pub shared_tab: Option<SharedMediaTab>,
}

impl HistoryCalendar {
    pub fn new(chat_id: ChatId, media: SearchMediaKind, now: i64) -> Self {
        Self {
            chat_id,
            media,
            month: YearMonth::of(now),
            days: BTreeMap::new(),
            oldest_loaded: MessageId(0),
            paging: if media == SearchMediaKind::All {
                CalendarPaging::Exhausted
            } else {
                CalendarPaging::Idle
            },
            shared_tab: None,
        }
    }

    pub fn day(&self, day_number: i64) -> Option<&CalendarDay> {
        self.days.get(&day_number)
    }

    /// Whether the displayed month needs an older page: the data loaded so
    /// far does not reach back before the month begins.
    pub fn needs_older_page(&self) -> bool {
        if matches!(
            self.paging,
            CalendarPaging::Exhausted | CalendarPaging::Loading { .. }
        ) {
            return false;
        }
        match self.days.keys().next() {
            None => true,
            Some(oldest) => *oldest > self.month.first_day_number(),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.paging, CalendarPaging::Loading { .. })
    }

    /// A page request went out as `request`.
    pub fn begin_page(&mut self, request: RequestId) {
        self.paging = CalendarPaging::Loading { request };
    }

    /// Whether the page `request` is the one the calendar waits for.
    fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!(self.paging, CalendarPaging::Loading { request: waiting } if Some(waiting) == request)
    }

    /// Merge the page `request` answered; any other request's page (an
    /// older one, or one asked for a calendar since closed) is dropped.
    /// `days` come newest first; an empty page (or the same cursor again)
    /// ends the list.
    pub fn accept(&mut self, request: Option<RequestId>, days: Vec<CalendarDay>) {
        if !self.waits_for(request) {
            return;
        }
        self.paging = CalendarPaging::Idle;
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
            self.paging = CalendarPaging::Exhausted;
        }
    }

    /// The page `request` failed: the list stays as it is and a later ask
    /// retries. Another request's error changes nothing.
    pub fn fail(&mut self, request: Option<RequestId>) {
        if self.waits_for(request) {
            self.paging = CalendarPaging::Failed;
        }
    }

    /// The page could not be sent: a later ask retries.
    pub fn unsent(&mut self) {
        if !self.is_loading() {
            self.paging = CalendarPaging::Failed;
        }
    }

    pub fn show_month(&mut self, month: YearMonth) {
        self.month = month;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: RequestId = RequestId(7);

    fn day(id: i64, date: i32, count: i32) -> CalendarDay {
        CalendarDay {
            total_count: count,
            message_id: MessageId(id),
            date,
        }
    }

    fn calendar() -> HistoryCalendar {
        // 2026-10-08 12:00 UTC-ish; the exact zone does not matter below.
        HistoryCalendar::new(ChatId(1), SearchMediaKind::Media, 1_791_460_800)
    }

    #[test]
    fn all_filter_never_fetches() {
        let all = HistoryCalendar::new(ChatId(1), SearchMediaKind::All, 1_791_460_800);
        assert!(!all.needs_older_page());
    }

    #[test]
    fn pages_until_the_month_is_covered() {
        let mut c = calendar();
        assert!(c.needs_older_page());
        c.begin_page(R);
        // Early October: the loaded days do not reach back before the 1st.
        c.accept(Some(R), vec![day(500 << 20, 1_791_000_000, 3)]);
        assert!(c.needs_older_page());
        c.begin_page(R);
        // A day in August covers the displayed month.
        c.accept(Some(R), vec![day(100 << 20, 1_788_000_000, 1)]);
        assert_eq!(c.oldest_loaded, MessageId(100 << 20));
        assert_eq!(c.paging, CalendarPaging::Idle);
        assert!(!c.needs_older_page());
    }

    #[test]
    fn an_empty_page_ends_the_list() {
        let mut c = calendar();
        c.begin_page(R);
        c.accept(Some(R), vec![day(5 << 20, 1_791_000_000, 1)]);
        c.begin_page(R);
        c.accept(Some(R), Vec::new());
        assert_eq!(c.paging, CalendarPaging::Exhausted);
        assert!(!c.needs_older_page());
    }

    #[test]
    fn a_failed_page_is_asked_for_again() {
        let mut c = calendar();
        c.begin_page(R);
        assert!(!c.needs_older_page(), "one page at a time");
        c.fail(Some(R));
        assert_eq!(c.paging, CalendarPaging::Failed);
        assert!(c.needs_older_page(), "a failure does not end the paging");
        c.begin_page(R);
        c.accept(Some(R), vec![day(5 << 20, 1_788_000_000, 1)]);
        assert_eq!(c.paging, CalendarPaging::Idle);
    }

    #[test]
    fn partial_days_keep_the_earliest_message() {
        let mut c = calendar();
        c.begin_page(R);
        c.accept(Some(R), vec![day(50 << 20, 1_791_000_100, 1)]);
        c.begin_page(R);
        c.accept(Some(R), vec![day(40 << 20, 1_791_000_000, 2)]);
        let key = local_day_number(1_791_000_000);
        assert_eq!(c.day(key).map(|d| d.message_id), Some(MessageId(40 << 20)));
    }

    #[test]
    fn only_the_page_asked_for_lands() {
        let mut c = calendar();
        c.begin_page(R);
        c.accept(Some(RequestId(3)), vec![day(5 << 20, 1_791_000_000, 1)]);
        c.accept(None, vec![day(5 << 20, 1_791_000_000, 1)]);
        assert!(
            c.days.is_empty() && c.is_loading(),
            "other requests' pages are dropped"
        );
        c.fail(Some(RequestId(3)));
        assert!(
            c.is_loading(),
            "another request's error leaves the page waiting"
        );
        c.fail(Some(R));
        assert_eq!(c.paging, CalendarPaging::Failed);
    }
}
