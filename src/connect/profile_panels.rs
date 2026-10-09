//! Connect driver: profile and contact panels (B10) — groups in common,
//! similar channels, personal channel, birthday, private note, profile
//! photo gallery, "Share contact".
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{ProfileChatsFetch, ProfileChatsKind, ProfilePhotosFetch, RequestPurpose};
use crate::telegram::requests::{
    get_chat_similar_chats, get_groups_in_common, get_suitable_personal_chats,
    get_user_profile_photos, send_contact, set_birthdate, set_personal_chat,
    set_profile_photo_previous, set_user_note,
};

/// Groups in common are listed in one page (tdesktop pages by 50 as you
/// scroll; one page of the maximum covers the practical case).
pub const GROUPS_IN_COMMON_LIMIT: i32 = 100;
/// `getUserProfilePhotos` page size (schema: up to 100).
pub const PROFILE_PHOTOS_LIMIT: i32 = 100;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send a profile-list request unless that list is loading or loaded
    /// (a `Failed` list is retried). The `Loading` marker is dropped again
    /// when the send fails.
    fn fetch_profile_chats(
        &mut self,
        kind: ProfileChatsKind,
        id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if matches!(
            self.session.profile_chat_lists.get(&(kind, id)),
            Some(ProfileChatsFetch::Loading | ProfileChatsFetch::Loaded(_))
        ) {
            return Ok(None);
        }
        let purpose = RequestPurpose::GetProfileChats(kind);
        let extra = match kind {
            ProfileChatsKind::GroupsInCommon => self.session.request_for_user(purpose, id),
            ProfileChatsKind::SimilarChats => self.session.request(purpose, Some(ChatId(id))),
            ProfileChatsKind::SuitablePersonalChats => self.session.request(purpose, None),
        };
        let json = match kind {
            ProfileChatsKind::GroupsInCommon => {
                get_groups_in_common(extra, id, 0, GROUPS_IN_COMMON_LIMIT)
            }
            ProfileChatsKind::SimilarChats => get_chat_similar_chats(extra, ChatId(id)),
            ProfileChatsKind::SuitablePersonalChats => get_suitable_personal_chats(extra),
        };
        self.session
            .profile_chat_lists
            .insert((kind, id), ProfileChatsFetch::Loading);
        match self.send_json_request(extra, &json) {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                self.session.profile_chat_lists.remove(&(kind, id));
                Err(err)
            }
        }
    }

    /// `getGroupsInCommon` for a user's profile (schema 1.8.67, line
    /// 11818).
    pub fn fetch_groups_in_common(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_profile_chats(ProfileChatsKind::GroupsInCommon, user_id)
    }

    /// `getChatSimilarChats` for a channel profile (line 11627). Only
    /// channels have similar chats.
    pub fn fetch_similar_chats(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        self.fetch_profile_chats(ProfileChatsKind::SimilarChats, chat_id.0)
    }

    /// `getSuitablePersonalChats` for the personal channel picker (line
    /// 11693). Always refetched: the candidates change as you create or
    /// leave channels.
    pub fn fetch_suitable_personal_chats(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        self.session
            .profile_chat_lists
            .remove(&(ProfileChatsKind::SuitablePersonalChats, 0));
        self.fetch_profile_chats(ProfileChatsKind::SuitablePersonalChats, 0)
    }

    /// `getUserProfilePhotos` for the gallery (line 14591); deduped like
    /// the chat lists.
    pub fn fetch_user_profile_photos(
        &mut self,
        user_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if matches!(
            self.session.user_profile_photos.get(&user_id),
            Some(ProfilePhotosFetch::Loading | ProfilePhotosFetch::Loaded { .. })
        ) {
            return Ok(None);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::GetUserProfilePhotos, user_id);
        self.session
            .user_profile_photos
            .insert(user_id, ProfilePhotosFetch::Loading);
        let json = get_user_profile_photos(extra, user_id, 0, PROFILE_PHOTOS_LIMIT);
        match self.send_json_request(extra, &json) {
            Ok(extra) => Ok(Some(extra)),
            Err(err) => {
                self.session.user_profile_photos.remove(&user_id);
                Err(err)
            }
        }
    }

    /// `setBirthdate` (line 14841). Day 1..=31 and month 1..=12 are
    /// checked here; `None` removes the birthday.
    pub fn set_birthdate(
        &mut self,
        birthdate: Option<(u8, u8, Option<i32>)>,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if let Some((day, month, _)) = birthdate
            && (!(1..=31).contains(&day) || !(1..=12).contains(&month))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetBirthdate, None);
        self.send_json_request(extra, &set_birthdate(extra, birthdate))
    }

    /// `setPersonalChat` (line 14847); 0 removes the personal channel.
    pub fn set_personal_chat(&mut self, chat_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetPersonalChat, None);
        self.send_json_request(extra, &set_personal_chat(extra, chat_id))
    }

    /// `setUserNote` (line 14553) for a contact; an empty note clears it.
    pub fn set_user_note(
        &mut self,
        user_id: i64,
        note: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::SetUserNote, user_id);
        self.send_json_request(extra, &set_user_note(extra, user_id, note))
    }

    /// "Set as main photo": `setProfilePhoto` with `inputChatPhotoPrevious`
    /// (line 1038). The `ok` answer drops the cached gallery so the
    /// reordered list is fetched again.
    pub fn set_profile_photo_previous(
        &mut self,
        chat_photo_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SetProfilePhoto, None);
        self.send_json_request(extra, &set_profile_photo_previous(extra, chat_photo_id))
    }

    /// "Share contact": send `user_id`'s card into `chat_id` as
    /// `inputMessageContact`. Needs a known phone number and a chat the
    /// current user can post to; rides `SendMessage` so the optimistic
    /// row flows through the normal send path.
    pub fn send_contact_message(
        &mut self,
        chat_id: ChatId,
        user_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let can_post = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| chat.supported() && chat.can_post());
        let Some((phone, first, last)) = self.session.user(user_id).map(|user| {
            (
                user.phone_number.clone(),
                user.first_name.clone(),
                user.last_name.clone(),
            )
        }) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !can_post || phone.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        let topic_id = self.send_topic(chat_id);
        let json = send_contact(extra, chat_id, topic_id, &phone, &first, &last, user_id);
        let json = self.thread_routed(chat_id, json);
        self.send_json_request(extra, &json)
    }
}
