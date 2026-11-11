//! Connect driver: a channel's or supergroup's appearance (tdesktop
//! `EditPeerColorBox` for channels): name color, profile color, emoji status.
use super::*;
use crate::chat_appearance::ApplyPlan;
use crate::ids::{ChatId, FileId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests_chat_appearance::{
    get_default_chat_emoji_statuses, set_chat_accent_color, set_chat_emoji_status,
    set_chat_profile_accent_color, set_chat_theme_background,
};

/// What Apply put on the wire: the requests, and the error of the part that
/// stopped the rest, if one did.
#[derive(Debug, Default)]
pub struct AppearanceSent {
    pub requests: Vec<RequestId>,
    pub error: Option<ConnectSendError>,
}

impl AppearanceSent {
    fn record(&mut self, request: RequestId, sent: Result<RequestId, ConnectSendError>) {
        match sent {
            Ok(_) => self.requests.push(request),
            Err(err) => self.error = Some(err),
        }
    }
}

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
        self.session.chats_state.appearance_answers.clear();
        self.refresh_chat_boost_level(chat_id)?;
        // Supergroups choose a custom emoji pack from their installed ones.
        let _ = self.load_group_sticker_choices(chat_id);
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

    /// Drop the cached boost level and fetch it again, with the level
    /// table. tdesktop asks for it at Apply time (`CheckBoostLevel`).
    pub fn refresh_chat_boost_level(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.groups.chat_boost_status.remove(&chat_id.0);
        self.fetch_chat_boost_status(chat_id)?;
        self.fetch_chat_boost_features(chat_id)?;
        Ok(())
    }

    /// Set a chat theme: a private chat gets `setChatTheme`; a boosted
    /// channel or supergroup gets a chat theme background instead.
    pub fn set_chat_look_theme(
        &mut self,
        chat_id: ChatId,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if self.session.chat_supergroup(chat_id).is_none() {
            return self.set_chat_theme(chat_id, name);
        }
        if !self.can_edit_chat_appearance(chat_id) || name.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetChatTheme, Some(chat_id));
        self.send_json_request(extra, &set_chat_theme_background(extra, chat_id, name))
    }

    /// Closing the box stops resolving its status choices.
    pub fn close_chat_appearance(&mut self) {
        self.session.stickers.emoji.chat_status_open = false;
    }

    /// Whether every part of `plan` can be sent: valid values, and the
    /// emoji pack allowed. Checked before any part goes out, so a refusal
    /// never leaves half a plan applied.
    fn appearance_plan_is_sendable(&self, chat_id: ChatId, plan: &ApplyPlan) -> bool {
        let name_ok = plan
            .name
            .is_none_or(|(color, emoji)| color >= 0 && emoji >= 0);
        let profile_ok = plan
            .profile
            .is_none_or(|(color, emoji)| color >= -1 && emoji >= 0);
        let status_ok = plan
            .status
            .is_none_or(|status| status.is_none_or(|id| id > 0));
        let pack_ok = plan
            .emoji_pack
            .is_none_or(|pack| pack >= 0 && self.emoji_pack_target(chat_id).is_some());
        name_ok && profile_ok && status_ok && pack_ok
    }

    /// Send the changed parts of `plan`, one request each, after the whole
    /// plan was checked. Returns the requests sent; each answers under its
    /// own id (`ChatsState::appearance_answers`). `Err` means nothing went
    /// out; a part that fails to send after earlier ones did is the last
    /// element's error, returned with what was sent.
    pub fn apply_chat_appearance(
        &mut self,
        chat_id: ChatId,
        plan: ApplyPlan,
    ) -> Result<AppearanceSent, ConnectSendError> {
        if !self.can_edit_chat_appearance(chat_id)
            || !self.appearance_plan_is_sendable(chat_id, &plan)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.session.chats_state.appearance_answers.clear();
        let mut sent = AppearanceSent::default();
        if let Some((color, emoji)) = plan.name {
            let extra = self
                .session
                .request(RequestPurpose::SetChatAccentColor, Some(chat_id));
            sent.record(
                extra,
                self.send_json_request(extra, &set_chat_accent_color(extra, chat_id, color, emoji)),
            );
        }
        if let Some((color, emoji)) = plan.profile
            && sent.error.is_none()
        {
            let extra = self
                .session
                .request(RequestPurpose::SetChatProfileAccentColor, Some(chat_id));
            sent.record(
                extra,
                self.send_json_request(
                    extra,
                    &set_chat_profile_accent_color(extra, chat_id, color, emoji),
                ),
            );
        }
        if let Some(status) = plan.status
            && sent.error.is_none()
        {
            let extra = self
                .session
                .request(RequestPurpose::SetChatEmojiStatus, Some(chat_id));
            sent.record(
                extra,
                self.send_json_request(extra, &set_chat_emoji_status(extra, chat_id, status)),
            );
        }
        if let Some(pack) = plan.emoji_pack
            && sent.error.is_none()
        {
            match self.set_supergroup_custom_emoji_sticker_set(chat_id, pack) {
                Ok(Some(extra)) => sent.requests.push(extra),
                Ok(None) => sent.error = Some(ConnectSendError::InvalidRequest),
                Err(err) => sent.error = Some(err),
            }
        }
        match (sent.requests.is_empty(), sent.error) {
            (true, Some(err)) => Err(err),
            _ => Ok(sent),
        }
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
