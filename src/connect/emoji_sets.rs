//! Custom emoji pack browser; independent of regular sticker picker state.
use super::*;
use crate::emoji::EmojiSetTab;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::telegram::requests::{change_sticker_set, get_sticker_set, view_trending_sticker_sets};
use crate::telegram::requests_emoji::{
    get_installed_emoji_sets, get_trending_emoji_sets, search_emoji_sets,
};

impl<S: JsonSender> ConnectDriver<S> {
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
        self.emoji_set_request(RequestPurpose::ChangeEmojiSet, |id| {
            change_sticker_set(id, set_id, installed, false)
        })
    }

    pub(crate) fn refresh_emoji_pack_catalog(&mut self) -> Result<(), ConnectSendError> {
        for purpose in [
            RequestPurpose::GetInstalledEmojiSets,
            RequestPurpose::SearchEmojiSets,
            RequestPurpose::GetTrendingEmojiSets,
            RequestPurpose::GetEmojiSet,
        ] {
            drop(self.session.requests.take_purpose(purpose));
        }
        if !self.chats_path_active() || !self.session.emoji.open {
            return Ok(());
        }
        self.emoji_set_request(
            RequestPurpose::GetInstalledEmojiSets,
            get_installed_emoji_sets,
        )?;
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
}
