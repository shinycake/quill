//! GIF search and saved collection controls.
use super::*;
use crate::ids::{ChatId, FileId, RequestId};
use crate::state::RequestPurpose;
use crate::state::StickersPurpose;
use crate::telegram::requests::{
    add_saved_animation, get_inline_query_results, remove_saved_animation, search_public_chat,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn gif_request(
        &mut self,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.requests.has_purpose(purpose) {
            return Ok(None);
        }
        let search = matches!(
            purpose,
            RequestPurpose::ResolveGifSearchBot
                | RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { .. })
        );
        if search {
            self.session.gifs.search_failed = false;
        } else {
            self.session.gifs.failed = false;
        }
        let extra = self.session.request(purpose, None);
        if let Err(error) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            if search {
                self.session.gifs.search_failed = true;
                self.session.gifs.search_loading = false;
            } else {
                self.session.gifs.failed = true;
            }
            return Err(error);
        }
        Ok(Some(extra))
    }

    pub(crate) fn cancel_gif_search_requests(&mut self) {
        for purpose in [
            RequestPurpose::ResolveGifSearchBot,
            RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { first_page: true }),
            RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { first_page: false }),
        ] {
            drop(self.session.requests.take_purpose(purpose));
        }
    }

    pub fn search_gifs(&mut self, query: &str) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.cancel_gif_search_requests();
        self.session.gifs.search_query = query.trim().to_string();
        self.session.gifs.search_results.clear();
        self.session.gifs.search_next_offset.clear();
        self.session.gifs.search_offset.clear();
        self.session.gifs.search_mode = true;
        self.session.gifs.search_loading = true;
        self.maybe_search_gifs(false)
    }

    pub fn show_saved_gifs(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.cancel_gif_search_requests();
        self.session.gifs.search_mode = false;
        self.session.gifs.search_loading = false;
        self.session.gifs.loaded = false;
        self.maybe_refresh_saved_animations()
    }

    pub(crate) fn maybe_search_gifs(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.gifs.search_mode {
            return Ok(None);
        }
        let username = self.session.gifs.search_bot_username.clone();
        if username.is_empty() {
            self.session.gifs.search_failed = true;
            self.session.gifs.search_loading = false;
            return Ok(None);
        }
        if self.session.requests.has_purpose(RequestPurpose::Stickers(
            StickersPurpose::GetGifSearchResults { first_page: true },
        )) || self.session.requests.has_purpose(RequestPurpose::Stickers(
            StickersPurpose::GetGifSearchResults { first_page: false },
        )) {
            return Ok(None);
        }
        let Some(bot_id) = self.session.gifs.search_bot_user_id else {
            return self.gif_request(RequestPurpose::ResolveGifSearchBot, |id| {
                search_public_chat(id, &username)
            });
        };
        let offset = if more {
            self.session.gifs.search_next_offset.clone()
        } else {
            String::new()
        };
        if more && offset.is_empty() {
            return Ok(None);
        }
        let query = self.session.gifs.search_query.clone();
        let chat_id = self.session.open_chat.unwrap_or(ChatId(0));
        self.session.gifs.search_loading = true;
        self.session.gifs.search_offset = offset.clone();
        self.gif_request(
            RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { first_page: !more }),
            |id| get_inline_query_results(id, bot_id, chat_id, &query, &offset),
        )
    }

    pub fn more_gif_search_results(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.maybe_search_gifs(true)
    }

    pub fn set_gif_saved(
        &mut self,
        file_id: FileId,
        saved: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || file_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::AddSavedAnimation)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::RemoveSavedAnimation)
        {
            return Ok(None);
        }
        let purpose = if saved {
            RequestPurpose::AddSavedAnimation
        } else {
            RequestPurpose::RemoveSavedAnimation
        };
        self.gif_request(purpose, |id| {
            if saved {
                add_saved_animation(id, file_id)
            } else {
                remove_saved_animation(id, file_id)
            }
        })
    }
}
