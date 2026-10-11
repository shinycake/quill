//! Connect driver: the emoji avatar and the video avatar
//! (`setProfilePhoto` with `inputChatPhotoSticker` / `inputChatPhotoAnimation`).
use super::*;
use crate::ids::{FileId, RequestId};
use crate::profile_photo_sources::{
    PhotoSticker, get_default_profile_photo_custom_emoji_stickers, set_profile_photo_animation,
    set_profile_photo_sticker,
};
use crate::state::RequestPurpose;

impl<S: JsonSender> ConnectDriver<S> {
    /// A custom emoji or sticker on a background as the main profile photo.
    pub fn set_profile_photo_sticker(
        &mut self,
        sticker: PhotoSticker,
        colors: &[u32],
    ) -> Result<RequestId, ConnectSendError> {
        let valid = match sticker {
            PhotoSticker::CustomEmoji { custom_emoji_id } => custom_emoji_id > 0,
            PhotoSticker::RegularOrMask {
                sticker_set_id,
                sticker_id,
            } => sticker_set_id > 0 && sticker_id > 0,
        };
        if !self.chats_path_active() || !valid || colors.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetProfilePhoto, None);
        let json = set_profile_photo_sticker(extra, sticker, colors, false);
        self.send_json_request(extra, &json)
    }

    /// An encoded avatar clip as the main profile photo; `main_frame` is
    /// seconds into the clip.
    pub fn set_profile_photo_video(
        &mut self,
        path: &str,
        main_frame: f64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || path.is_empty() || !main_frame.is_finite() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetProfilePhoto, None);
        let json = set_profile_photo_animation(extra, path, main_frame.max(0.0), false);
        self.send_json_request(extra, &json)
    }

    /// `getDefaultProfilePhotoCustomEmojiStickers` once (again after a
    /// failure); the answer fills `UsersState::profile_photo_emoji_icons`.
    pub fn load_profile_photo_emoji_icons(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let users = &self.session.users_state;
        if (!users.profile_photo_emoji_icons.is_empty() && !users.profile_photo_emoji_icons_failed)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetProfilePhotoEmojiIcons)
        {
            return Ok(None);
        }
        self.session.users_state.profile_photo_emoji_icons_failed = false;
        let extra = self
            .session
            .request(RequestPurpose::GetProfilePhotoEmojiIcons, None);
        let json = get_default_profile_photo_custom_emoji_stickers(extra);
        self.send_json_request(extra, &json).map(Some)
    }

    /// Download the builder's emoji thumbnails that are not local yet.
    pub fn download_profile_photo_emoji_icons(&mut self) {
        let files: Vec<FileId> = self
            .session
            .users_state
            .profile_photo_emoji_icons
            .iter()
            .filter_map(|sticker| sticker.display_file_id())
            .collect();
        for file in files {
            let _ = self.download_file(file, THUMB_DOWNLOAD_PRIORITY);
        }
    }
}
