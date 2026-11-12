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
        let search = matches!(
            purpose,
            RequestPurpose::ResolveGifSearchBot
                | RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { .. })
        );
        // The search waits on its own request; a save or remove on the one
        // this driver sent.
        let busy = if search {
            self.gif_search_out()
        } else {
            self.asked.is_out(&self.session.requests, purpose)
        };
        if busy {
            return Ok(None);
        }
        if search {
            self.session.stickers.gifs.search_failure_cleared();
        } else {
            self.session.stickers.gifs.saved_failure_cleared();
        }
        let extra = self.session.request(purpose, None);
        if !search {
            self.asked.sent(&self.session.requests, purpose, extra);
        }
        if let Err(error) = self.sender.send_json(&build(extra)) {
            self.session.requests.take(extra);
            if search {
                self.session.stickers.gifs.search_failed_now();
            } else {
                self.session.stickers.gifs.saved_mutation_failed();
            }
            return Err(error);
        }
        if search {
            self.session.stickers.gifs.search_sent(extra);
        }
        Ok(Some(extra))
    }

    /// The request the search waits for (the bot's lookup or a results page)
    /// is still out: one the registry no longer holds was answered.
    fn gif_search_out(&self) -> bool {
        self.session
            .stickers
            .gifs
            .search_request()
            .is_some_and(|request| self.session.requests.get(request).is_some())
    }

    pub(crate) fn cancel_gif_search_requests(&mut self) {
        let asked = self.session.stickers.gifs.search_request();
        self.drop_request(asked);
    }

    pub fn search_gifs(
        &mut self,
        query: &str,
        chat: Option<ChatId>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.cancel_gif_search_requests();
        self.session.stickers.gifs.search_chat = chat;
        self.session
            .stickers
            .gifs
            .search_begun(query.trim().to_string());
        self.maybe_search_gifs(false)
    }

    pub fn show_saved_gifs(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.cancel_gif_search_requests();
        self.session.stickers.gifs.search_left();
        self.session.stickers.gifs.saved_forgotten();
        self.maybe_refresh_saved_animations()
    }

    pub(crate) fn maybe_search_gifs(
        &mut self,
        more: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.stickers.gifs.search_mode() {
            return Ok(None);
        }
        let username = self.session.stickers.gifs.search_bot_username.clone();
        if username.is_empty() {
            self.session.stickers.gifs.search_failed_now();
            return Ok(None);
        }
        if self.gif_search_out() {
            return Ok(None);
        }
        let Some(bot_id) = self.session.stickers.gifs.search_bot_user_id else {
            return self.gif_request(RequestPurpose::ResolveGifSearchBot, |id| {
                search_public_chat(id, &username)
            });
        };
        let offset = if more {
            self.session.stickers.gifs.search_next_offset().to_string()
        } else {
            String::new()
        };
        if more && offset.is_empty() {
            return Ok(None);
        }
        let query = self.session.stickers.gifs.search_query().to_string();
        let chat_id = self
            .session
            .stickers
            .gifs
            .search_chat
            .or(self.session.open_chat_id())
            .unwrap_or(ChatId(0));
        self.session.stickers.gifs.search_page_asked(offset.clone());
        let sent = self.gif_request(
            RequestPurpose::Stickers(StickersPurpose::GetGifSearchResults { first_page: !more }),
            |id| get_inline_query_results(id, bot_id, chat_id, &query, &offset),
        )?;
        if let Some(request) = sent {
            self.session.stickers.gifs.search_sent(request);
        }
        Ok(sent)
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
        let requests = &self.session.requests;
        if self
            .asked
            .is_out(requests, RequestPurpose::AddSavedAnimation)
            || self
                .asked
                .is_out(requests, RequestPurpose::RemoveSavedAnimation)
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
