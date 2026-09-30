//! Sticker set acceptors.
use super::*;

impl Session {
    pub fn accept_installed_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.loading_sets = false;
        self.stickers.failed = false;
        self.stickers.sets = sets;
        let still_selected = self
            .stickers
            .selected_set_id
            .is_some_and(|id| self.stickers.sets.iter().any(|set| set.id == id));
        if !still_selected {
            self.stickers.selected_set_id = self.stickers.sets.first().map(|set| set.id);
            self.stickers.loaded_set_id = None;
            self.stickers.stickers.clear();
        }
    }

    /// Slice S8: store a `trendingStickerSets` page. Single-page replace
    /// semantics: a paged second call overwrites page one. Append-before-
    /// needed is speculative — the tab UI will own paging when it lands.
    pub fn accept_trending_sticker_sets(&mut self, sets: Vec<StickerSetInfo>, is_premium: bool) {
        self.stickers.trending = sets;
        self.stickers.trending_is_premium = is_premium;
    }

    /// Slice S8: store a `getFavoriteStickers` answer.
    pub fn accept_favorite_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.favorites = stickers;
    }

    /// Slice S8: store a `getRecentStickers` answer.
    pub fn accept_recent_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.recent = stickers;
    }

    /// Slice S8: store a `searchStickerSets` answer.
    pub fn accept_found_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.found_sets = sets;
    }

    /// Slice S8: store a `searchStickers` answer.
    pub fn accept_found_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.found_stickers = stickers;
    }

    /// Slice S12: drop the composer sticker suggestions.
    pub fn clear_sticker_suggestions(&mut self) {
        self.stickers.suggestions.clear();
        self.stickers.suggest_for = None;
    }

    /// Slice S12: store a `searchStickers` answer issued for the
    /// composer's trailing emoji. `InstalledOnly` keeps results from
    /// installed sets; `None` drops the answer (the mode changed
    /// mid-flight).
    pub fn accept_sticker_suggestions(&mut self, stickers: Vec<StickerItem>) {
        match self.media_prefs.sticker_suggest_mode {
            StickerSuggestMode::None => {
                self.clear_sticker_suggestions();
            }
            StickerSuggestMode::InstalledOnly => {
                let installed: HashSet<i64> = self.stickers.sets.iter().map(|set| set.id).collect();
                self.stickers.suggestions = stickers
                    .into_iter()
                    .filter(|sticker| installed.contains(&sticker.set_id))
                    .collect();
            }
            StickerSuggestMode::InstalledAndRecommended => {
                self.stickers.suggestions = stickers;
            }
        }
    }

    /// Slice S8: a sticker-set mutation (`changeStickerSet` /
    /// `reorderInstalledStickerSets`) succeeded — drop the installed-sets
    /// cache so the panel refetches the authoritative list instead of
    /// showing a stale order.
    pub fn invalidate_installed_sticker_sets(&mut self) {
        self.stickers.sets.clear();
        self.stickers.selected_set_id = None;
        self.stickers.loaded_set_id = None;
        self.stickers.stickers.clear();
    }

    /// Slice S15: apply TDLib's `updateInstalledStickerSets` order to the
    /// cached installed sets. Stable: sets missing from the update keep
    /// their relative order at the end. Regular sets reorder the sticker
    /// panel; other types reorder the emoji panel's installed sets. This
    /// never fights the manual reorder (`parity:stickers-reorder`) — both
    /// flows converge on TDLib's authoritative order.
    pub fn apply_installed_sticker_set_order(&mut self, ids: &[i64], is_regular: bool) {
        let sets = if is_regular {
            &mut self.stickers.sets
        } else {
            // Non-regular types (custom emoji; mask sets are never fetched)
            // route to the emoji panel's installed sets.
            &mut self.emoji.installed_sets
        };
        if sets.is_empty() || ids.is_empty() {
            return;
        }
        let rank: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        sets.sort_by_key(|set| rank.get(&set.id).copied().unwrap_or(usize::MAX));
    }

    pub fn select_sticker_set(&mut self, set_id: i64) {
        if self.stickers.selected_set_id == Some(set_id) {
            return;
        }
        self.stickers.selected_set_id = Some(set_id);
        self.stickers.loaded_set_id = None;
        self.stickers.stickers.clear();
        self.stickers.loading_set = false;
        self.stickers.failed = false;
    }

    pub fn mark_sticker_set_loading(&mut self) {
        self.stickers.loading_set = true;
        self.stickers.failed = false;
    }

    pub fn accept_saved_animations(&mut self, animations: Vec<AnimationItem>) {
        self.gifs.loading = false;
        self.gifs.failed = false;
        self.gifs.stale = false;
        self.gifs.animations = animations;
    }

    /// Slice S9: store a GIF-search `inlineQueryResults` page. A first page
    /// replaces; a later page appends, deduped by animation file id, and
    /// keeps the page's `next_offset` — mirrors the composer's inline-query
    /// paging (Loop 3) without its slot machinery, since the purpose carries
    /// `first_page`.
    pub fn accept_gif_search_results(
        &mut self,
        animations: Vec<AnimationItem>,
        next_offset: String,
        first_page: bool,
    ) {
        if first_page {
            self.gifs.search_results = animations;
        } else {
            for item in animations {
                if !self
                    .gifs
                    .search_results
                    .iter()
                    .any(|r| r.file_id == item.file_id)
                {
                    self.gifs.search_results.push(item);
                }
            }
        }
        self.gifs.search_next_offset = next_offset;
    }

    pub fn accept_sticker_set(&mut self, id: i64, stickers: Vec<StickerItem>) {
        self.stickers.loading_set = false;
        if self
            .stickers
            .selected_set_id
            .is_some_and(|selected| selected != id)
        {
            return;
        }
        self.stickers.failed = false;
        self.stickers.loaded_set_id = Some(id);
        self.stickers.stickers = stickers;
    }
}
