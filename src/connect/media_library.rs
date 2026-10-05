//! Connect driver: the composer's emoji/sticker panel library and the
//! message reaction picker.
use super::*;
use crate::ids::{ChatId, FileId, MessageId, RequestId};
use crate::state::{ReactionChoice, RequestPurpose};
use crate::telegram::requests::{
    get_installed_sticker_sets, get_message_available_reactions, get_recent_stickers,
    get_sticker_set, reaction_type_custom_emoji, reaction_type_emoji, set_message_reaction,
};

/// Download priority for panel cells: below chat thumbnails, above
/// background media.
const PANEL_DOWNLOAD_PRIORITY: i32 = 8;
/// Reactions in the picker's quick strip (`row_size`).
pub const REACTION_STRIP_SIZE: i32 = 8;

impl<S: JsonSender> ConnectDriver<S> {
    /// The panel opened: make sure the installed sticker and custom-emoji
    /// set lists, recent stickers and favorites are (being) loaded. Set
    /// contents load per section via [`Self::ensure_library_sets`].
    pub fn open_media_panel(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.stickers.installed_loaded
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetInstalledStickerSets)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetInstalledStickerSets, None);
            if let Err(err) = self.sender.send_json(&get_installed_sticker_sets(extra)) {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        if self.session.emoji.installed_sets.is_empty()
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetInstalledEmojiSets)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetInstalledEmojiSets, None);
            if let Err(err) =
                self.sender
                    .send_json(&crate::telegram::requests_emoji::get_installed_emoji_sets(
                        extra,
                    ))
            {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        if self.session.stickers.recent.is_empty()
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetRecentStickers)
        {
            let extra = self
                .session
                .request(RequestPurpose::GetRecentStickers, None);
            if let Err(err) = self.sender.send_json(&get_recent_stickers(extra, false)) {
                self.session.requests.take(extra);
                return Err(err);
            }
        }
        if self.session.stickers.favorites.is_empty() {
            self.sticker_request_favorites()?;
        }
        Ok(())
    }

    /// Load the contents of the sets whose sections are on screen (capped
    /// in flight; loaded, loading and failed sets are skipped).
    pub fn ensure_library_sets(&mut self, wanted: &[i64]) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        for set_id in self.session.library_sets_to_load(wanted) {
            let extra = self
                .session
                .request(RequestPurpose::LoadLibrarySet { set_id }, None);
            if let Err(err) = self.sender.send_json(&get_sticker_set(extra, set_id)) {
                self.session.requests.take(extra);
                return Err(err);
            }
            self.session.media_library.loading.insert(set_id);
        }
        Ok(())
    }

    /// Download the files of the panel cells on screen (static display
    /// files: WEBP stickers, thumbnails of animated ones).
    pub fn ensure_media_files(&mut self, files: &[FileId]) -> Result<(), ConnectSendError> {
        for file_id in files {
            if file_id.0 != 0 {
                self.download_file(*file_id, PANEL_DOWNLOAD_PRIORITY)?;
            }
        }
        Ok(())
    }

    /// Ask which reactions `message_id` may get (the picker's strip and
    /// full list). Drops the options of any other message.
    pub fn fetch_message_reactions(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .message_reaction_options
            .as_ref()
            .is_some_and(|o| o.chat_id == chat_id && o.message_id == message_id)
        {
            return Ok(None);
        }
        self.session.message_reaction_options = None;
        let purpose = RequestPurpose::GetMessageAvailableReactions {
            message_id: message_id.0,
        };
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        match self.sender.send_json(&get_message_available_reactions(
            extra,
            chat_id,
            message_id,
            REACTION_STRIP_SIZE,
        )) {
            Ok(()) => Ok(Some(extra)),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// Add `choice` to the message, or remove it when it's already ours.
    pub fn toggle_reaction_choice(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        choice: &ReactionChoice,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(message) = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if !message.can_react() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let reactions = message
            .interaction_info
            .as_ref()
            .and_then(|info| info.reactions.as_ref());
        let (chosen, value) = match choice {
            ReactionChoice::Emoji(emoji) => (
                reactions.is_some_and(|r| r.chosen_emoji(emoji)),
                reaction_type_emoji(emoji),
            ),
            ReactionChoice::CustomEmoji(id) => (
                reactions.is_some_and(|r| r.chosen_custom_emoji(*id)),
                reaction_type_custom_emoji(*id),
            ),
        };
        let purpose = if chosen {
            RequestPurpose::RemoveMessageReaction
        } else {
            RequestPurpose::AddMessageReaction
        };
        let extra = self.session.request(purpose, Some(chat_id));
        match self.sender.send_json(&set_message_reaction(
            extra, chat_id, message_id, value, !chosen,
        )) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
