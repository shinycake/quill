//! Connect driver: sticker and GIF panels.
use super::*;
use crate::ids::FileId;
use crate::ids::RequestId;
use crate::state::StickersPurpose;
use crate::state::{RequestPurpose, StickerTab};
use crate::sticker_suggest::{SUGGEST_LIMIT, StickerSuggestMode, suggest_emoji_for};
use crate::telegram::requests::{
    add_favorite_sticker, change_sticker_set, clear_recent_stickers, get_archived_sticker_sets,
    get_favorite_stickers, get_recent_stickers, get_trending_sticker_sets, remove_favorite_sticker,
    reorder_installed_sticker_sets, search_sticker_sets, view_trending_sticker_sets,
};
use crate::telegram::requests::{
    get_installed_sticker_sets, get_saved_animations, get_sticker_set, search_stickers,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Recent and favorite stickers for the photo editor's Stickers mode,
    /// without opening the panel.
    pub fn fetch_editor_stickers(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.sticker_request_favorites()?;
        self.sticker_request(RequestPurpose::GetRecentStickers, |id| {
            get_recent_stickers(id, false)
        })?;
        Ok(())
    }

    /// B11: "Attached Stickers" in the media viewer menu
    /// (`getAttachedStickerSets` of the photo or video file). The answer
    /// opens the first set in the sticker set dialog.
    pub fn fetch_attached_sticker_sets(
        &mut self,
        file_id: FileId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.attached_answer = None;
        self.session.sticker_set_view = Some(crate::state::StickerSetView {
            set_id: 0,
            stage: crate::state::StickerSetViewStage::Loading,
            files_requested: false,
        });
        let extra = self.session.request(
            RequestPurpose::Stickers(StickersPurpose::GetAttachedStickerSets {
                file_id: file_id.0,
            }),
            None,
        );
        match self
            .sender
            .send_json(&crate::telegram::requests::get_attached_sticker_sets(
                extra, file_id,
            )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.sticker_set_view = None;
                Err(err)
            }
        }
    }

    /// The attached sets arrived: show the first, or say there are none.
    pub(crate) fn open_attached_sticker_set(&mut self) -> Result<(), ConnectSendError> {
        match self.session.stickers.attached_answer.take() {
            Some(Some(set_id)) => {
                self.view_sticker_set(set_id)?;
            }
            Some(None) => {
                if let Some(view) = self.session.sticker_set_view.as_mut() {
                    view.stage = crate::state::StickerSetViewStage::Failed;
                }
            }
            None => {}
        }
        Ok(())
    }

    /// B11: the hello stickers an empty private chat offers
    /// (`getGreetingStickers`), asked once per session.
    pub fn fetch_greeting_stickers(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self.session.stickers.greeting_loaded {
            return Ok(None);
        }
        // One attempt per session: a failure must not retry on every frame.
        self.session.stickers.greeting_loaded = true;
        self.sticker_request(RequestPurpose::GetGreetingStickers, |id| {
            crate::telegram::requests::get_greeting_stickers(id)
        })
    }

    /// Open the sticker panel and load installed regular sets
    /// (`getInstalledStickerSets` + `stickerTypeRegular`).
    pub fn open_sticker_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.open = true;
        let favorites = self.sticker_request_favorites()?;
        match self.session.stickers.tab {
            StickerTab::Recent => {
                return self.sticker_request(RequestPurpose::GetRecentStickers, |id| {
                    get_recent_stickers(id, false)
                });
            }
            StickerTab::Favorites => return Ok(favorites),
            StickerTab::Trending => return self.fetch_trending_stickers(false),
            StickerTab::Archived => return self.fetch_archived_stickers(false),
            StickerTab::Search => return Ok(None),
            StickerTab::Installed => {}
        }
        self.session.stickers.failed = false;
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            return Ok(None);
        }
        if !self.session.stickers.sets.is_empty() {
            return self.maybe_load_selected_sticker_set();
        }
        self.session.stickers.loading_sets = true;
        let extra = self
            .session
            .request(RequestPurpose::GetInstalledStickerSets, None);
        match self.sender.send_json(&get_installed_sticker_sets(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_sets = false;
                Err(err)
            }
        }
    }

    pub fn close_sticker_panel(&mut self) {
        self.session.stickers.close();
    }

    fn sticker_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Viewed pages are independent: a new page may arrive while the prior ack is pending.
        if purpose != RequestPurpose::ViewTrendingStickerSets
            && self.session.requests.has_purpose(purpose)
        {
            return Ok(None);
        }
        self.session.stickers.failed = false;
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            self.session.stickers.failed = true;
            return Err(err);
        }
        Ok(Some(extra))
    }

    pub fn select_sticker_tab(
        &mut self,
        tab: StickerTab,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if tab == StickerTab::Search
            || matches!(tab, StickerTab::Trending | StickerTab::Archived)
                && self.session.stickers.tab != tab
        {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetStickerSet),
            );
            self.session.stickers.loading_set = false;
            self.session.stickers.selected_set_id = None;
            self.session.stickers.loaded_set_id = None;
            self.session.stickers.stickers.clear();
        }
        if tab == StickerTab::Installed {
            let id = self.session.stickers.sets.first().map(|set| set.id);
            if let Some(id) = id
                && !self
                    .session
                    .stickers
                    .sets
                    .iter()
                    .any(|set| Some(set.id) == self.session.stickers.selected_set_id)
            {
                self.session.select_sticker_set(id);
            }
        }
        self.session.stickers.tab = tab;
        match tab {
            StickerTab::Installed => self.open_sticker_panel(),
            StickerTab::Recent => self.sticker_request(RequestPurpose::GetRecentStickers, |id| {
                get_recent_stickers(id, false)
            }),
            StickerTab::Favorites => {
                self.sticker_request(RequestPurpose::GetFavoriteStickers, get_favorite_stickers)
            }
            StickerTab::Trending => self.fetch_trending_stickers(false),
            StickerTab::Archived => self.fetch_archived_stickers(false),
            StickerTab::Search => Ok(None),
        }
    }

    pub fn search_sticker_picker(&mut self, query: &str) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SearchStickerSets),
        );
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SearchStickers),
        );
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetStickerSet),
        );
        self.session.stickers.selected_set_id = None;
        self.session.stickers.loaded_set_id = None;
        self.session.stickers.loading_set = false;
        self.session.stickers.failed = false;
        self.session.stickers.search_offset = 0;
        self.session.stickers.search_has_more = false;
        self.session.stickers.search_query = query.trim().to_string();
        self.session.stickers.found_sets.clear();
        self.session.stickers.found_stickers.clear();
        let query = self.session.stickers.search_query.clone();
        if query.is_empty() {
            return Ok(());
        }
        self.sticker_request(RequestPurpose::SearchStickerSets, |id| {
            search_sticker_sets(id, &query)
        })?;
        self.sticker_request(RequestPurpose::SearchStickers, |id| {
            search_stickers(id, "", &query, 0, 100)
        })?;
        Ok(())
    }

    pub fn more_sticker_search_results(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let query = self.session.stickers.search_query.clone();
        let offset = self.session.stickers.search_offset;
        if query.is_empty() || !self.session.stickers.search_has_more {
            return Ok(None);
        }
        self.sticker_request(RequestPurpose::SearchStickers, |id| {
            search_stickers(id, "", &query, offset as i32, 100)
        })
    }

    /// Batch changes report each confirmed result; they are not an atomic TDLib operation.
    pub fn manage_sticker_sets(
        &mut self,
        set_ids: &[i64],
        installed: bool,
    ) -> Result<usize, ConnectSendError> {
        if !self.chats_path_active() || set_ids.is_empty() || set_ids.iter().any(|id| *id <= 0) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.pending.values().any(|p| {
            matches!(
                p.purpose,
                RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
                    | RequestPurpose::ReorderInstalledStickerSets
            )
        }) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut seen = std::collections::HashSet::new();
        let ids: Vec<_> = set_ids
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect();
        self.session.stickers.batch_total = ids.len();
        self.session.stickers.batch_completed = 0;
        self.session.stickers.batch_failed = 0;
        self.session.stickers.batch_pending = ids.clone();
        let mut sent = 0;
        for id in ids {
            if self
                .manage_sticker_set(id, installed, false)
                .is_ok_and(|request| request.is_some())
            {
                sent += 1;
            } else {
                self.session.finish_sticker_batch_item(id, false);
            }
        }
        Ok(sent)
    }

    pub fn manage_sticker_set(
        &mut self,
        set_id: i64,
        installed: bool,
        archived: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || set_id <= 0 || installed && archived {
            return Err(ConnectSendError::InvalidRequest);
        }
        // Serialize contradictory operations on one set, while allowing independent sets to change.
        let busy = [(true, false), (false, true), (false, false)]
            .into_iter()
            .any(|(installed, archived)| {
                self.session.requests.has_purpose(RequestPurpose::Stickers(
                    StickersPurpose::ManageStickerSet {
                        set_id,
                        installed,
                        archived,
                    },
                ))
            });
        if busy
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::ReorderInstalledStickerSets)
        {
            return Ok(None);
        }
        self.sticker_request(
            RequestPurpose::Stickers(StickersPurpose::ManageStickerSet {
                set_id,
                installed,
                archived,
            }),
            |id| change_sticker_set(id, set_id, installed, archived),
        )
    }

    pub(crate) fn refresh_installed_sticker_sets(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.sticker_request(
            RequestPurpose::GetInstalledStickerSets,
            get_installed_sticker_sets,
        )
    }

    /// Move an installed set to the target slot; commit only after TDLib confirms.
    pub fn reorder_sticker_set(
        &mut self,
        source: i64,
        target: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let mut ids: Vec<_> = self
            .session
            .stickers
            .sets
            .iter()
            .map(|set| set.id)
            .collect();
        let from = ids.iter().position(|id| *id == source);
        let to = ids.iter().position(|id| *id == target);
        let (Some(from), Some(to)) = (from, to) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        if unique.len() != ids.len() || ids.iter().any(|id| *id <= 0) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if from == to
            || self.session.requests.pending.values().any(|pending| {
                matches!(
                    pending.purpose,
                    RequestPurpose::Stickers(StickersPurpose::ManageStickerSet { .. })
                        | RequestPurpose::ReorderInstalledStickerSets
                        | RequestPurpose::GetInstalledStickerSets
                )
            })
        {
            return Ok(None);
        }
        ids.remove(from);
        ids.insert(to, source);
        self.sticker_request(RequestPurpose::ReorderInstalledStickerSets, |id| {
            reorder_installed_sticker_sets(id, &ids)
        })
    }

    pub fn fetch_archived_stickers(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetArchivedStickerSets)
            || more && !self.session.stickers.archived_has_more
        {
            return Ok(None);
        }
        let offset = if more {
            self.session.stickers.archived_next_offset
        } else {
            0
        };
        self.session.stickers.archived_offset = offset;
        self.sticker_request(RequestPurpose::GetArchivedStickerSets, |id| {
            get_archived_sticker_sets(id, offset)
        })
    }

    pub fn fetch_trending_stickers(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetTrendingStickerSets)
        {
            return Ok(None);
        }
        let offset = if more {
            self.session.stickers.trending_next_offset
        } else {
            0
        };
        self.session.stickers.trending_offset = offset;
        self.sticker_request(RequestPurpose::GetTrendingStickerSets, |id| {
            get_trending_sticker_sets(id, offset as i32, 100)
        })
    }

    pub(crate) fn sticker_request_favorites(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.sticker_request(RequestPurpose::GetFavoriteStickers, get_favorite_stickers)
    }

    pub fn clear_recent_stickers(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.sticker_request(RequestPurpose::ClearRecentStickers, |id| {
            clear_recent_stickers(id, false)
        })
    }

    /// "Remove from recent": `removeRecentSticker`. The sticker leaves
    /// the cached list at once; `updateRecentStickers` then refetches it.
    pub fn remove_recent_sticker(
        &mut self,
        file_id: FileId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let sent = self.sticker_request(RequestPurpose::RemoveRecentSticker, |id| {
            crate::telegram::requests::remove_recent_sticker(id, file_id, false)
        })?;
        if sent.is_some() {
            self.session
                .stickers
                .recent
                .retain(|sticker| sticker.file_id != file_id);
        }
        Ok(sent)
    }

    pub fn set_favorite_sticker(
        &mut self,
        file_id: FileId,
        favorite: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let purpose = if favorite {
            RequestPurpose::AddFavoriteSticker
        } else {
            RequestPurpose::RemoveFavoriteSticker
        };
        self.sticker_request(purpose, |id| {
            if favorite {
                add_favorite_sticker(id, file_id)
            } else {
                remove_favorite_sticker(id, file_id)
            }
        })
    }

    pub(crate) fn mark_trending_stickers_viewed(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let ids: Vec<_> = self
            .session
            .stickers
            .trending
            .iter()
            .map(|set| set.id)
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        self.sticker_request(RequestPurpose::ViewTrendingStickerSets, |id| {
            view_trending_sticker_sets(id, &ids)
        })
    }

    /// Slice S12: refresh the composer sticker suggestions for the
    /// current composer text. `None` mode or no trailing emoji clears
    /// the row; an unchanged emoji is not re-requested; a stale
    /// in-flight suggest is dropped before the new one goes out, so a
    /// late answer can never land under a newer emoji. Returns the
    /// issued `RequestId` when a `searchStickers` went out. The UI
    /// calls this on composer text change; rendering the suggestion
    /// row is the post-Phase-9 UI slice.
    pub fn update_sticker_suggestions(
        &mut self,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let emoji = match self.session.media_prefs.sticker_suggest_mode {
            StickerSuggestMode::None => None,
            _ => suggest_emoji_for(text),
        };
        let Some(emoji) = emoji else {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::SuggestStickers),
            );
            self.session.clear_sticker_suggestions();
            return Ok(None);
        };
        if self.session.media_prefs.sticker_suggest_mode == StickerSuggestMode::InstalledOnly
            && !self.session.stickers.installed_loaded
        {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::SuggestStickers),
            );
            self.session.stickers.suggestions.clear();
            self.session.stickers.suggest_for = Some(emoji.to_string());
            self.session.stickers.suggest_waiting_for_sets = true;
            return self.refresh_installed_sticker_sets();
        }
        self.session.stickers.suggest_waiting_for_sets = false;
        if self.session.stickers.suggest_for.as_deref() == Some(emoji) {
            return Ok(None);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SuggestStickers),
        );
        self.session.stickers.suggest_for = Some(emoji.to_string());
        let extra = self.session.request(RequestPurpose::SuggestStickers, None);
        match self
            .sender
            .send_json(&search_stickers(extra, emoji, "", 0, SUGGEST_LIMIT))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.suggest_for = None;
                Err(err)
            }
        }
    }

    pub fn select_sticker_set(
        &mut self,
        set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.select_sticker_set(set_id);
        self.maybe_load_selected_sticker_set()
    }

    pub(crate) fn maybe_load_selected_sticker_set(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.stickers.open
            || !self.chats_path_active()
            || !matches!(
                self.session.stickers.tab,
                StickerTab::Installed
                    | StickerTab::Trending
                    | StickerTab::Search
                    | StickerTab::Archived
            )
        {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetStickerSet)
        {
            return Ok(None);
        }
        let Some(set_id) = self.session.stickers.selected_needs_load() else {
            return Ok(None);
        };
        self.session.mark_sticker_set_loading();
        let extra = self.session.request(RequestPurpose::GetStickerSet, None);
        match self.sender.send_json(&get_sticker_set(extra, set_id)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.loading_set = false;
                Err(err)
            }
        }
    }

    /// Open the GIF panel and load `getSavedAnimations`.
    pub fn open_gif_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.gifs.open = true;
        if self.session.gifs.failed {
            self.session.gifs.loaded = false;
        }
        if self.session.gifs.search_mode && self.session.gifs.search_failed {
            self.session.gifs.search_loading = true;
        }
        self.session.gifs.failed = false;
        let saved = self.maybe_refresh_saved_animations()?;
        if self.session.gifs.search_mode && self.session.gifs.search_loading {
            Ok(self.maybe_search_gifs(false)?.or(saved))
        } else {
            Ok(saved)
        }
    }

    /// B11: refetch what another device changed (`updateRecentStickers`,
    /// `updateFavoriteStickers`, `updateTrendingStickerSets`, and the
    /// reaction picker's options after `updateActiveEmojiReactions` /
    /// `updateChatAvailableReactions`) so open panels follow without
    /// reopening. Lists nobody has loaded yet are fetched on first open
    /// anyway, so a stale flag on an empty, closed list is just dropped.
    pub(crate) fn refresh_stale_panels(&mut self) -> Result<(), ConnectSendError> {
        let stickers = &mut self.session.stickers;
        let recent = std::mem::take(&mut stickers.recent_stale);
        let favorites = std::mem::take(&mut stickers.favorites_stale);
        let trending = std::mem::take(&mut stickers.trending_stale);
        let open = stickers.open;
        let recent = recent && (open || !stickers.recent.is_empty());
        let favorites = favorites && (open || !stickers.favorites.is_empty());
        let trending = trending && open && stickers.tab == StickerTab::Trending;
        let emoji_trending = std::mem::take(&mut self.session.emoji.trending_stale)
            && self.session.emoji.open
            && self.session.emoji.tab == crate::emoji::EmojiSetTab::Trending;
        let reactions = std::mem::take(&mut self.session.reaction_options_stale);
        if !self.chats_path_active() {
            return Ok(());
        }
        if recent {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetRecentStickers),
            );
            self.sticker_request(RequestPurpose::GetRecentStickers, |id| {
                get_recent_stickers(id, false)
            })?;
        }
        if favorites {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetFavoriteStickers),
            );
            self.sticker_request_favorites()?;
        }
        if trending {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetTrendingStickerSets),
            );
            self.fetch_trending_stickers(false)?;
        }
        if emoji_trending {
            self.select_emoji_set_tab(crate::emoji::EmojiSetTab::Trending)?;
        }
        if reactions && let Some(options) = self.session.message_reaction_options.take() {
            self.fetch_message_reactions(options.chat_id, options.message_id)?;
        }
        Ok(())
    }

    pub fn close_gif_panel(&mut self) {
        self.session.gifs.close();
    }

    pub(crate) fn maybe_refresh_saved_animations(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.gifs.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetSavedAnimations)
        {
            return Ok(None);
        }
        let needs = !self.session.gifs.loaded || self.session.gifs.stale;
        if !needs {
            return Ok(None);
        }
        self.session.gifs.loading = true;
        self.session.gifs.stale = false;
        let extra = self
            .session
            .request(RequestPurpose::GetSavedAnimations, None);
        match self.sender.send_json(&get_saved_animations(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.gifs.loading = false;
                self.session.gifs.loaded = true;
                self.session.gifs.failed = true;
                Err(err)
            }
        }
    }
}
