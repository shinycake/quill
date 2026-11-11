//! Sticker and GIF panel state types.
use super::*;
use crate::state::fetch::Fetch;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StickerTab {
    #[default]
    Installed,
    Recent,
    Favorites,
    Trending,
    Search,
    Archived,
}

/// What the session's found stickers were asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StickerSearch {
    /// A typed query (`searchStickers` by keyword); empty before any.
    Typed(String),
    /// A category of the sticker strip (`searchStickers` for its emoji,
    /// `getPremiumStickers`).
    Category(crate::telegram::EmojiCategorySource),
}

impl Default for StickerSearch {
    fn default() -> Self {
        Self::Typed(String::new())
    }
}

impl StickerSearch {
    /// The typed query; empty for a category.
    pub fn typed(&self) -> &str {
        match self {
            Self::Typed(query) => query,
            Self::Category(_) => "",
        }
    }
}

/// The selected set's stickers. A failed set is not asked again until
/// another set is selected or the panel opens again, or the per-update pass
/// would resend it at round-trip speed. An answer applies only to the
/// request that asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StickerSetStage {
    #[default]
    Idle,
    Loading {
        set_id: i64,
        request: RequestId,
    },
    Ready {
        set_id: i64,
    },
    Failed {
        set_id: i64,
    },
}

impl StickerSetStage {
    /// The set this stage is about.
    pub fn set_id(&self) -> Option<i64> {
        match *self {
            Self::Idle => None,
            Self::Loading { set_id, .. } | Self::Ready { set_id } | Self::Failed { set_id } => {
                Some(set_id)
            }
        }
    }

    /// The set whose stickers are in the panel's `stickers`.
    pub fn loaded_id(&self) -> Option<i64> {
        match *self {
            Self::Ready { set_id } => Some(set_id),
            _ => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// One fetch per list tab of the sticker panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerLists {
    pub installed: Fetch<()>,
    pub recent: Fetch<()>,
    pub favorites: Fetch<()>,
    pub trending: Fetch<()>,
    pub archived: Fetch<()>,
    /// The search tab asks two things at once: the matching sets
    /// (`searchStickerSets`) and the matching stickers (`searchStickers`).
    /// Each answer ends only its own half.
    pub search: Fetch<()>,
    pub search_stickers: Fetch<()>,
}

impl StickerLists {
    /// The fetches a tab shows: two for the search tab, one for the rest.
    fn of(&self, tab: StickerTab) -> [&Fetch<()>; 2] {
        match tab {
            StickerTab::Installed => [&self.installed; 2],
            StickerTab::Recent => [&self.recent; 2],
            StickerTab::Favorites => [&self.favorites; 2],
            StickerTab::Trending => [&self.trending; 2],
            StickerTab::Archived => [&self.archived; 2],
            StickerTab::Search => [&self.search, &self.search_stickers],
        }
    }

    /// Whether the tab is still loading: the search tab while either of
    /// its requests is out, so the first answer does not read as "No
    /// stickers found" before the other arrives.
    pub fn tab_loading(&self, tab: StickerTab) -> bool {
        self.of(tab).iter().any(|fetch| fetch.is_loading())
    }

    /// The tab's failure, if any of its fetches failed.
    pub fn tab_error(&self, tab: StickerTab) -> Option<&str> {
        self.of(tab).into_iter().find_map(|fetch| fetch.error())
    }

    /// A new search: both halves start over.
    pub fn reset_search(&mut self) {
        for fetch in [&mut self.search, &mut self.search_stickers] {
            fetch.cancel();
            fetch.clear_error();
        }
    }

    /// The list a request loads; `None` for the requests that change
    /// something (those report a note when they fail).
    pub fn for_purpose(&mut self, purpose: RequestPurpose) -> Option<&mut Fetch<()>> {
        match purpose {
            RequestPurpose::GetInstalledStickerSets => Some(&mut self.installed),
            RequestPurpose::GetRecentStickers => Some(&mut self.recent),
            RequestPurpose::GetFavoriteStickers => Some(&mut self.favorites),
            RequestPurpose::GetTrendingStickerSets => Some(&mut self.trending),
            RequestPurpose::GetArchivedStickerSets => Some(&mut self.archived),
            RequestPurpose::SearchStickerSets => Some(&mut self.search),
            RequestPurpose::SearchStickers => Some(&mut self.search_stickers),
            _ => None,
        }
    }
}

/// Composer sticker panel (Unigram `StickerDrawerViewModel` installed regular sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerPanel {
    pub open: bool,
    pub tab: StickerTab,
    pub sets: Vec<StickerSetInfo>,
    pub archived: Vec<StickerSetInfo>,
    pub archived_offset: i64,
    pub archived_next_offset: i64,
    pub archived_has_more: bool,
    pub selected_set_id: Option<i64>,
    pub stickers: Vec<StickerItem>,
    /// The selected set's `getStickerSet`: asked, answered or failed.
    pub set: StickerSetStage,
    pub installed_loaded: bool,
    /// Each list tab's own fetch: a failure of one list shows on that tab
    /// only, and a failed mutation is a note, not a list failure.
    pub lists: StickerLists,
    pub batch_total: usize,
    pub batch_completed: usize,
    pub batch_failed: usize,
    pub batch_pending: Vec<i64>,
    /// Slice S8: trending sets (`getTrendingStickerSets`) + premium-row flag.
    pub trending: Vec<StickerSetInfo>,
    pub trending_total: usize,
    pub trending_offset: usize,
    pub trending_next_offset: usize,
    pub trending_is_premium: bool,
    /// `updateRecentStickers` / `updateFavoriteStickers` /
    /// `updateTrendingStickerSets` arrived (another device changed them):
    /// refetch the loaded list.
    /// `getGreetingStickers`: hello stickers for empty private chats.
    pub greeting: Vec<StickerItem>,
    pub greeting_loaded: bool,
    /// `getAttachedStickerSets` answered: the sets (first opens in the
    /// sticker set dialog; `None` inside: the media has none).
    pub attached_answer: Option<Option<i64>>,
    /// The `getAttachedStickerSets` the open "Attached Stickers" box waits
    /// for. An answer or error of an earlier one is dropped.
    pub attached_request: Option<RequestId>,
    pub recent_stale: bool,
    pub favorites_stale: bool,
    pub trending_stale: bool,
    /// Slice S8: favorite stickers (`getFavoriteStickers`).
    pub favorites: Vec<StickerItem>,
    /// Slice S8: recent stickers (`getRecentStickers`).
    pub recent: Vec<StickerItem>,
    /// Slice S8: `searchStickerSets` / `searchStickers` results.
    pub found_sets: Vec<StickerSetInfo>,
    /// What `found_stickers` are (being) found for: a typed query or a
    /// category. One list per session; a window shows it only while it is
    /// for what that window asks.
    pub searched: StickerSearch,
    pub search_offset: usize,
    pub search_has_more: bool,
    pub found_stickers: Vec<StickerItem>,
    /// Slice S12: sticker suggestions for the composer's trailing emoji
    /// (`searchStickers` answers under `SuggestStickers`). `suggest_for`
    /// is the emoji they were requested for.
    pub suggestions: Vec<StickerItem>,
    pub suggest_for: Option<String>,
    pub suggest_waiting_for_sets: bool,
}

/// Saved GIFs (`getSavedAnimations`). tdesktop Gifs tab / Unigram animation drawer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifPanel {
    pub open: bool,
    pub animations: Vec<AnimationItem>,
    pub loaded: bool,
    pub search_mode: bool,
    pub search_loading: bool,
    pub search_failed: bool,
    pub search_query: String,
    pub search_bot_username: String,
    pub search_bot_user_id: Option<i64>,
    pub loading: bool,
    pub failed: bool,
    /// `updateSavedAnimations` arrived; refresh even if the panel was closed.
    pub stale: bool,
    /// Slice S9: GIF search via the animation search bot
    /// (`getInlineQueryResults`; schema 1.8.67, lines 6483, 13019).
    /// First page replaces; later pages append, deduped by file id.
    pub search_results: Vec<AnimationItem>,
    /// Slice S9: `next_offset` of the last search page ("" = exhausted).
    pub search_next_offset: String,
    pub search_offset: String,
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the upstream animation-search provider name and its
    /// suggested search emojis.
    pub search_provider: String,
    pub provider_emojis: Vec<String>,
    /// The searches answered lately, so a window's GIF tab shows its own
    /// search while another window's is the one in `search_results`.
    pub searches_kept: GifSearches,
}

/// GIF searches answered lately, by query: each window's GIF tab asks for
/// its own section (tdesktop has a `GifsListWidget` per window), but the
/// session pages one search at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifSearches(Vec<GifSearchKept>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifSearchKept {
    pub query: String,
    pub results: Vec<AnimationItem>,
}

impl GifSearches {
    /// Enough for every open window's GIF tab.
    pub const KEPT: usize = 8;

    /// The GIFs found for `query`, when it was answered lately.
    pub fn get(&self, query: &str) -> Option<&[AnimationItem]> {
        self.0
            .iter()
            .find(|kept| kept.query == query)
            .map(|kept| kept.results.as_slice())
    }

    /// `query` was answered with `results` (every page so far).
    pub fn keep(&mut self, query: &str, results: &[AnimationItem]) {
        self.0.retain(|kept| kept.query != query);
        self.0.push(GifSearchKept {
            query: query.to_string(),
            results: results.to_vec(),
        });
        let over = self.0.len().saturating_sub(Self::KEPT);
        self.0.drain(..over);
    }
}

impl GifPanel {
    pub fn visible_animations(&self) -> &[AnimationItem] {
        if self.search_mode {
            &self.search_results
        } else {
            &self.animations
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }
}

impl StickerPanel {
    pub fn visible_stickers(&self) -> &[StickerItem] {
        match self.tab {
            StickerTab::Installed | StickerTab::Trending => &self.stickers,
            StickerTab::Archived if self.selected_set_id.is_some() => &self.stickers,
            StickerTab::Archived => &[],
            StickerTab::Recent => &self.recent,
            StickerTab::Favorites => &self.favorites,
            StickerTab::Search if self.selected_set_id.is_some() => &self.stickers,
            StickerTab::Search => &self.found_stickers,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn selected_needs_load(&self) -> Option<i64> {
        let id = self.selected_set_id?;
        if self.set.set_id() == Some(id) {
            None
        } else {
            Some(id)
        }
    }
}
