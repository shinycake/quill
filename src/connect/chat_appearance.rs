//! Connect driver: a channel's or supergroup's appearance (tdesktop
//! `EditPeerColorBox` for channels): name color, profile color, emoji status.
use super::*;
use crate::chat_appearance::ApplyPlan;
use crate::ids::{ChatId, FileId};
use crate::state::RequestPurpose;
use crate::telegram::requests_chat_appearance::{
    get_default_chat_emoji_statuses, set_chat_accent_color, set_chat_emoji_status,
    set_chat_profile_accent_color,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// The viewer may change the look of `chat_id` (a channel or supergroup
    /// they own or administer with the change-info right).
    fn can_edit_chat_appearance(&self, chat_id: ChatId) -> bool {
        self.chats_path_active()
            && self.session.chat_supergroup(chat_id).is_some()
            && self.session.chat_can_change_info(chat_id)
    }

    /// Opening the box: refetch the boost level (it changes in other apps;
    /// tdesktop's `CheckBoostLevel` asks at Apply time), load the level
    /// table, the chat status choices and the icon choices.
    pub fn open_chat_appearance(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.can_edit_chat_appearance(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.appearance_error = None;
        self.session.groups.chat_boost_status.remove(&chat_id.0);
        self.fetch_chat_boost_status(chat_id)?;
        self.fetch_chat_boost_features(chat_id)?;
        self.load_background_emoji_icons()?;
        self.session.stickers.emoji.chat_status_open = true;
        let purpose = RequestPurpose::GetDefaultChatEmojiStatuses;
        if self
            .session
            .stickers
            .emoji
            .chat_default_status_ids
            .is_empty()
            && !self.session.requests.has_purpose(purpose)
        {
            let extra = self.session.request(purpose, None);
            self.send_json_request(extra, &get_default_chat_emoji_statuses(extra))?;
        }
        Ok(())
    }

    /// Closing the box stops resolving its status choices.
    pub fn close_chat_appearance(&mut self) {
        self.session.stickers.emoji.chat_status_open = false;
    }

    /// Send the changed parts of `plan`, one request each. Returns how many
    /// were sent; the chat's updates carry the result.
    pub fn apply_chat_appearance(
        &mut self,
        chat_id: ChatId,
        plan: ApplyPlan,
    ) -> Result<usize, ConnectSendError> {
        if !self.can_edit_chat_appearance(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.appearance_error = None;
        let mut sent = 0;
        if let Some((color, emoji)) = plan.name {
            if color < 0 || emoji < 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
            let extra = self
                .session
                .request(RequestPurpose::SetChatAccentColor, Some(chat_id));
            self.send_json_request(extra, &set_chat_accent_color(extra, chat_id, color, emoji))?;
            sent += 1;
        }
        if let Some((color, emoji)) = plan.profile {
            if color < -1 || emoji < 0 {
                return Err(ConnectSendError::InvalidRequest);
            }
            let extra = self
                .session
                .request(RequestPurpose::SetChatProfileAccentColor, Some(chat_id));
            self.send_json_request(
                extra,
                &set_chat_profile_accent_color(extra, chat_id, color, emoji),
            )?;
            sent += 1;
        }
        if let Some(status) = plan.status {
            if status.is_some_and(|id| id <= 0) {
                return Err(ConnectSendError::InvalidRequest);
            }
            let extra = self
                .session
                .request(RequestPurpose::SetChatEmojiStatus, Some(chat_id));
            self.send_json_request(extra, &set_chat_emoji_status(extra, chat_id, status))?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Download the thumbnails of the status choices that are resolved but
    /// not local yet.
    pub fn download_chat_status_choices(&mut self) {
        let emoji = &self.session.stickers.emoji;
        let files: Vec<FileId> = emoji
            .chat_default_status_ids
            .iter()
            .filter_map(|id| {
                emoji
                    .custom_emoji_stickers
                    .iter()
                    .find(|sticker| sticker.custom_emoji_id == Some(*id))
            })
            .filter_map(|sticker| sticker.display_file_id())
            .collect();
        for file in files {
            let _ = self.download_file(file, THUMB_DOWNLOAD_PRIORITY);
        }
    }
}
