//! Custom emoji pack browser; independent of regular sticker picker state.
use super::*;
use crate::emoji::EmojiSetTab;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::sticker_suggest::suggest_emoji_for;
use crate::telegram::requests::{change_sticker_set, get_sticker_set, view_trending_sticker_sets};
use crate::telegram::requests_emoji::{
    get_animated_emoji, get_installed_emoji_sets, get_trending_emoji_sets, search_emoji_sets,
};

impl<S: JsonSender> ConnectDriver<S> {
    pub fn load_emoji_status_choices(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.emoji.status_open = true;
        self.session.emoji.status_note = None;
        self.session.emoji.status_resolution_attempted.clear();
        for (purpose, build) in [
            (
                RequestPurpose::GetRecentEmojiStatuses,
                crate::telegram::requests_emoji::get_recent_emoji_statuses
                    as fn(RequestId) -> String,
            ),
            (
                RequestPurpose::GetThemedEmojiStatuses,
                crate::telegram::requests_emoji::get_themed_emoji_statuses
                    as fn(RequestId) -> String,
            ),
            (
                RequestPurpose::GetDefaultEmojiStatuses,
                crate::telegram::requests_emoji::get_default_emoji_statuses
                    as fn(RequestId) -> String,
            ),
        ] {
            self.emoji_set_request(purpose, build)?;
        }
        Ok(())
    }

    pub fn change_emoji_status(
        &mut self,
        custom_emoji_id: Option<i64>,
        duration_secs: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if duration_secs < 0
            || custom_emoji_id.is_some_and(|id| id <= 0)
            || !self
                .session
                .my_user_id
                .and_then(|id| self.session.user(id))
                .is_some_and(|u| u.is_premium)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::SetEmojiStatus)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::ClearRecentEmojiStatuses)
        {
            return Ok(None);
        }
        let expiration = if duration_secs == 0 || custom_emoji_id.is_none() {
            0
        } else {
            let now = crate::state::unix_ms_now() / 1000;
            i32::try_from(
                now.checked_add(duration_secs as u64)
                    .ok_or(ConnectSendError::InvalidRequest)?,
            )
            .map_err(|_| ConnectSendError::InvalidRequest)?
        };
        self.session.emoji.status_note = None;
        let request = self.emoji_set_request(RequestPurpose::SetEmojiStatus, |extra| {
            crate::telegram::requests_emoji::set_emoji_status(extra, custom_emoji_id, expiration)
        })?;
        if request.is_some() {
            self.session.emoji.pending_status_emoji = custom_emoji_id;
        }
        Ok(request)
    }

    pub fn clear_recent_emoji_statuses(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::SetEmojiStatus)
        {
            return Ok(None);
        }
        self.session.emoji.status_note = None;
        self.emoji_set_request(
            RequestPurpose::ClearRecentEmojiStatuses,
            crate::telegram::requests_emoji::clear_recent_emoji_statuses,
        )
    }

    pub(crate) fn maybe_resolve_emoji_status_choices(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || !self.session.emoji.open
            || !self.session.emoji.status_open
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let mut ids: Vec<_> = self
            .session
            .emoji
            .recent_statuses
            .iter()
            .map(|s| s.custom_emoji_id)
            .chain(self.session.emoji.themed_status_ids.iter().copied())
            .chain(self.session.emoji.default_status_ids.iter().copied())
            .filter(|id| {
                *id > 0
                    && !self.session.emoji.status_resolution_attempted.contains(id)
                    && !self
                        .session
                        .emoji
                        .custom_emoji_stickers
                        .iter()
                        .any(|s| s.custom_emoji_id == Some(*id))
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(200);
        if !ids.is_empty() {
            self.session
                .emoji
                .status_resolution_attempted
                .extend(ids.iter().copied());
            self.emoji_set_request(RequestPurpose::GetCustomEmojiStickers, |extra| {
                crate::telegram::requests_emoji::get_custom_emoji_stickers(extra, &ids)
            })?;
        }
        Ok(())
    }

    /// Resolve custom emoji ids referenced by the open chat's message text.
    /// Unlike the status-panel variant this has no panel-open gate — message
    /// text renders in the normal chat view. Shares the status pipeline's
    /// purpose, cache, and attempted-set.
    pub(crate) fn maybe_resolve_message_custom_emoji(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let mut ids = self.session.message_custom_emoji_ids_to_resolve();
        ids.extend(
            self.session
                .settings
                .media_prefs
                .recent_custom_emoji_ids
                .iter()
                .copied()
                .filter(|id| {
                    !self.session.emoji.status_resolution_attempted.contains(id)
                        && !self
                            .session
                            .emoji
                            .custom_emoji_stickers
                            .iter()
                            .any(|item| item.custom_emoji_id == Some(*id))
                }),
        );
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(200);
        if !ids.is_empty() {
            self.session
                .emoji
                .status_resolution_attempted
                .extend(ids.iter().copied());
            self.emoji_set_request(RequestPurpose::GetCustomEmojiStickers, |extra| {
                crate::telegram::requests_emoji::get_custom_emoji_stickers(extra, &ids)
            })?;
        }
        Ok(())
    }

    fn emoji_set_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if purpose != RequestPurpose::ViewTrendingEmojiSets
            && self.session.requests.has_purpose(purpose)
        {
            return Ok(None);
        }
        self.session.emoji.failed = false;
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            self.session.emoji.failed = true;
            if matches!(
                purpose,
                RequestPurpose::SetEmojiStatus
                    | RequestPurpose::ClearRecentEmojiStatuses
                    | RequestPurpose::GetRecentEmojiStatuses
                    | RequestPurpose::GetThemedEmojiStatuses
                    | RequestPurpose::GetDefaultEmojiStatuses
                    | RequestPurpose::GetCustomEmojiStickers
            ) {
                self.session.emoji.status_note =
                    Some("Could not update emoji statuses. Retry the action.".into());
            }
            return Err(err);
        }
        Ok(Some(extra))
    }

    pub(crate) fn mark_emoji_packs_viewed(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let ids: Vec<_> = self
            .session
            .emoji
            .trending_sets
            .iter()
            .map(|set| set.id)
            .collect();
        if ids.is_empty() {
            return Ok(None);
        }
        self.emoji_set_request(RequestPurpose::ViewTrendingEmojiSets, |id| {
            view_trending_sticker_sets(id, &ids)
        })
    }

    pub fn open_emoji_sets(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.emoji.open = true;
        self.select_emoji_set_tab(EmojiSetTab::Installed)
    }

    pub fn select_emoji_set_tab(
        &mut self,
        tab: EmojiSetTab,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.emoji.tab = tab;
        self.session.emoji.selected_set_id = None;
        self.session.emoji.preview.clear();
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetEmojiSet),
        );
        match tab {
            EmojiSetTab::Installed => self.emoji_set_request(
                RequestPurpose::GetInstalledEmojiSets,
                get_installed_emoji_sets,
            ),
            EmojiSetTab::Trending => {
                drop(
                    self.session
                        .requests
                        .take_purpose(RequestPurpose::GetTrendingEmojiSets),
                );
                self.session.emoji.trending_offset = 0;
                self.emoji_set_request(RequestPurpose::GetTrendingEmojiSets, |id| {
                    get_trending_emoji_sets(id, 0, 100)
                })
            }
            EmojiSetTab::Search => Ok(None),
        }
    }

    /// B11: the panel's emoji search also asks `getKeywordEmojis`, in the
    /// language of what was typed, the system language and English; the
    /// matches join the catalog's. A newer query replaces the pending one.
    pub fn search_keyword_emojis(&mut self, query: &str) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetKeywordEmojis),
        );
        self.session.emoji.keyword_emojis.clear();
        let query = query.trim();
        if query.is_empty() {
            return Ok(());
        }
        let codes =
            crate::emoji::keyword_language_codes(query, crate::emoji::system_locale().as_deref());
        let extra = self.session.request(RequestPurpose::GetKeywordEmojis, None);
        if let Err(err) = self
            .sender
            .send_json(&crate::telegram::requests::get_keyword_emojis(
                extra, query, &codes,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }

    pub fn search_emoji_packs(
        &mut self,
        query: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.select_emoji_set_tab(EmojiSetTab::Search)?;
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::SearchEmojiSets),
        );
        self.session.emoji.search_query = query.trim().to_string();
        self.session.emoji.found_sets.clear();
        let query = self.session.emoji.search_query.clone();
        self.emoji_set_request(RequestPurpose::SearchEmojiSets, |id| {
            search_emoji_sets(id, &query)
        })
    }

    pub fn more_trending_emoji_packs(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.emoji.trending_has_more {
            return Ok(None);
        }
        let offset = self.session.emoji.trending_next_offset;
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetTrendingEmojiSets)
        {
            return Ok(None);
        }
        self.session.emoji.trending_offset = offset;
        self.emoji_set_request(RequestPurpose::GetTrendingEmojiSets, |id| {
            get_trending_emoji_sets(id, offset, 100)
        })
    }

    pub fn preview_emoji_pack(
        &mut self,
        set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || set_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetEmojiSet),
        );
        self.session.emoji.selected_set_id = Some(set_id);
        self.session.emoji.preview.clear();
        self.session.emoji.preview_title.clear();
        self.emoji_set_request(RequestPurpose::GetEmojiSet, |id| {
            get_sticker_set(id, set_id)
        })
    }

    pub fn set_emoji_pack_installed(
        &mut self,
        set_id: i64,
        installed: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || set_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.emoji.mutation_failed = false;
        let request = self.emoji_set_request(RequestPurpose::ChangeEmojiSet, |id| {
            change_sticker_set(id, set_id, installed, false)
        })?;
        if request.is_some() {
            self.session.emoji.mutating_set = Some((set_id, installed));
        }
        Ok(request)
    }

    pub fn download_emoji_pack(&mut self, set_id: i64) -> Result<(), ConnectSendError> {
        if self.session.emoji.outdated_packs.contains(&set_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let ids = self
            .session
            .emoji
            .pack_files
            .get(&set_id)
            .cloned()
            .ok_or(ConnectSendError::InvalidRequest)?;
        for id in ids {
            self.download_file(id, THUMB_DOWNLOAD_PRIORITY)?;
        }
        Ok(())
    }

    pub(crate) fn refresh_emoji_pack_catalog(&mut self) -> Result<(), ConnectSendError> {
        // TDLib sends this update while it loads the list a request asked
        // for, so a dropped in-flight request is sent again; a cached list
        // (the composer panel's) is refreshed too.
        let installed_wanted = self
            .session
            .requests
            .take_purpose(RequestPurpose::GetInstalledEmojiSets)
            .is_some()
            || !self.session.emoji.installed_sets.is_empty()
            || self.session.emoji.open;
        for purpose in [
            RequestPurpose::SearchEmojiSets,
            RequestPurpose::GetTrendingEmojiSets,
            RequestPurpose::GetEmojiSet,
        ] {
            drop(self.session.requests.take_purpose(purpose));
        }
        if !self.chats_path_active() {
            return Ok(());
        }
        if installed_wanted {
            self.emoji_set_request(
                RequestPurpose::GetInstalledEmojiSets,
                get_installed_emoji_sets,
            )?;
        }
        if !self.session.emoji.open {
            return Ok(());
        }
        match self.session.emoji.tab {
            EmojiSetTab::Search => {
                let query = self.session.emoji.search_query.clone();
                self.search_emoji_packs(&query)?;
            }
            EmojiSetTab::Trending => {
                self.select_emoji_set_tab(EmojiSetTab::Trending)?;
            }
            EmojiSetTab::Installed => {
                self.session.emoji.selected_set_id = None;
                self.session.emoji.preview.clear();
            }
        }
        Ok(())
    }

    /// Suggest-animated-emoji: refresh the composer's animated emoji
    /// suggestion for the current composer text. No trailing emoji
    /// clears the suggestion; an unchanged emoji is not re-requested;
    /// a stale in-flight `GetAnimatedEmoji` is dropped before the new
    /// one goes out, so a late answer can never land under a newer
    /// emoji. Returns the issued `RequestId` when a `getAnimatedEmoji`
    /// went out. The UI calls this on composer text change.
    pub fn update_animated_emoji_suggestion(
        &mut self,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(emoji) = suggest_emoji_for(text) else {
            drop(
                self.session
                    .requests
                    .take_purpose(RequestPurpose::GetAnimatedEmoji),
            );
            self.session.emoji.animated_emoji = None;
            self.session.emoji.animated_emoji_for = None;
            return Ok(None);
        };
        if self.session.emoji.animated_emoji_for.as_deref() == Some(emoji) {
            return Ok(None);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetAnimatedEmoji),
        );
        self.session.emoji.animated_emoji = None;
        self.session.emoji.animated_emoji_for = Some(emoji.to_string());
        let extra = self.session.request(RequestPurpose::GetAnimatedEmoji, None);
        match self.sender.send_json(&get_animated_emoji(extra, emoji)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.emoji.animated_emoji_for = None;
                Err(err)
            }
        }
    }
}
