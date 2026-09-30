//! Connect driver: sticker and GIF panels.
use super::*;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::sticker_suggest::{SUGGEST_LIMIT, StickerSuggestMode, suggest_emoji_for};
use crate::telegram::requests::{
    get_installed_sticker_sets, get_saved_animations, get_sticker_set, search_stickers,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Open the sticker panel and load installed regular sets
    /// (`getInstalledStickerSets` + `stickerTypeRegular`).
    pub fn open_sticker_panel(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.stickers.open = true;
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
        if !self.session.stickers.open || !self.chats_path_active() {
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
        self.session.gifs.failed = false;
        self.maybe_refresh_saved_animations()
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
        let needs = self.session.gifs.animations.is_empty() || self.session.gifs.stale;
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
                Err(err)
            }
        }
    }
}
