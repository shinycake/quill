//! Connect driver: chat welcome messages.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice G2: `loadChatWelcomeMessages` (schema 1.8.67, line 12630).
    /// The pack also arrives spontaneously as `updateChatWelcomeMessages`;
    /// deduped on a cached pack or an in-flight fetch.
    pub fn load_chat_welcome_messages(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) {
            return Ok(None);
        }
        if self.session.welcome_messages.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::LoadChatWelcomeMessages, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::LoadChatWelcomeMessages, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&load_chat_welcome_messages(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session
            .welcome_message_fetches
            .insert(chat_id.0, WelcomeMessagesFetch::Loading);
        Ok(Some(extra))
    }

    /// Slice G2: shared gate for welcome-message mutations — requires
    /// `can_send_welcome_messages` (creator or an admin with the right)
    /// in a supergroup or channel.
    fn welcome_mutation_gate(&self, chat_id: ChatId) -> bool {
        matches!(
            self.session.chats.get(&chat_id.0).map(|chat| &chat.kind),
            Some(ChatKind::Supergroup { .. })
        ) && self.session.chat_can_send_welcome_messages(chat_id)
    }

    /// Slice G2: `addChatWelcomeMessage` (schema 1.8.67, line 12639).
    /// Answers `ok`; the pack is refetched on success.
    pub fn add_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::AddChatWelcomeMessage)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::AddChatWelcomeMessage, Some(chat_id));
        if let Err(err) =
            self.sender
                .send_json(&add_chat_welcome_message(extra, chat_id, text.trim()))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `editChatWelcomeMessage` (schema 1.8.67, line 12646).
    /// Answers `ok`; the pack is refetched on success.
    pub fn edit_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
        text: &str,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose =
            RequestPurpose::Groups(GroupsPurpose::EditChatWelcomeMessage { welcome_message_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&edit_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
            text.trim(),
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: `deleteChatWelcomeMessage` (schema 1.8.67, line 12651).
    /// Answers `ok`; the pack is refetched on success.
    pub fn delete_chat_welcome_message(
        &mut self,
        chat_id: ChatId,
        welcome_message_id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.welcome_mutation_gate(chat_id) {
            return Ok(None);
        }
        let purpose =
            RequestPurpose::Groups(GroupsPurpose::DeleteChatWelcomeMessage { welcome_message_id });
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        if let Err(err) = self.sender.send_json(&delete_chat_welcome_message(
            extra,
            chat_id,
            welcome_message_id,
        )) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }
}
