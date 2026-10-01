//! Sticker and GIF panel state types.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StickerTab {
    #[default]
    Installed,
    Recent,
    Favorites,
    Trending,
    Search,
}

/// Composer sticker panel (Unigram `StickerDrawerViewModel` installed regular sets).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StickerPanel {
    pub open: bool,
    pub tab: StickerTab,
    pub sets: Vec<StickerSetInfo>,
    pub selected_set_id: Option<i64>,
    pub stickers: Vec<StickerItem>,
    pub loaded_set_id: Option<i64>,
    pub loading_sets: bool,
    pub loading_set: bool,
    pub failed: bool,
    /// Slice S8: trending sets (`getTrendingStickerSets`) + premium-row flag.
    pub trending: Vec<StickerSetInfo>,
    pub trending_total: usize,
    pub trending_offset: usize,
    pub trending_next_offset: usize,
    pub trending_is_premium: bool,
    /// Slice S8: favorite stickers (`getFavoriteStickers`).
    pub favorites: Vec<StickerItem>,
    /// Slice S8: recent stickers (`getRecentStickers`).
    pub recent: Vec<StickerItem>,
    /// Slice S8: `searchStickerSets` / `searchStickers` results.
    pub found_sets: Vec<StickerSetInfo>,
    pub search_query: String,
    pub search_offset: usize,
    pub search_has_more: bool,
    pub found_stickers: Vec<StickerItem>,
    /// Slice S12: sticker suggestions for the composer's trailing emoji
    /// (`searchStickers` answers under `SuggestStickers`). `suggest_for`
    /// is the emoji they were requested for.
    pub suggestions: Vec<StickerItem>,
    pub suggest_for: Option<String>,
}

/// Saved GIFs (`getSavedAnimations`). tdesktop Gifs tab / Unigram animation drawer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GifPanel {
    pub open: bool,
    pub animations: Vec<AnimationItem>,
    pub loading: bool,
    pub failed: bool,
    /// `updateSavedAnimations` arrived while the panel was open.
    pub stale: bool,
    /// Slice S9: GIF search via the animation search bot
    /// (`getInlineQueryResults`; schema 1.8.67, lines 6483, 13019).
    /// First page replaces; later pages append, deduped by file id.
    pub search_results: Vec<AnimationItem>,
    /// Slice S9: `next_offset` of the last search page ("" = exhausted).
    pub search_next_offset: String,
    /// Slice S9: `updateAnimationSearchParameters` (schema 1.8.67, line
    /// 11064) — the upstream animation-search provider name and its
    /// suggested search emojis.
    pub search_provider: String,
    pub provider_emojis: Vec<String>,
}

impl GifPanel {
    pub fn close(&mut self) {
        self.open = false;
    }
}

impl StickerPanel {
    pub fn visible_stickers(&self) -> &[StickerItem] {
        match self.tab {
            StickerTab::Installed | StickerTab::Trending => &self.stickers,
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
        if self.loading_set || self.loaded_set_id == Some(id) {
            None
        } else {
            Some(id)
        }
    }
}
