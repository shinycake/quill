//! Connect driver: supergroup toggles (signatures, anti-spam) and group sticker sets.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice G2: `toggleSupergroupSignMessages` (schema 1.8.67, line
    /// 15175). Channels only; gated on `can_change_info` (creator or an
    /// admin with the right, like Telegram X's `ProfileController`).
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_sign_messages(
        &mut self,
        chat_id: ChatId,
        sign_messages: bool,
        show_message_sender: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: true,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_change_info(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupSignMessages;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&toggle_supergroup_sign_messages(
            extra,
            supergroup_id,
            sign_messages,
            show_message_sender,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flags ride on the pending entry so
        // the error arm can roll back.
        let previous_sign = self
            .session
            .supergroup_sign_messages
            .get(&supergroup_id)
            .copied();
        let previous_show = self
            .session
            .supergroup_show_message_sender
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_sign_messages
            .insert(supergroup_id, sign_messages);
        self.session
            .supergroup_show_message_sender
            .insert(supergroup_id, sign_messages && show_message_sender);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::SignMessages {
                supergroup_id,
                previous_sign,
                previous_show,
            });
        }
        Ok(Some(extra))
    }

    /// Slice G2: `toggleSupergroupHasAggressiveAntiSpamEnabled` (schema
    /// 1.8.67, line 15212). Non-channel supergroups only; the schema
    /// requires `supergroupFullInfo.can_toggle_aggressive_anti_spam`.
    /// Optimistic — the error arm rolls back via `RequestRollback`.
    pub fn toggle_aggressive_anti_spam(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let supergroup_id = match self.session.chats.get(&chat_id.0) {
            Some(chat) => match chat.kind {
                ChatKind::Supergroup {
                    supergroup_id,
                    is_channel: false,
                } => supergroup_id,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };
        if !self.session.chat_can_toggle_anti_spam(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::ToggleSupergroupAggressiveAntiSpam;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&toggle_supergroup_aggressive_anti_spam(
                extra,
                supergroup_id,
                enabled,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        // Optimistic: the previous flag rides on the pending entry so
        // the error arm can roll back.
        let previous = self
            .session
            .supergroup_anti_spam_enabled
            .get(&supergroup_id)
            .copied();
        self.session
            .supergroup_anti_spam_enabled
            .insert(supergroup_id, enabled);
        if let Some(pending) = self.session.requests.pending_mut(extra) {
            pending.rollback = Some(RequestRollback::AntiSpam {
                supergroup_id,
                previous,
            });
        }
        Ok(Some(extra))
    }

    /// Slice S11: `setSupergroupStickerSet` (schema 1.8.67, line 15154).
    /// Gated on `supergroupFullInfo.can_set_sticker_set` (fail closed while
    /// the full info is unfetched). `sticker_set_id` 0 removes the group
    /// sticker set per the schema; negative ids are refused client-side.
    /// Not optimistic — `updateSupergroupFullInfo` carries the confirmed
    /// `sticker_set_id` back. In-flight dedup per chat.
    pub fn set_supergroup_sticker_set(
        &mut self,
        chat_id: ChatId,
        sticker_set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if sticker_set_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(supergroup_id) =
            self.session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                    _ => None,
                })
        else {
            return Ok(None);
        };
        if !self.session.chat_can_set_sticker_set(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupStickerSet;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&set_supergroup_sticker_set(
            extra,
            supergroup_id,
            sticker_set_id,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice S11: `setSupergroupCustomEmojiStickerSet` (schema 1.8.67,
    /// line 15159). Same gating as the regular group sticker set;
    /// `custom_emoji_sticker_set_id` 0 removes it per the schema.
    pub fn set_supergroup_custom_emoji_sticker_set(
        &mut self,
        chat_id: ChatId,
        custom_emoji_sticker_set_id: i64,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if custom_emoji_sticker_set_id < 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(supergroup_id) =
            self.session
                .chats
                .get(&chat_id.0)
                .and_then(|chat| match chat.kind {
                    ChatKind::Supergroup { supergroup_id, .. } => Some(supergroup_id),
                    _ => None,
                })
        else {
            return Ok(None);
        };
        if !self.session.chat_can_set_sticker_set(chat_id) {
            return Ok(None);
        }
        let purpose = RequestPurpose::SetSupergroupCustomEmojiStickerSet;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&set_supergroup_custom_emoji_sticker_set(
                extra,
                supergroup_id,
                custom_emoji_sticker_set_id,
            ))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
