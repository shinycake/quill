//! Connect driver: the group and channel Edit box (`setChatTitle`,
//! `setChatDescription`, `setChatPhoto`) and the emoji its photo builder
//! offers.
use super::*;
use crate::group_info_edit::{DESCRIPTION_MAX, GroupInfoEdit, TITLE_MAX};
use crate::profile_photo_sources::{ChatPhotoInput, get_default_chat_photo_custom_emoji_stickers};
use crate::telegram::requests::{set_chat_description, set_chat_photo, set_chat_title};

/// What one Edit box save sent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupInfoSent {
    /// `setChatTitle` and `setChatDescription`: the box stays open, saving,
    /// until TDLib answers these.
    pub text: Vec<RequestId>,
    /// `setChatPhoto`: uploads after the box closes, as in tdesktop.
    pub photo: Option<RequestId>,
}

impl<S: JsonSender> ConnectDriver<S> {
    /// `setChatTitle` (schema line 14424): 1-128 characters, refused
    /// client-side otherwise. Not optimistic: `updateChatTitle` carries the
    /// new title. `None` when the viewer may not change it or one is in
    /// flight.
    pub fn set_group_title(
        &mut self,
        chat_id: ChatId,
        title: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let len = title.chars().count();
        if !self.chats_path_active() || len == 0 || len > TITLE_MAX {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_info_edit(chat_id, RequestPurpose::SetChatTitle, |extra| {
            set_chat_title(extra, chat_id, title)
        })
    }

    /// `setChatDescription` (schema line 14532): 0-255 characters, empty
    /// clears it. TDLib sends the new text in `updateSupergroupFullInfo` /
    /// `updateBasicGroupFullInfo`.
    pub fn set_group_description(
        &mut self,
        chat_id: ChatId,
        description: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || description.chars().count() > DESCRIPTION_MAX {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_info_edit(chat_id, RequestPurpose::SetChatDescription, |extra| {
            set_chat_description(extra, chat_id, description)
        })
    }

    /// `setChatPhoto` (schema line 14429) with a still, a clip or an emoji
    /// sticker; `None` deletes the photo ("pass null to delete").
    pub fn set_chat_photo(
        &mut self,
        chat_id: ChatId,
        photo: Option<&ChatPhotoInput>,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() || photo.is_some_and(|photo| !photo.is_valid()) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let json = photo.map_or(serde_json::Value::Null, ChatPhotoInput::to_json);
        self.send_info_edit(chat_id, RequestPurpose::SetChatPhoto, |extra| {
            set_chat_photo(extra, chat_id, json)
        })
    }

    /// The Edit box's Save: every edit in order. A refused edit (no right,
    /// or the same edit still in flight) stops the save there, like a
    /// failed tdesktop stage; what was already sent stays sent.
    pub fn save_group_info(
        &mut self,
        chat_id: ChatId,
        edits: &[GroupInfoEdit],
    ) -> Result<GroupInfoSent, ConnectSendError> {
        self.session.groups.info_edit_errors.remove(&chat_id.0);
        let mut sent = GroupInfoSent::default();
        for edit in edits {
            let request = match edit {
                GroupInfoEdit::Title(title) => self.set_group_title(chat_id, title)?,
                GroupInfoEdit::Description(text) => self.set_group_description(chat_id, text)?,
                GroupInfoEdit::Photo(photo) => self.set_chat_photo(chat_id, Some(photo))?,
            };
            let Some(request) = request else {
                return Err(ConnectSendError::InvalidRequest);
            };
            match edit {
                GroupInfoEdit::Photo(_) => sent.photo = Some(request),
                _ => sent.text.push(request),
            }
        }
        Ok(sent)
    }

    /// `getDefaultChatPhotoCustomEmojiStickers` once (again after a
    /// failure); the answer fills `UsersState::chat_photo_emoji_icons`.
    pub fn load_chat_photo_emoji_icons(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let users = &self.session.users_state;
        if (!users.chat_photo_emoji_icons.is_empty() && !users.chat_photo_emoji_icons_failed)
            || self
                .session
                .requests
                .has_purpose(RequestPurpose::GetChatPhotoEmojiIcons)
        {
            return Ok(None);
        }
        self.session.users_state.chat_photo_emoji_icons_failed = false;
        let extra = self
            .session
            .request(RequestPurpose::GetChatPhotoEmojiIcons, None);
        let json = get_default_chat_photo_custom_emoji_stickers(extra);
        self.send_json_request(extra, &json).map(Some)
    }

    /// One info edit: gated on `Session::chat_can_edit_information`
    /// (tdesktop `canEditInformation`) and deduped per chat.
    fn send_info_edit(
        &mut self,
        chat_id: ChatId,
        purpose: RequestPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.chat_can_edit_information(chat_id)
            || self.session.requests.has_purpose_for_chat(purpose, chat_id)
        {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(extra, &build(extra)).map(Some)
    }
}
