//! Search inside the gallery's Files, Music and Links tabs (tdesktop
//! `info/media`: `SharedMediaAllowSearch`, `kMediaCountForSearch`, and the
//! `lng_media_*_empty_search` empty states). A tab's `query` is the text its
//! items answer: the list and the empty state both render from it.
use super::*;

/// tdesktop `kMediaCountForSearch`: the tab needs more than this many
/// items before the header offers search.
pub const MEDIA_COUNT_FOR_SEARCH: i32 = 10;

impl SharedMediaTab {
    /// tdesktop `SharedMediaAllowSearch` (music, files and links; the
    /// Voice tab has no text to match).
    pub fn searchable(self) -> bool {
        matches!(
            self,
            SharedMediaTab::Files | SharedMediaTab::Music | SharedMediaTab::Links
        )
    }
}

impl SharedMediaState {
    /// The text the open tab's items answer.
    pub fn active_query(&self) -> &str {
        self.tab_at(self.active_tab)
            .map_or("", |tab| tab.query.as_str())
    }

    /// Ask `tab` for `query` (already trimmed). `false` when the tab does
    /// not search or already answers that query; otherwise the caller
    /// sends the first page, which replaces the items.
    pub fn begin_search(&mut self, tab: SharedMediaTab, query: &str) -> bool {
        if !tab.searchable() || !self.open {
            return false;
        }
        let state = self.tab_state(tab);
        if state.query == query {
            return false;
        }
        state.query = query.to_string();
        true
    }

    /// Select `tab`. A search on the tab being left is dropped with its
    /// results (tdesktop keeps the query per section; Quill starts every
    /// visit with the plain list), and answers still on their way for it
    /// are discarded. Other tabs' requests are untouched.
    pub fn select_tab_dropping_search(&mut self, tab: SharedMediaTab) -> bool {
        let leaving = self.active_tab;
        if leaving != tab {
            let had_query = self
                .tabs
                .get(leaving.index())
                .is_some_and(|state| !state.query.is_empty());
            if had_query {
                self.discard_search(leaving);
            }
        }
        self.select_tab(tab)
    }

    fn discard_search(&mut self, tab: SharedMediaTab) {
        // A late answer for the dropped search is told apart by a fresh stamp.
        self.generation = self.generation.saturating_add(1);
        let stamp = self.generation;
        let state = self.tab_state(tab);
        state.clear();
        state.generation = stamp;
    }
}

impl Session {
    /// Whether the open gallery shows its search field for `tab`: the tab
    /// searches, and either holds a query or has more than
    /// [`MEDIA_COUNT_FOR_SEARCH`] items (tdesktop
    /// `setSearchEnabledByContent`).
    pub fn shared_media_search_shown(&self, tab: SharedMediaTab) -> bool {
        if !tab.searchable() {
            return false;
        }
        let gallery = &self.media.shared_media;
        if gallery
            .tab_at(tab)
            .is_some_and(|state| !state.query.is_empty())
        {
            return true;
        }
        let Some(counts) = gallery
            .chat_id
            .and_then(|id| self.media.chat_media_counts.get(&id.0))
        else {
            return false;
        };
        let total: i32 = tab
            .count_keys()
            .iter()
            .filter_map(|key| counts.get(key))
            .sum();
        total > MEDIA_COUNT_FOR_SEARCH
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_gallery() -> SharedMediaState {
        let mut gallery = SharedMediaState::default();
        gallery.open_for_tab(ChatId(5), SharedMediaTab::Files);
        gallery
    }

    fn files_state(gallery: &SharedMediaState) -> &SharedMediaTabState {
        gallery.tab_at(SharedMediaTab::Files).expect("files slot")
    }

    #[test]
    fn only_files_music_and_links_search() {
        for tab in SharedMediaTab::ALL {
            let expected = matches!(
                tab,
                SharedMediaTab::Files | SharedMediaTab::Music | SharedMediaTab::Links
            );
            assert_eq!(tab.searchable(), expected, "{tab:?}");
        }
    }

    #[test]
    fn begin_search_reports_changes_only() {
        let mut gallery = open_gallery();
        assert!(gallery.begin_search(SharedMediaTab::Files, "report"));
        assert_eq!(gallery.active_query(), "report");
        assert!(!gallery.begin_search(SharedMediaTab::Files, "report"));
        assert!(gallery.begin_search(SharedMediaTab::Files, ""));
        assert!(!gallery.begin_search(SharedMediaTab::Voice, "x"));
        assert!(!gallery.begin_search(SharedMediaTab::Media, "x"));
    }

    #[test]
    fn leaving_the_tab_clears_the_query_and_its_late_answer() {
        let mut gallery = open_gallery();
        assert!(gallery.begin_search(SharedMediaTab::Files, "report"));
        let generation = gallery.begin_fetch(SharedMediaTab::Files);
        assert!(gallery.select_tab_dropping_search(SharedMediaTab::Links));
        assert_eq!(gallery.active_tab, SharedMediaTab::Links);
        assert_eq!(files_state(&gallery).query, "");
        assert_eq!(files_state(&gallery).status, SharedMediaTabStatus::Idle);
        gallery.accept(
            ChatId(5),
            SharedMediaTab::Files,
            generation,
            Vec::new(),
            0,
            MessageId(0),
        );
        assert_eq!(files_state(&gallery).status, SharedMediaTabStatus::Idle);
        // Coming back asks for the plain list again.
        assert!(gallery.select_tab_dropping_search(SharedMediaTab::Files));
    }

    #[test]
    fn staying_on_the_tab_keeps_the_query() {
        let mut gallery = open_gallery();
        assert!(gallery.begin_search(SharedMediaTab::Files, "report"));
        gallery.select_tab_dropping_search(SharedMediaTab::Files);
        assert_eq!(gallery.active_query(), "report");
    }

    #[test]
    fn closing_the_gallery_drops_every_query() {
        let mut gallery = open_gallery();
        gallery.begin_search(SharedMediaTab::Files, "report");
        gallery.close();
        assert!(gallery.tabs.iter().all(|tab| tab.query.is_empty()));
    }
}
