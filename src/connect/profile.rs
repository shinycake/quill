//! Connect driver: own profile.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    check_chat_username, delete_profile_photo, reorder_active_usernames, set_bio, set_name,
    set_profile_accent_color, set_profile_photo, set_username, toggle_username_is_active,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// A5: `setName` (schema 1.8.67, line 14823). Best-effort: the name
    /// refreshes via `updateUser`; failures surface in
    /// `Session::profile_edit_error`.
    pub fn set_name(
        &mut self,
        first_name: &str,
        last_name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetName, None);
        let json = set_name(extra, first_name, last_name);
        self.send_json_request(extra, &json)
    }

    /// A5: `setBio` (schema 1.8.67, line 14826).
    pub fn set_bio(&mut self, bio: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetBio, None);
        let json = set_bio(extra, bio);
        self.send_json_request(extra, &json)
    }

    /// A5: `setUsername` (schema 1.8.67, line 14830). Changes the
    /// editable username; empty string removes it.
    pub fn set_username(&mut self, username: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetUsername, None);
        let json = set_username(extra, username);
        self.send_json_request(extra, &json)
    }

    /// A5: `checkChatUsername` for the current user's own username (schema
    /// 1.8.67, line 11677; the private chat with self is the documented
    /// chat id — TGX `EditUsernameController` sends it with
    /// `tdlib.selfChatId()`). The verdict lands in
    /// `Session::username_check`; the in-flight text in
    /// `Session::username_check_pending` so the dialog can ignore stale
    /// verdicts.
    pub fn check_username(&mut self, username: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(me) = self.session.my_user_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self.session.request(RequestPurpose::CheckUsername, None);
        self.session.username_check_pending = Some(username.to_string());
        let json = check_chat_username(extra, ChatId(me), username);
        let sent = self.send_json_request(extra, &json);
        if sent.is_err() {
            // Don't leave the dialog showing "Checking…" for a request
            // that never went out.
            self.session.username_check_pending = None;
        }
        sent
    }

    /// A5: `reorderActiveUsernames` (schema 1.8.67, line 14838) — the
    /// full active list in the new order.
    pub fn reorder_active_usernames(
        &mut self,
        usernames: &[String],
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ReorderActiveUsernames, None);
        let json = reorder_active_usernames(extra, usernames);
        self.send_json_request(extra, &json)
    }

    /// A5: `toggleUsernameIsActive` (schema 1.8.67, line 14835).
    pub fn toggle_username_is_active(
        &mut self,
        username: &str,
        is_active: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleUsernameIsActive, None);
        let json = toggle_username_is_active(extra, username, is_active);
        self.send_json_request(extra, &json)
    }

    /// A5: `setProfilePhoto` with `inputChatPhotoStatic` / `inputFileLocal`
    /// (schema 1.8.67, lines 14803/1042/325). `is_public` is hard-coded
    /// false: this edits the main photo, not the public one (which stays
    /// visible even when the main photo is hidden by privacy settings).
    pub fn set_profile_photo(&mut self, photo_path: &str) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetProfilePhoto, None);
        let json = set_profile_photo(extra, photo_path, false);
        self.send_json_request(extra, &json)
    }

    /// A5: `deleteProfilePhoto` (schema 1.8.67, line 14806).
    pub fn delete_profile_photo(
        &mut self,
        profile_photo_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::DeleteProfilePhoto, None);
        let json = delete_profile_photo(extra, profile_photo_id);
        self.send_json_request(extra, &json)
    }

    /// Slice A12: `setProfileAccentColor` (schema 1.8.67, line 14820).
    /// The current `profile_background_custom_emoji_id` is preserved —
    /// Quill has no background-emoji picker (separate unchecked concern).
    pub fn set_profile_accent_color(
        &mut self,
        profile_accent_color_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let background_emoji_id = self
            .session
            .my_user_id
            .and_then(|me| self.session.user(me))
            .map(|u| u.profile_background_custom_emoji_id)
            .unwrap_or(0);
        let extra = self
            .session
            .request(RequestPurpose::SetProfileAccentColor, None);
        let json = set_profile_accent_color(extra, profile_accent_color_id, background_emoji_id);
        self.send_json_request(extra, &json)
    }
}
