//! Custom emoji pack browser; independent of regular sticker picker state.
use super::*;
use crate::emoji::EmojiSetTab;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::sticker_suggest::suggest_emoji_for;
use crate::telegram::requests::{change_sticker_set, view_trending_sticker_sets};
use crate::telegram::requests_emoji::{
    get_animated_emoji, get_installed_emoji_sets, get_trending_emoji_sets, search_emoji_sets,
};

impl<S: JsonSender> ConnectDriver<S> {
    pub fn load_emoji_status_choices(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.emoji.status_open = true;
        self.session.stickers.emoji.status_note = None;
        self.session
            .stickers
            .emoji
            .status_resolution_attempted
            .clear();
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
            (
                RequestPurpose::GetUpgradedGiftEmojiStatuses,
                crate::telegram::requests_emoji::get_upgraded_gift_emoji_statuses
                    as fn(RequestId) -> String,
            ),
        ] {
            self.emoji_set_request(purpose, build)?;
        }
        Ok(())
    }

    /// The status selector closed: stop resolving its choices.
    pub fn close_emoji_status_choices(&mut self) {
        self.session.stickers.emoji.status_open = false;
    }

    /// Checks a status change and returns the `expiration_date` to send.
    /// Removing a status needs no Premium; setting one does.
    fn status_change_expiration(
        &self,
        custom_emoji_id: Option<i64>,
        duration_secs: i32,
    ) -> Result<i32, ConnectSendError> {
        let premium = self
            .session
            .my_user_id
            .and_then(|id| self.session.user(id))
            .is_some_and(|u| u.is_premium);
        if duration_secs < 0 || custom_emoji_id.is_some_and(|id| id <= 0 || !premium) {
            return Err(ConnectSendError::InvalidRequest);
        }
        if custom_emoji_id.is_none() {
            return Ok(0);
        }
        crate::main_menu::emoji_status_expiration(crate::state::unix_ms_now() / 1000, duration_secs)
            .ok_or(ConnectSendError::InvalidRequest)
    }

    fn status_change_busy(&self) -> bool {
        let requests = &self.session.requests;
        requests.has_purpose(RequestPurpose::SetEmojiStatus)
            || requests.has_purpose(RequestPurpose::ClearRecentEmojiStatuses)
    }

    /// Set a collectible (upgraded gift) status for `duration_secs`
    /// (`0` keeps it). `custom_emoji_id` is the gift model's emoji, the
    /// cell the selector showed. `Ok(None)`: another change is in flight.
    pub fn change_collectible_emoji_status(
        &mut self,
        custom_emoji_id: i64,
        duration_secs: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let gift = self
            .session
            .stickers
            .emoji
            .upgraded_gift_statuses
            .iter()
            .filter_map(|item| item.gift.clone())
            .find(|gift| gift.model_custom_emoji_id == custom_emoji_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let expiration = self.status_change_expiration(Some(custom_emoji_id), duration_secs)?;
        if self.status_change_busy() {
            return Ok(None);
        }
        self.session.stickers.emoji.status_note = None;
        self.session.stickers.emoji.pending_status_emoji = None;
        self.emoji_set_request(RequestPurpose::SetEmojiStatus, |extra| {
            crate::telegram::requests_emoji::set_collectible_emoji_status(extra, &gift, expiration)
        })
    }

    /// `custom_emoji_id: None` removes the status.
    pub fn change_emoji_status(
        &mut self,
        custom_emoji_id: Option<i64>,
        duration_secs: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let expiration = self.status_change_expiration(custom_emoji_id, duration_secs)?;
        if self.status_change_busy() {
            return Ok(None);
        }
        self.session.stickers.emoji.status_note = None;
        let request = self.emoji_set_request(RequestPurpose::SetEmojiStatus, |extra| {
            crate::telegram::requests_emoji::set_emoji_status(extra, custom_emoji_id, expiration)
        })?;
        if request.is_some() {
            self.session.stickers.emoji.pending_status_emoji = custom_emoji_id;
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
        self.session.stickers.emoji.status_note = None;
        self.emoji_set_request(
            RequestPurpose::ClearRecentEmojiStatuses,
            crate::telegram::requests_emoji::clear_recent_emoji_statuses,
        )
    }

    /// `getCustomEmojiStickers` for one custom emoji a mini app suggests as the
    /// emoji status, so its sticker can be checked and shown. `Ok(None)`: the
    /// shared lookup is busy with another batch; ask again on the next tick.
    pub fn resolve_custom_emoji(
        &mut self,
        custom_emoji_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if custom_emoji_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.emoji_set_request(RequestPurpose::GetCustomEmojiStickers, |extra| {
            crate::telegram::requests_emoji::get_custom_emoji_stickers(extra, &[custom_emoji_id])
        })
    }

    pub(crate) fn maybe_resolve_emoji_status_choices(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || !(self.session.stickers.emoji.status_open
                || self.session.stickers.emoji.chat_status_open)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let mut ids: Vec<_> = self
            .session
            .stickers
            .emoji
            .recent_statuses
            .iter()
            .map(|s| s.custom_emoji_id)
            .chain(
                self.session
                    .stickers
                    .emoji
                    .themed_status_ids
                    .iter()
                    .copied(),
            )
            .chain(
                self.session
                    .stickers
                    .emoji
                    .default_status_ids
                    .iter()
                    .copied(),
            )
            .chain(
                self.session
                    .stickers
                    .emoji
                    .chat_default_status_ids
                    .iter()
                    .copied(),
            )
            .chain(
                self.session
                    .stickers
                    .emoji
                    .upgraded_gift_statuses
                    .iter()
                    .filter_map(|item| item.gift.as_ref().map(|g| g.model_custom_emoji_id)),
            )
            .filter(|id| {
                *id > 0
                    && !self
                        .session
                        .stickers
                        .emoji
                        .status_resolution_attempted
                        .contains(id)
                    && !self
                        .session
                        .stickers
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
                .stickers
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
    pub(crate) fn maybe_resolve_message_custom_emoji(
        &mut self,
        open_chat_ids: &[i64],
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active()
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetCustomEmojiStickers)
        {
            return Ok(());
        }
        let resolved = self.session.resolved_custom_emoji_ids();
        let mut ids = self
            .session
            .unresolved_custom_emoji_ids_in(open_chat_ids.to_vec(), &resolved);
        ids.extend(
            self.session
                .settings
                .media_prefs
                .recent_custom_emoji_ids
                .iter()
                .copied()
                .filter(|id| {
                    !self
                        .session
                        .stickers
                        .emoji
                        .status_resolution_attempted
                        .contains(id)
                        && !resolved.contains(id)
                }),
        );
        ids.sort_unstable();
        ids.dedup();
        ids.truncate(200);
        if !ids.is_empty() {
            self.session
                .stickers
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
        self.session.stickers.emoji.failed = false;
        let extra = self.session.request(purpose, None);
        if let Err(err) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            self.session.stickers.emoji.failed = true;
            if matches!(
                purpose,
                RequestPurpose::SetEmojiStatus
                    | RequestPurpose::ClearRecentEmojiStatuses
                    | RequestPurpose::GetRecentEmojiStatuses
                    | RequestPurpose::GetThemedEmojiStatuses
                    | RequestPurpose::GetDefaultEmojiStatuses
                    | RequestPurpose::GetDefaultChatEmojiStatuses
                    | RequestPurpose::GetCustomEmojiStickers
            ) {
                self.session.stickers.emoji.status_note =
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
            .stickers
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
        self.session.stickers.emoji.open = true;
        self.select_emoji_set_tab(EmojiSetTab::Installed)
    }

    pub fn select_emoji_set_tab(
        &mut self,
        tab: EmojiSetTab,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.emoji.tab = tab;
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
                self.session.stickers.emoji.trending_offset = 0;
                self.emoji_set_request(RequestPurpose::GetTrendingEmojiSets, |id| {
                    get_trending_emoji_sets(id, 0, 100)
                })
            }
            EmojiSetTab::Search => Ok(None),
        }
    }

    /// Download the cover previews of the sets the box lists, once each.
    pub fn ensure_emoji_pack_covers(&mut self) -> Result<(), ConnectSendError> {
        let emoji = &mut self.session.stickers.emoji;
        let listed: &[crate::telegram::envelope::StickerSetInfo] = match emoji.tab {
            EmojiSetTab::Installed => &emoji.installed_sets,
            EmojiSetTab::Trending => &emoji.trending_sets,
            EmojiSetTab::Search => &emoji.found_sets,
        };
        let mut files = Vec::new();
        let mut asked = Vec::new();
        for set in listed {
            if emoji.covers_requested.contains(&set.id) {
                continue;
            }
            asked.push(set.id);
            files.extend(
                set.covers
                    .iter()
                    .filter_map(|cover| cover.display_file_id()),
            );
        }
        emoji.covers_requested.extend(asked);
        self.ensure_media_files(&files)
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
        self.session.stickers.emoji.keyword_emojis.clear();
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
        self.session.stickers.emoji.search_query = query.trim().to_string();
        self.session.stickers.emoji.found_sets.clear();
        let query = self.session.stickers.emoji.search_query.clone();
        self.emoji_set_request(RequestPurpose::SearchEmojiSets, |id| {
            search_emoji_sets(id, &query)
        })
    }

    pub fn more_trending_emoji_packs(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.stickers.emoji.trending_has_more {
            return Ok(None);
        }
        let offset = self.session.stickers.emoji.trending_next_offset;
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetTrendingEmojiSets)
        {
            return Ok(None);
        }
        self.session.stickers.emoji.trending_offset = offset;
        self.emoji_set_request(RequestPurpose::GetTrendingEmojiSets, |id| {
            get_trending_emoji_sets(id, offset, 100)
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
        self.session.stickers.emoji.mutation_failed = false;
        let request = self.emoji_set_request(RequestPurpose::ChangeEmojiSet, |id| {
            change_sticker_set(id, set_id, installed, false)
        })?;
        if request.is_some() {
            self.session.stickers.emoji.mutating_set = Some((set_id, installed));
        }
        Ok(request)
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
            || !self.session.stickers.emoji.installed_sets.is_empty()
            || self.session.stickers.emoji.open;
        for purpose in [
            RequestPurpose::SearchEmojiSets,
            RequestPurpose::GetTrendingEmojiSets,
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
        if !self.session.stickers.emoji.open {
            return Ok(());
        }
        match self.session.stickers.emoji.tab {
            EmojiSetTab::Search => {
                let query = self.session.stickers.emoji.search_query.clone();
                self.search_emoji_packs(&query)?;
            }
            EmojiSetTab::Trending => {
                self.select_emoji_set_tab(EmojiSetTab::Trending)?;
            }
            EmojiSetTab::Installed => {}
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
            self.session.stickers.emoji.animated_emoji = None;
            self.session.stickers.emoji.animated_emoji_for = None;
            return Ok(None);
        };
        if self.session.stickers.emoji.animated_emoji_for.as_deref() == Some(emoji) {
            return Ok(None);
        }
        drop(
            self.session
                .requests
                .take_purpose(RequestPurpose::GetAnimatedEmoji),
        );
        self.session.stickers.emoji.animated_emoji = None;
        self.session.stickers.emoji.animated_emoji_for = Some(emoji.to_string());
        let extra = self.session.request(RequestPurpose::GetAnimatedEmoji, None);
        match self.sender.send_json(&get_animated_emoji(extra, emoji)) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                self.session.stickers.emoji.animated_emoji_for = None;
                Err(err)
            }
        }
    }
}
