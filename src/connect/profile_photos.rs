//! Connect driver: the profile photo viewer's pages and actions for users,
//! groups and channels (tdesktop `OverlayWidget` over `UserPhotos` and
//! `SharedMediaType::ChatPhoto`).
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::{ChatPhotosFetch, PhotoDelete, PhotoOwner, ProfilePhotosFetch, RequestPurpose};
use crate::telegram::envelope::ChatKind;
use crate::telegram::requests::{
    delete_messages, get_basic_group_full_info, get_supergroup_full_info, get_user_profile_photos,
    report_chat_photo, search_chat_photos, set_chat_photo_from_file,
};

/// `searchChatMessages` page size for a chat's photo history (schema: up
/// to 100).
pub const CHAT_PHOTOS_LIMIT: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    /// Load what a chat's photo viewer shows first: the current photo
    /// (`getSupergroupFullInfo` / `getBasicGroupFullInfo`, answered from
    /// TDLib's cache) and the first page of the photo history. Nothing is
    /// sent for what is known or on its way; a failed history is retried.
    pub fn fetch_chat_photos(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let kind = self
            .session
            .chats
            .get(&chat_id.0)
            .map(|chat| chat.kind.clone());
        if !self
            .session
            .users_state
            .chat_current_photos
            .contains_key(&chat_id.0)
        {
            match kind {
                Some(ChatKind::Supergroup { supergroup_id, .. }) => {
                    let purpose = RequestPurpose::GetSupergroupFullInfo;
                    if !self
                        .session
                        .requests
                        .has_purpose_for_supergroup(purpose, supergroup_id)
                    {
                        let extra = self.session.request_for_supergroup(purpose, supergroup_id);
                        self.send_json_request(
                            extra,
                            &get_supergroup_full_info(extra, supergroup_id),
                        )?;
                    }
                }
                Some(ChatKind::BasicGroup { basic_group_id }) => {
                    let purpose = RequestPurpose::GetBasicGroupFullInfo;
                    if !self.session.requests.has_purpose_for_chat(purpose, chat_id) {
                        let extra = self.session.request(purpose, Some(chat_id));
                        self.send_json_request(
                            extra,
                            &get_basic_group_full_info(extra, basic_group_id),
                        )?;
                    }
                }
                _ => return Err(ConnectSendError::InvalidRequest),
            }
        }
        if matches!(
            self.session.users_state.chat_photo_history.get(&chat_id.0),
            Some(ChatPhotosFetch::Loading | ChatPhotosFetch::Loaded { .. })
        ) {
            return Ok(());
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatPhotoHistory, Some(chat_id));
        self.session
            .users_state
            .chat_photo_history
            .insert(chat_id.0, ChatPhotosFetch::Loading);
        let json = search_chat_photos(extra, chat_id, MessageId(0), CHAT_PHOTOS_LIMIT);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session
                .users_state
                .chat_photo_history
                .remove(&chat_id.0);
            return Err(err);
        }
        Ok(())
    }

    /// The next page of the photos `owner` has, when the viewer nears the
    /// end of what is loaded. Nothing is sent while a page is in flight or
    /// the list is complete.
    pub fn fetch_more_photos(
        &mut self,
        owner: PhotoOwner,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        match owner {
            PhotoOwner::User(user_id) => {
                let Some(ProfilePhotosFetch::Loaded {
                    total_count,
                    photos,
                    loading_more,
                }) = self
                    .session
                    .users_state
                    .user_profile_photos
                    .get_mut(&user_id)
                else {
                    return Ok(None);
                };
                let offset = i32::try_from(photos.len()).unwrap_or(i32::MAX);
                if *loading_more || offset >= *total_count {
                    return Ok(None);
                }
                *loading_more = true;
                let extra = self
                    .session
                    .request_for_user(RequestPurpose::GetUserProfilePhotos, user_id);
                let json = get_user_profile_photos(
                    extra,
                    user_id,
                    offset,
                    super::profile_panels::PROFILE_PHOTOS_LIMIT,
                );
                let sent = self.send_json_request(extra, &json);
                if sent.is_err()
                    && let Some(ProfilePhotosFetch::Loaded { loading_more, .. }) = self
                        .session
                        .users_state
                        .user_profile_photos
                        .get_mut(&user_id)
                {
                    *loading_more = false;
                }
                sent.map(Some)
            }
            PhotoOwner::Chat(chat_id) => {
                let Some(ChatPhotosFetch::Loaded {
                    next_from_message_id,
                    loading_more,
                    ..
                }) = self
                    .session
                    .users_state
                    .chat_photo_history
                    .get_mut(&chat_id.0)
                else {
                    return Ok(None);
                };
                if *loading_more || next_from_message_id.0 == 0 {
                    return Ok(None);
                }
                let from = *next_from_message_id;
                *loading_more = true;
                let extra = self
                    .session
                    .request(RequestPurpose::GetChatPhotoHistory, Some(chat_id));
                let sent = self.send_json_request(
                    extra,
                    &search_chat_photos(extra, chat_id, from, CHAT_PHOTOS_LIMIT),
                );
                if sent.is_err()
                    && let Some(ChatPhotosFetch::Loaded { loading_more, .. }) = self
                        .session
                        .users_state
                        .chat_photo_history
                        .get_mut(&chat_id.0)
                {
                    *loading_more = false;
                }
                sent.map(Some)
            }
        }
    }

    /// "Set as Main" on an earlier photo of a group or channel, by its
    /// largest size's file. Needs the right to change the chat's info.
    pub fn set_chat_photo_from_earlier(
        &mut self,
        chat_id: ChatId,
        file_id: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || !self.session.chat_can_edit_information(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatPhoto, Some(chat_id));
        self.send_json_request(extra, &set_chat_photo_from_file(extra, chat_id, file_id))
    }

    /// Report a profile, group or channel photo (`reportChatPhoto`, schema
    /// line 16844). A user's photos are reported in the private chat with
    /// them, whose id is the user id. Never your own photos.
    pub fn report_photo(
        &mut self,
        owner: PhotoOwner,
        file_id: i32,
        reason_type: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let chat_id = match owner {
            PhotoOwner::User(user_id) if self.session.my_user_id == Some(user_id) => {
                return Err(ConnectSendError::InvalidRequest);
            }
            PhotoOwner::User(user_id) => user_id,
            PhotoOwner::Chat(chat_id) => chat_id.0,
        };
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::ReportChatPhoto, None);
        self.send_json_request(
            extra,
            &report_chat_photo(extra, chat_id, file_id, reason_type, ""),
        )
    }

    /// Delete a photo from the viewer once its box is confirmed: your own
    /// (`deleteProfilePhoto`), a group or channel's current one
    /// (`setChatPhoto` with no photo) or an earlier one (its
    /// `messageChatChangePhoto`, by `deleteMessages`; `revoke` deletes it
    /// for everyone). The earlier photo's message need not be in the
    /// loaded history, only in the photo list.
    pub fn delete_photo(
        &mut self,
        delete: PhotoDelete,
        revoke: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        match delete {
            PhotoDelete::Own { photo_id } => self.delete_profile_photo(photo_id).map(Some),
            PhotoDelete::ChatPhoto { chat_id } => self.set_group_photo(chat_id, None),
            PhotoDelete::Message {
                chat_id,
                message_id,
                ..
            } => {
                if !self.chats_path_active() || message_id.0 <= 0 {
                    return Err(ConnectSendError::InvalidRequest);
                }
                let listed = matches!(
                    self.session.users_state.chat_photo_history.get(&chat_id.0),
                    Some(ChatPhotosFetch::Loaded { changes, .. })
                        if changes.iter().any(|change| change.message_id == message_id)
                );
                if !listed {
                    return Err(ConnectSendError::InvalidRequest);
                }
                let extra = self
                    .session
                    .request(RequestPurpose::DeleteMessages, Some(chat_id));
                self.send_json_request(
                    extra,
                    &delete_messages(extra, chat_id, &[message_id], revoke),
                )
                .map(Some)
            }
        }
    }
}
