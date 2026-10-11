//! Sticker set acceptors.
use super::*;

impl Session {
    pub fn sticker_requires_premium(&self, file_id: FileId) -> bool {
        [&self.stickers.stickers.stickers, &self.stickers.stickers.recent, &self.stickers.stickers.favorites,
            &self.stickers.stickers.found_stickers, &self.stickers.stickers.suggestions].into_iter()
            .flatten().any(|sticker| sticker.file_id == file_id && sticker.requires_premium)
            || self.histories.values().flat_map(|history| history.messages.values()).chain(self.threads.topic_histories.values().flat_map(|history| history.messages.values())).any(|message| {
                matches!(&message.content, MessageContent::Sticker(sticker) if sticker.file_id == file_id && sticker.requires_premium)
            })
    }

    pub(crate) fn finish_sticker_batch_item(&mut self, set_id: i64, success: bool) {
        if let Some(index) = self
            .stickers
            .stickers
            .batch_pending
            .iter()
            .position(|id| *id == set_id)
        {
            self.stickers.stickers.batch_pending.remove(index);
            if success {
                self.stickers.stickers.batch_completed += 1;
            } else {
                self.stickers.stickers.batch_failed += 1;
            }
        }
    }

    pub fn accept_archived_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        let next = sets.last().map(|set| set.id).unwrap_or(0);
        self.stickers.stickers.archived_has_more =
            sets.len() == 100 && next > 0 && next != self.stickers.stickers.archived_offset;
        self.stickers.stickers.archived_next_offset = next;
        if self.stickers.stickers.archived_offset == 0 {
            self.stickers.stickers.archived.clear();
        }
        let mut ids: HashSet<_> = self
            .stickers
            .stickers
            .archived
            .iter()
            .map(|set| set.id)
            .collect();
        self.stickers
            .stickers
            .archived
            .extend(sets.into_iter().filter(|set| ids.insert(set.id)));
        self.stickers.stickers.failed = false;
    }

    pub fn accept_installed_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.stickers.loading_sets = false;
        self.stickers.stickers.failed = false;
        self.stickers.stickers.sets = sets;
        self.stickers.stickers.installed_loaded = true;
        if self.stickers.stickers.tab != StickerTab::Installed {
            return;
        }
        let still_selected = self
            .stickers
            .stickers
            .selected_set_id
            .is_some_and(|id| self.stickers.stickers.sets.iter().any(|set| set.id == id));
        if !still_selected {
            self.stickers.stickers.selected_set_id =
                self.stickers.stickers.sets.first().map(|set| set.id);
            self.stickers.stickers.loaded_set_id = None;
            self.stickers.stickers.stickers.clear();
        }
    }

    /// Replace the first trending page; append later pages without duplicate sets.
    pub fn accept_trending_sticker_sets(&mut self, sets: Vec<StickerSetInfo>, is_premium: bool) {
        self.stickers.stickers.trending_next_offset =
            self.stickers.stickers.trending_offset + sets.len();
        if sets.is_empty() {
            self.stickers.stickers.trending_total = self.stickers.stickers.trending_next_offset;
        }
        if self.stickers.stickers.trending_offset == 0 {
            self.stickers.stickers.trending = sets;
        } else {
            let mut ids: HashSet<_> = self
                .stickers
                .stickers
                .trending
                .iter()
                .map(|set| set.id)
                .collect();
            self.stickers
                .stickers
                .trending
                .extend(sets.into_iter().filter(|set| ids.insert(set.id)));
        }
        self.stickers.stickers.trending_is_premium = is_premium;
    }

    /// Slice S8: store a `getFavoriteStickers` answer.
    pub fn accept_favorite_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.stickers.favorites = stickers;
    }

    /// Slice S8: store a `getRecentStickers` answer.
    pub fn accept_recent_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.stickers.recent = stickers;
    }

    /// Slice S8: store a `searchStickerSets` answer.
    pub fn accept_found_sticker_sets(&mut self, sets: Vec<StickerSetInfo>) {
        self.stickers.stickers.found_sets = sets;
    }

    /// Slice S8: store a `searchStickers` answer.
    pub fn accept_found_stickers(&mut self, stickers: Vec<StickerItem>) {
        self.stickers.stickers.search_has_more = stickers.len() == 100;
        if self.stickers.stickers.search_offset == 0 {
            self.stickers.stickers.found_stickers.clear();
        }
        self.stickers.stickers.search_offset += stickers.len();
        let mut files: HashSet<_> = self
            .stickers
            .stickers
            .found_stickers
            .iter()
            .map(|sticker| sticker.file_id)
            .collect();
        self.stickers.stickers.found_stickers.extend(
            stickers
                .into_iter()
                .filter(|sticker| files.insert(sticker.file_id)),
        );
    }

    /// Slice S12: drop the composer sticker suggestions.
    pub fn clear_sticker_suggestions(&mut self) {
        self.stickers.stickers.suggestions.clear();
        self.stickers.stickers.suggest_for = None;
        self.stickers.stickers.suggest_waiting_for_sets = false;
    }

    /// Slice S12: store a `searchStickers` answer issued for the
    /// composer's trailing emoji. `InstalledOnly` keeps results from
    /// installed sets; `None` drops the answer (the mode changed
    /// mid-flight).
    pub fn accept_sticker_suggestions(&mut self, stickers: Vec<StickerItem>) {
        match self.settings.media_prefs.sticker_suggest_mode {
            StickerSuggestMode::None => {
                self.clear_sticker_suggestions();
            }
            StickerSuggestMode::InstalledOnly => {
                let installed: HashSet<i64> = self
                    .stickers
                    .stickers
                    .sets
                    .iter()
                    .map(|set| set.id)
                    .collect();
                self.stickers.stickers.suggestions = stickers
                    .into_iter()
                    .filter(|sticker| installed.contains(&sticker.set_id))
                    .collect();
            }
            StickerSuggestMode::InstalledAndRecommended => {
                self.stickers.stickers.suggestions = stickers;
            }
        }
    }

    /// Slice S8: a sticker-set mutation (`changeStickerSet` /
    /// `reorderInstalledStickerSets`) succeeded — drop the installed-sets
    /// cache so the panel refetches the authoritative list instead of
    /// showing a stale order.
    pub fn invalidate_installed_sticker_sets(&mut self) {
        self.stickers.stickers.installed_loaded = false;
        self.stickers.stickers.sets.clear();
        self.stickers.stickers.selected_set_id = None;
        self.stickers.stickers.loaded_set_id = None;
        self.stickers.stickers.stickers.clear();
    }

    /// Slice S15: apply TDLib's `updateInstalledStickerSets` order to the
    /// cached installed sets. Stable: sets missing from the update keep
    /// their relative order at the end. Regular sets reorder the sticker
    /// panel; other types reorder the emoji panel's installed sets. This
    /// never fights the manual reorder (`parity:stickers-reorder`) — both
    /// flows converge on TDLib's authoritative order.
    pub fn apply_installed_sticker_set_order(&mut self, ids: &[i64], is_regular: bool) {
        let sets = if is_regular {
            &mut self.stickers.stickers.sets
        } else {
            // Non-regular types (custom emoji; mask sets are never fetched)
            // route to the emoji panel's installed sets.
            &mut self.stickers.emoji.installed_sets
        };
        if sets.is_empty() || ids.is_empty() {
            return;
        }
        let rank: HashMap<i64, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        sets.sort_by_key(|set| rank.get(&set.id).copied().unwrap_or(usize::MAX));
    }

    pub fn select_sticker_set(&mut self, set_id: i64) {
        if self.stickers.stickers.selected_set_id == Some(set_id) {
            return;
        }
        self.stickers.stickers.selected_set_id = Some(set_id);
        self.stickers.stickers.loaded_set_id = None;
        self.stickers.stickers.stickers.clear();
        self.stickers.stickers.loading_set = false;
        self.stickers.stickers.failed = false;
    }

    pub fn mark_sticker_set_loading(&mut self) {
        self.stickers.stickers.loading_set = true;
        self.stickers.stickers.failed = false;
    }

    pub fn accept_saved_animations(&mut self, animations: Vec<AnimationItem>) {
        self.stickers.gifs.loading = false;
        self.stickers.gifs.failed = false;
        self.stickers.gifs.stale = false;
        self.stickers.gifs.animations = animations;
        self.stickers.gifs.loaded = true;
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
        self.stickers.gifs.search_loading = false;
        self.stickers.gifs.search_failed = false;
        if first_page {
            self.stickers.gifs.search_results = animations;
        } else {
            for item in animations {
                if !self
                    .stickers
                    .gifs
                    .search_results
                    .iter()
                    .any(|r| r.file_id == item.file_id)
                {
                    self.stickers.gifs.search_results.push(item);
                }
            }
        }
        self.stickers.gifs.search_next_offset =
            if !next_offset.is_empty() && next_offset == self.stickers.gifs.search_offset {
                String::new()
            } else {
                next_offset
            };
    }

    pub fn accept_sticker_set(&mut self, id: i64, stickers: Vec<StickerItem>) {
        self.stickers.stickers.loading_set = false;
        if self
            .stickers
            .stickers
            .selected_set_id
            .is_some_and(|selected| selected != id)
        {
            return;
        }
        self.stickers.stickers.failed = false;
        self.stickers.stickers.loaded_set_id = Some(id);
        self.stickers.stickers.stickers = stickers;
    }
}
