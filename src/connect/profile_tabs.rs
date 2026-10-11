//! Connect driver: the shared-media gallery's profile tabs (Stories, Gifts,
//! Playlist), their counts for the tab bar, and the Photos / Videos split.
use super::*;
use crate::ids::ChatId;
use crate::state::{
    MediaPurpose, RequestPurpose, SharedMediaTab, SharedMediaTabStatus, UsersPurpose,
};
use crate::telegram::envelope::{ChatKind, MessageSender};
use crate::telegram::requests::{EXTRA_COUNT_FILTERS, get_user_profile_audios};
use crate::telegram::requests_premium::get_received_gifts;

/// Rows per page of the Stories, Gifts and Playlist tabs.
const PROFILE_PAGE: i32 = 30;

impl<S: JsonSender> ConnectDriver<S> {
    /// The user (for saved music) and the gifts owner of a chat, when its
    /// kind has them.
    fn profile_owner(&self, chat_id: ChatId) -> Option<(Option<i64>, MessageSender)> {
        let chat = self.session.chats.get(&chat_id.0)?;
        match chat.kind {
            ChatKind::Private { user_id } if user_id.0 > 0 => {
                Some((Some(user_id.0), MessageSender::User { user_id: user_id.0 }))
            }
            ChatKind::Supergroup {
                is_channel: true, ..
            } => Some((None, MessageSender::Chat { chat_id: chat.id.0 })),
            _ => None,
        }
    }

    /// Fetch the first page of the open gallery's profile tabs and the
    /// message counts, so the tab bar knows which tabs to show. Tabs that
    /// were fetched before are left alone.
    pub fn fetch_profile_tabs(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.media.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        self.fetch_gallery_counts(chat_id)?;
        let Some(kind) = self.session.chats.get(&chat_id.0).map(|c| c.kind.clone()) else {
            return Ok(());
        };
        for tab in [
            SharedMediaTab::Stories,
            SharedMediaTab::Gifts,
            SharedMediaTab::SavedMusic,
        ] {
            let idle = self.session.media.shared_media.tabs[tab.index()].status
                == SharedMediaTabStatus::Idle;
            if tab.available_in(&kind) && idle {
                // A failed send leaves that tab hidden; the others still load.
                let _ = self.fetch_profile_tab(tab, false);
            }
        }
        Ok(())
    }

    /// The info panel's media counts plus the gallery's extra filters
    /// (`EXTRA_COUNT_FILTERS`), once per chat per session.
    fn fetch_gallery_counts(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        let first_time = !self
            .session
            .media
            .chat_media_counts
            .contains_key(&chat_id.0);
        self.fetch_chat_media_counts(chat_id)?;
        if !first_time {
            return Ok(());
        }
        let base = crate::telegram::requests::MEDIA_COUNT_FILTERS.len() as u8;
        for (index, filter) in EXTRA_COUNT_FILTERS.iter().enumerate() {
            let purpose = RequestPurpose::Users(UsersPurpose::GetChatMessageCount {
                filter: base + index as u8,
            });
            let extra = self.session.request(purpose, Some(chat_id));
            let json = crate::telegram::requests::get_chat_message_count(extra, chat_id, filter);
            if let Err(err) = self.sender.send_json(&json) {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        Ok(())
    }

    /// One page of Stories, Gifts or Playlist for the open gallery.
    /// `append` asks for the page after the loaded ones.
    pub fn fetch_profile_tab(
        &mut self,
        tab: SharedMediaTab,
        append: bool,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || tab.lists_messages() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat_id) = self.session.media.shared_media.chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let Some((user_id, owner)) = self.profile_owner(chat_id) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if append {
            if !self.session.media.shared_media.begin_profile_more(tab) {
                return Ok(());
            }
        } else {
            self.session.media.shared_media.begin_profile_fetch(tab);
        }
        let state = &self.session.media.shared_media.tabs[tab.index()];
        let loaded = state.items.len() as i32;
        let next_offset = state.next_offset.clone();
        let result = match tab {
            SharedMediaTab::Stories => {
                let from = if append {
                    self.session
                        .stories
                        .chat_page_stories
                        .get(&chat_id.0)
                        .and_then(|page| page.story_ids.iter().copied().min())
                        .unwrap_or(0)
                } else {
                    0
                };
                self.get_chat_posted_to_chat_page_stories(chat_id, from, PROFILE_PAGE)
                    .map(|_| ())
            }
            SharedMediaTab::Gifts => {
                let extra = self.session.request(
                    RequestPurpose::Media(MediaPurpose::GetProfileGifts { append }),
                    Some(chat_id),
                );
                let offset = if append { next_offset.as_str() } else { "" };
                let json = get_received_gifts(extra, owner, offset, PROFILE_PAGE);
                self.send_profile_request(extra, &json)
            }
            SharedMediaTab::SavedMusic => {
                let Some(user_id) = user_id else {
                    return Err(ConnectSendError::InvalidRequest);
                };
                let extra = self.session.request(
                    RequestPurpose::Media(MediaPurpose::GetProfileAudios { append }),
                    Some(chat_id),
                );
                let offset = if append { loaded } else { 0 };
                let json = get_user_profile_audios(extra, user_id, offset, PROFILE_PAGE);
                self.send_profile_request(extra, &json)
            }
            _ => Err(ConnectSendError::InvalidRequest),
        };
        if result.is_err() {
            self.session
                .fail_profile_tab(chat_id, tab, "Could not send the request.".to_string());
        }
        result
    }

    fn send_profile_request(
        &mut self,
        extra: crate::ids::RequestId,
        json: &str,
    ) -> Result<(), ConnectSendError> {
        let sent = self.sender.send_json(json);
        if sent.is_err() {
            self.session.requests.take(extra);
        }
        sent
    }

    /// Show Photos and Videos as two tabs (`split`) or as one Media tab,
    /// fetching the tab the gallery moves to.
    pub fn set_shared_media_split(&mut self, split: bool) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let counts = self
            .session
            .media
            .shared_media
            .chat_id
            .and_then(|id| self.session.media.chat_media_counts.get(&id.0))
            .cloned();
        let moved = self
            .session
            .media
            .shared_media
            .set_media_split(split, counts.as_ref());
        if let Some((tab, true)) = moved {
            self.fetch_shared_media(tab)?;
        }
        Ok(())
    }
}
