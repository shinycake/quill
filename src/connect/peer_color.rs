//! Connect driver: the account's name color, profile color and the reply
//! and profile icons (tdesktop `EditPeerColorBox`).
use super::*;
use crate::ids::{FileId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    get_default_background_custom_emoji_stickers, set_accent_color, set_profile_accent_color,
};

impl<S: JsonSender> ConnectDriver<S> {
    fn my_user_is_premium(&self) -> bool {
        self.session.my_is_premium()
    }

    /// `setAccentColor`: name color and reply icon (0 = none). Telegram
    /// Premium only; the UI opens Premium instead of calling this.
    pub fn set_accent_color(
        &mut self,
        accent_color_id: i32,
        background_custom_emoji_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || accent_color_id < 0
            || background_custom_emoji_id < 0
            || !self.my_user_is_premium()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetAccentColor, None);
        let json = set_accent_color(extra, accent_color_id, background_custom_emoji_id);
        self.send_json_request(extra, &json)
    }

    /// `setProfileAccentColor`: profile color (-1 = none) and profile icon
    /// (0 = none). Telegram Premium only.
    pub fn set_profile_colors(
        &mut self,
        profile_accent_color_id: i32,
        profile_background_custom_emoji_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || profile_accent_color_id < -1
            || profile_background_custom_emoji_id < 0
            || !self.my_user_is_premium()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetProfileAccentColor, None);
        let json = set_profile_accent_color(
            extra,
            profile_accent_color_id,
            profile_background_custom_emoji_id,
        );
        self.send_json_request(extra, &json)
    }

    /// `getDefaultBackgroundCustomEmojiStickers` once (again after a
    /// failure); the answer fills `UsersState::background_emoji_icons`.
    pub fn load_background_emoji_icons(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if (!self.session.users_state.background_emoji_icons.is_empty()
            && !self.session.users_state.background_emoji_icons_failed)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetBackgroundEmojiIcons)
        {
            return Ok(None);
        }
        self.session.users_state.background_emoji_icons_failed = false;
        let extra = self
            .session
            .request(RequestPurpose::GetBackgroundEmojiIcons, None);
        let json = get_default_background_custom_emoji_stickers(extra);
        self.send_json_request(extra, &json).map(Some)
    }

    /// Download the icon thumbnails that are not local yet.
    pub fn download_background_emoji_icons(&mut self) {
        let files: Vec<FileId> = self
            .session
            .users_state
            .background_emoji_icons
            .iter()
            .filter_map(|sticker| sticker.display_file_id())
            .collect();
        for file in files {
            let _ = self.download_file(file, THUMB_DOWNLOAD_PRIORITY);
        }
    }
}
