//! Connect driver: sticker and GIF panels.
use super::*;
use crate::ids::FileId;
use crate::ids::RequestId;
use crate::state::StickersPurpose;
use crate::state::{RequestPurpose, StickerSearch, StickerTab};
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
        self.session.stickers.stickers.attached_answer = None;
        self.session.stickers.sticker_set_view = Some(crate::state::StickerSetView::loading(0));
        let extra = self.session.request(
            RequestPurpose::Stickers(StickersPurpose::GetAttachedStickerSets {
                file_id: file_id.0,
            }),
            None,
        );
        self.session.stickers.stickers.attached_request = Some(extra);
        match self
            .sender
            .send_json(&crate::telegram::requests::get_attached_sticker_sets(
                extra, file_id,
            )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.stickers.attached_request = None;
                self.session.stickers.sticker_set_view = None;
                Err(err)
            }
        }
    }

    /// The attached sets arrived: show the first, or say there are none.
    /// Only into the box still waiting for them (set 0, loading): one
    /// closed meanwhile stays closed, and another set's box is left alone.
    pub(crate) fn open_attached_sticker_set(&mut self) -> Result<(), ConnectSendError> {
        let answer = self.session.stickers.stickers.attached_answer.take();
        let waiting = self
            .session
            .stickers
            .sticker_set_view
            .as_ref()
            .is_some_and(|view| {
                view.set_id == 0 && view.stage == crate::state::StickerSetViewStage::Loading
            });
        if !waiting {
            return Ok(());
        }
        match answer {
            Some(Some(set_id)) => {
                self.view_sticker_set(set_id)?;
            }
            Some(None) => {
                if let Some(view) = self.session.stickers.sticker_set_view.as_mut() {
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
        if self.session.stickers.stickers.greeting_loaded {
            return Ok(None);
        }
        // One attempt per session: a failure must not retry on every frame.
        self.session.stickers.stickers.greeting_loaded = true;
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
        self.session.stickers.stickers.open = true;
        let favorites = self.sticker_request_favorites()?;
        match self.session.stickers.stickers.tab {
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
        // Opening the panel again tries a failed list and set again.
        let panel = &mut self.session.stickers.stickers;
        panel.lists.installed.clear_error();
        if panel.set.is_failed() {
            panel.set = crate::state::StickerSetStage::Idle;
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            return Ok(None);
        }
        if !self.session.stickers.stickers.sets.is_empty() {
            return self.maybe_load_selected_sticker_set();
        }
        let extra = self
            .session
            .request(RequestPurpose::GetInstalledStickerSets, None);
        match self.sender.send_json(&get_installed_sticker_sets(extra)) {
            Ok(()) => {
                self.session.stickers.stickers.lists.installed.begin();
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                self.session
                    .stickers
                    .stickers
                    .lists
                    .installed
                    .fail(crate::lang::tr(
                        crate::lang::keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN,
                    ));
                Err(err)
            }
        }
    }

    pub fn close_sticker_panel(&mut self) {
        self.session.stickers.stickers.close();
    }

    pub(super) fn sticker_request(
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
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            if let Some(list) = self.session.stickers.stickers.lists.for_purpose(purpose) {
                list.fail(crate::lang::tr(
                    crate::lang::keys::Q_MEDIA_COULDNT_UPDATE_STICKERS_TRY_AGAIN,
                ));
            }
            return Err(err);
        }
        if let Some(list) = self.session.stickers.stickers.lists.for_purpose(purpose) {
            list.begin();
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
                && self.session.stickers.stickers.tab != tab
        {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetStickerSet),
            );
            self.session.stickers.stickers.set = crate::state::StickerSetStage::Idle;
            self.session.stickers.stickers.selected_set_id = None;
            self.session.stickers.stickers.stickers.clear();
        }
        if tab == StickerTab::Installed {
            let id = self
                .session
                .stickers
                .stickers
                .sets
                .first()
                .map(|set| set.id);
            if let Some(id) = id
                && !self
                    .session
                    .stickers
                    .stickers
                    .sets
                    .iter()
                    .any(|set| Some(set.id) == self.session.stickers.stickers.selected_set_id)
            {
                self.session.select_sticker_set(id);
            }
        }
        self.session.stickers.stickers.tab = tab;
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
        self.session.stickers.stickers.selected_set_id = None;
        self.session.stickers.stickers.set = crate::state::StickerSetStage::Idle;
        self.session.stickers.stickers.lists.search.cancel();
        self.session.stickers.stickers.lists.search.clear_error();
        self.session.stickers.stickers.search_offset = 0;
        self.session.stickers.stickers.search_has_more = false;
        self.session.stickers.stickers.searched = StickerSearch::Typed(query.trim().to_string());
        self.session.stickers.stickers.found_sets.clear();
        self.session.stickers.stickers.found_stickers.clear();
        let query = self.session.stickers.stickers.searched.typed().to_string();
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
        let query = self.session.stickers.stickers.searched.typed().to_string();
        let offset = self.session.stickers.stickers.search_offset;
        if query.is_empty() || !self.session.stickers.stickers.search_has_more {
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
        self.session.stickers.stickers.batch_total = ids.len();
        self.session.stickers.stickers.batch_completed = 0;
        self.session.stickers.stickers.batch_failed = 0;
        self.session.stickers.stickers.batch_pending = ids.clone();
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
            || more && !self.session.stickers.stickers.archived_has_more
        {
            return Ok(None);
        }
        let offset = if more {
            self.session.stickers.stickers.archived_next_offset
        } else {
            0
        };
        self.session.stickers.stickers.archived_offset = offset;
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
            self.session.stickers.stickers.trending_next_offset
        } else {
            0
        };
        self.session.stickers.stickers.trending_offset = offset;
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

    /// Whether the composer's sticker and animated-emoji suggestions have
    /// nothing to do for `text`: the path is live, the text does not end in
    /// an emoji, and neither suggestion nor its request is held. Then
    /// `update_sticker_suggestions` and `update_animated_emoji_suggestion`
    /// would only clear what is already clear.
    pub fn composer_suggestions_idle(&self, text: &str) -> bool {
        let stickers = &self.session.stickers.stickers;
        let emoji = &self.session.stickers.emoji;
        self.chats_path_active()
            && suggest_emoji_for(text).is_none()
            && stickers.suggestions.is_empty()
            && stickers.suggest_for.is_none()
            && !stickers.suggest_waiting_for_sets
            && emoji.animated_emoji.is_none()
            && emoji.animated_emoji_for.is_none()
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::SuggestStickers)
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetAnimatedEmoji)
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
        let emoji = match self.session.settings.media_prefs.sticker_suggest_mode {
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
        if self.session.settings.media_prefs.sticker_suggest_mode
            == StickerSuggestMode::InstalledOnly
            && !self.session.stickers.stickers.installed_loaded
        {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::SuggestStickers),
            );
            self.session.stickers.stickers.suggestions.clear();
            self.session.stickers.stickers.suggest_for = Some(emoji.to_string());
            self.session.stickers.stickers.suggest_waiting_for_sets = true;
            return self.refresh_installed_sticker_sets();
        }
        self.session.stickers.stickers.suggest_waiting_for_sets = false;
        if self.session.stickers.stickers.suggest_for.as_deref() == Some(emoji) {
            return Ok(None);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SuggestStickers),
        );
        self.session.stickers.stickers.suggest_for = Some(emoji.to_string());
        let extra = self.session.request(RequestPurpose::SuggestStickers, None);
        match self
            .sender
            .send_json(&search_stickers(extra, emoji, "", 0, SUGGEST_LIMIT))
        {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.stickers.suggest_for = None;
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
        if !self.session.stickers.stickers.open
            || !self.chats_path_active()
            || !matches!(
                self.session.stickers.stickers.tab,
                StickerTab::Installed
                    | StickerTab::Trending
                    | StickerTab::Search
                    | StickerTab::Archived
            )
        {
            return Ok(None);
        }
        let Some(set_id) = self.session.stickers.stickers.selected_needs_load() else {
            return Ok(None);
        };
        // A request for a set that is no longer the selected one is dropped:
        // its answer would match nothing.
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetStickerSet),
        );
        let extra = self.session.request(RequestPurpose::GetStickerSet, None);
        match self.sender.send_json(&get_sticker_set(extra, set_id)) {
            Ok(()) => {
                self.session.mark_sticker_set_loading(set_id, extra);
                Ok(Some(extra))
            }
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.stickers.set =
                    crate::state::StickerSetStage::Failed { set_id };
                Err(err)
            }
        }
    }

    /// Open the GIF panel and load `getSavedAnimations`.
    pub fn open_gif_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.gifs.open = true;
        if self.session.stickers.gifs.failed {
            self.session.stickers.gifs.loaded = false;
        }
        if self.session.stickers.gifs.search_mode && self.session.stickers.gifs.search_failed {
            self.session.stickers.gifs.search_loading = true;
        }
        self.session.stickers.gifs.failed = false;
        let saved = self.maybe_refresh_saved_animations()?;
        if self.session.stickers.gifs.search_mode && self.session.stickers.gifs.search_loading {
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
        // A flag leaves only when its refetch went out: while the path is
        // inactive, or when an earlier send fails, the rest stay set.
        let stickers = &mut self.session.stickers.stickers;
        let open = stickers.open;
        // A list nobody shows and nobody loaded is fetched on first open.
        stickers.recent_stale &= open || !stickers.recent.is_empty();
        stickers.favorites_stale &= open || !stickers.favorites.is_empty();
        stickers.trending_stale &= open && stickers.tab == StickerTab::Trending;
        let (recent, favorites, trending) = (
            stickers.recent_stale,
            stickers.favorites_stale,
            stickers.trending_stale,
        );
        let emoji = &mut self.session.stickers.emoji;
        emoji.trending_stale &= emoji.open && emoji.tab == crate::emoji::EmojiSetTab::Trending;
        let emoji_trending = emoji.trending_stale;
        let reactions = self.session.stickers.reaction_options_stale;
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
            self.session.stickers.stickers.recent_stale = false;
        }
        if favorites {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetFavoriteStickers),
            );
            self.sticker_request_favorites()?;
            self.session.stickers.stickers.favorites_stale = false;
        }
        if trending {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetTrendingStickerSets),
            );
            self.fetch_trending_stickers(false)?;
            self.session.stickers.stickers.trending_stale = false;
        }
        if emoji_trending {
            self.select_emoji_set_tab(crate::emoji::EmojiSetTab::Trending)?;
            self.session.stickers.emoji.trending_stale = false;
        }
        if reactions {
            // With no picker open there is nothing to refetch.
            if let Some((chat_id, message_id)) =
                self.session.stickers.message_reaction_options.target()
            {
                self.session.stickers.message_reaction_options =
                    crate::state::MessageReactionsState::Idle;
                self.fetch_message_reactions(chat_id, message_id)?;
            }
            self.session.stickers.reaction_options_stale = false;
        }
        Ok(())
    }

    pub fn close_gif_panel(&mut self) {
        self.session.stickers.gifs.close();
    }

    pub(crate) fn maybe_refresh_saved_animations(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.stickers.gifs.open || !self.chats_path_active() {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetSavedAnimations)
        {
            return Ok(None);
        }
        let needs = !self.session.stickers.gifs.loaded || self.session.stickers.gifs.stale;
        if !needs {
            return Ok(None);
        }
        self.session.stickers.gifs.loading = true;
        self.session.stickers.gifs.stale = false;
        let extra = self
            .session
            .request(RequestPurpose::GetSavedAnimations, None);
        match self.sender.send_json(&get_saved_animations(extra)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.gifs.loading = false;
                self.session.stickers.gifs.loaded = true;
                self.session.stickers.gifs.failed = true;
                Err(err)
            }
        }
    }
}
