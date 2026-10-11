//! What the tabs of the global search keep besides the hits: the paging
//! cursor of the open query, the Channels tab's recommendations and the
//! Posts tab's free-search quota.
use super::*;

/// `publicPostSearchLimits` (schema line 3135): the daily quota of the
/// Posts tab. tdesktop shows the free searches left on its intro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicPostLimits {
    pub daily_free: i32,
    pub remaining_free: i32,
    /// Seconds until a free search comes back; 0 = now.
    pub next_free_in: i32,
    pub star_count: i64,
    pub current_query_free: bool,
}

impl PublicPostLimits {
    /// The line under the Posts tab: the free searches left today.
    pub fn quota_line(&self) -> String {
        if self.remaining_free > 0 {
            let left = self.remaining_free;
            return format!(
                "{left} free search{} left today",
                if left == 1 { "" } else { "es" }
            );
        }
        if self.next_free_in > 0 {
            let minutes = (self.next_free_in + 59) / 60;
            return if minutes >= 60 {
                format!(
                    "No free searches left. The next one is in {} h",
                    minutes / 60
                )
            } else {
                format!("No free searches left. The next one is in {minutes} min")
            };
        }
        "No free searches left today".to_string()
    }
}

impl PublicPostLimits {
    /// "A free search unlocks in 2 h 5 min", `None` when one is available.
    pub fn unlock_line(&self) -> Option<String> {
        if self.next_free_in <= 0 {
            return None;
        }
        let minutes = (self.next_free_in + 59) / 60;
        let (hours, minutes) = (minutes / 60, minutes % 60);
        let span = match (hours, minutes) {
            (0, m) => format!("{m} min"),
            (h, 0) => format!("{h} h"),
            (h, m) => format!("{h} h {m} min"),
        };
        Some(format!("A free search unlocks in {span}"))
    }
}

/// Tab-specific state of [`SearchState`].
#[derive(Debug, Clone, Default)]
pub struct SearchTabsState {
    /// `getRecommendedChats`, shown under "Recommended channels".
    pub recommended: Vec<ChatId>,
    pub recommended_loading: bool,
    pub recommended_loaded: bool,
    /// `next_offset` of the newest page of the open query; empty = no more.
    pub next_offset: String,
    pub loading_more: bool,
    /// The last page failed; "Load more" says so and tries again.
    pub more_failed: bool,
    pub limits: Option<PublicPostLimits>,
}

impl SearchTabsState {
    /// A new query or filter starts a new list.
    pub(crate) fn reset_paging(&mut self) {
        self.next_offset.clear();
        self.loading_more = false;
        self.more_failed = false;
    }

    pub fn has_more(&self) -> bool {
        !self.next_offset.is_empty()
    }

    /// The offset of the page to ask for, once; `None` while one is in
    /// flight or the list has ended.
    pub(crate) fn begin_more(&mut self) -> Option<String> {
        if self.loading_more || !self.has_more() {
            return None;
        }
        self.loading_more = true;
        self.more_failed = false;
        Some(self.next_offset.clone())
    }
}

impl SearchState {
    /// The first page of a query: remember where the next one starts.
    pub(crate) fn set_first_page_offset(&mut self, next_offset: String) {
        self.tabs.reset_paging();
        self.tabs.next_offset = next_offset;
    }

    /// A further page: append the hits not already listed (TDLib may repeat
    /// the boundary message) and move the cursor.
    pub(crate) fn append_page(&mut self, hits: Vec<SearchMessageHit>, next_offset: String) {
        self.tabs.loading_more = false;
        self.tabs.more_failed = false;
        for hit in hits {
            let seen = self
                .messages
                .iter()
                .any(|known| known.chat_id == hit.chat_id && known.message_id == hit.message_id);
            if !seen {
                self.messages.push(hit);
            }
        }
        self.tabs.next_offset = next_offset;
    }

    /// The next page failed; the cursor stays so a retry asks again.
    pub(crate) fn fail_page(&mut self) {
        self.tabs.loading_more = false;
        self.tabs.more_failed = true;
    }

    /// `getRecommendedChats` answer.
    pub(crate) fn accept_recommended(&mut self, chat_ids: Vec<ChatId>) {
        self.tabs.recommended = chat_ids;
        self.tabs.recommended_loading = false;
        self.tabs.recommended_loaded = true;
    }
}

/// One row of a media tab: the hit labelled like a shared-media row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMediaRow {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub glyph: &'static str,
    pub label: String,
    /// Unix seconds; 0 when unknown.
    pub date: i32,
}

/// Where a new month starts in newest-first rows (rows without a date join
/// the section before them, like `month_sections`).
pub fn media_month_starts(
    rows: &[SearchMediaRow],
) -> Vec<(usize, crate::search_filters::YearMonth)> {
    let mut starts: Vec<(usize, crate::search_filters::YearMonth)> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if row.date <= 0 {
            continue;
        }
        let month = crate::search_filters::YearMonth::of(i64::from(row.date));
        if starts.last().map(|(_, last)| *last) != Some(month) {
            starts.push((index, month));
        }
    }
    starts
}

impl SearchState {
    /// The hits as rows of a media tab, in the order TDLib sent them.
    pub fn media_rows(&self, tab: SharedMediaTab) -> Vec<SearchMediaRow> {
        self.messages
            .iter()
            .map(|hit| SearchMediaRow {
                chat_id: hit.chat_id,
                message_id: hit.message_id,
                glyph: tab.glyph(),
                label: SharedMediaItem::label_for(tab, &hit.content),
                date: hit.date,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::ids::{ChatId, MessageId};
    use crate::state::{PublicPostLimits, SearchMessageHit, SearchState};
    use crate::telegram::envelope::MessageContent;

    fn hit(chat: i64, id: i64) -> SearchMessageHit {
        SearchMessageHit {
            sender: None,
            chat_id: ChatId(chat),
            message_id: MessageId(id),
            preview: String::new(),
            is_outgoing: false,
            content: MessageContent::Text("".into()),
            reply_to: None,
            forward_info: None,
            extras: Default::default(),
            interaction_info: None,
            is_pinned: false,
            media_album_id: 0,
            reply_markup: None,
            self_destruct: None,
            auto_delete: None,
            author_signature: None,
            date: 1,
        }
    }

    #[test]
    fn paging_appends_without_repeats_and_ends() {
        let mut search = SearchState {
            messages: vec![hit(1, 10), hit(1, 9)],
            ..Default::default()
        };
        search.set_first_page_offset("p2".into());
        assert_eq!(search.tabs.begin_more().as_deref(), Some("p2"));
        // One page in flight at a time.
        assert_eq!(search.tabs.begin_more(), None);
        search.append_page(vec![hit(1, 9), hit(2, 8)], String::new());
        let ids: Vec<_> = search.messages.iter().map(|m| m.message_id.0).collect();
        assert_eq!(ids, [10, 9, 8]);
        assert!(!search.tabs.has_more());
        assert_eq!(search.tabs.begin_more(), None);
    }

    #[test]
    fn a_failed_page_can_be_asked_again() {
        let mut search = SearchState::default();
        search.set_first_page_offset("p2".into());
        assert!(search.tabs.begin_more().is_some());
        search.fail_page();
        assert!(search.tabs.more_failed);
        assert_eq!(search.tabs.begin_more().as_deref(), Some("p2"));
        assert!(!search.tabs.more_failed);
    }

    #[test]
    fn a_new_query_drops_the_cursor() {
        let mut search = SearchState::default();
        search.set_first_page_offset("p2".into());
        search.begin_query("hello");
        assert!(!search.tabs.has_more());
    }

    #[test]
    fn media_rows_label_hits_like_the_gallery() {
        use crate::state::{SharedMediaTab, media_month_starts};
        let mut search = SearchState::default();
        let mut photo = hit(1, 3);
        photo.content = MessageContent::Text("hello".into());
        photo.date = 1_791_460_800;
        let mut older = hit(2, 2);
        older.content = MessageContent::Text("older".into());
        older.date = 1_700_000_000;
        let mut undated = hit(2, 1);
        undated.date = 0;
        search.messages = vec![photo, older, undated];
        let rows = search.media_rows(SharedMediaTab::Links);
        assert_eq!(rows[0].label, "hello");
        // A hit without text or URL falls back to its kind.
        assert_eq!(rows[2].label, "Link");
        assert_eq!(rows[0].glyph, SharedMediaTab::Links.glyph());
        let starts = media_month_starts(&rows);
        assert_eq!(starts.len(), 2);
        assert_eq!(starts[0].0, 0);
        assert_eq!(starts[1].0, 1);
    }

    #[test]
    fn unlock_line_counts_down() {
        let mut limits = PublicPostLimits {
            daily_free: 3,
            remaining_free: 0,
            next_free_in: 0,
            star_count: 10,
            current_query_free: false,
        };
        assert_eq!(limits.unlock_line(), None);
        limits.next_free_in = 3_000;
        assert_eq!(
            limits.unlock_line().as_deref(),
            Some("A free search unlocks in 50 min")
        );
        limits.next_free_in = 7_500;
        assert_eq!(
            limits.unlock_line().as_deref(),
            Some("A free search unlocks in 2 h 5 min")
        );
    }

    #[test]
    fn quota_line_reads_like_tdesktop() {
        let mut limits = PublicPostLimits {
            daily_free: 3,
            remaining_free: 2,
            next_free_in: 0,
            star_count: 10,
            current_query_free: true,
        };
        assert_eq!(limits.quota_line(), "2 free searches left today");
        limits.remaining_free = 1;
        assert_eq!(limits.quota_line(), "1 free search left today");
        limits.remaining_free = 0;
        assert_eq!(limits.quota_line(), "No free searches left today");
        limits.next_free_in = 90;
        assert_eq!(
            limits.quota_line(),
            "No free searches left. The next one is in 2 min"
        );
        limits.next_free_in = 7200;
        assert_eq!(
            limits.quota_line(),
            "No free searches left. The next one is in 2 h"
        );
    }
}
