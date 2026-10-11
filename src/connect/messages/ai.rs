//! Connect driver: full rich messages and AI text tools.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// M2: `getFullRichMessage` (TDLib 1.8.67, line 11554) for a
    /// partially-received rich message (`is_full == false`). The reducer
    /// replaces the history row's blocks with the full ones on success; a
    /// failed fetch leaves the partial blocks in place (honest).
    pub fn fetch_full_rich_message(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Messages(MessagesPurpose::GetFullRichMessage {
                chat_id,
                message_id,
            }),
            Some(chat_id),
        );
        let json = get_full_rich_message(extra, chat_id, message_id);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: shared guard for the AI composer
    /// methods. The schema forbids `fixTextWithAi` in secret chats; the
    /// other four get the same refusal — sending draft text to a
    /// server-side AI model would break secret-chat privacy.
    fn ai_compose_guard(&self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let is_secret = self
            .session
            .chats
            .get(&chat_id.0)
            .is_some_and(|chat| matches!(chat.kind, ChatKind::Secret { .. }));
        if is_secret {
            return Err(ConnectSendError::InvalidRequest);
        }
        Ok(())
    }

    /// Slice msg-richtext-ai-tools: `fixTextWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12172`) on the composer draft. The `fixedText`
    /// answer replaces the draft.
    pub fn fix_text_with_ai(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::FixTextWithAi, Some(chat_id));
        let json = fix_text_with_ai(extra, text);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `composeTextWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12154`) on the composer draft. Defaults are the
    /// honest no-ops: no translation, keep the current style, no emoji
    /// (the composer has no style/translate picker in this slice).
    pub fn compose_text_with_ai(
        &mut self,
        chat_id: ChatId,
        text: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if text.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::ComposeTextWithAi, Some(chat_id));
        let json = compose_text_with_ai(extra, text, "", "", false);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `composeRichMessageWithAi` (TDLib
    /// 1.8.67, `schema/td_api.tl:12162`) on the composer's parsed blocks.
    /// Defaults: no translation, keep the current style, no custom
    /// prompt, no emoji — same no-picker rationale as
    /// `compose_text_with_ai`.
    pub fn compose_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
    ) -> Result<RequestId, ConnectSendError> {
        let message =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::ComposeRichMessageWithAi, Some(chat_id));
        let json = compose_rich_message_with_ai(extra, &message, "", "", "", false);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `createRichMessageWithAi` (TDLib
    /// 1.8.67, `schema/td_api.tl:12168`). The composer text is the
    /// prompt; `language_code` is the user's app language
    /// (`session.settings.language_prefs.system_language_code`, e.g. "en") —
    /// the schema documents no server-side default for it, so a real
    /// code is always sent. `add_emojis` is false — no pickers in
    /// this slice. The `richMessage` answer replaces the draft (the
    /// prompt was the whole draft).
    pub fn create_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        prompt: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if prompt.trim().is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::CreateRichMessageWithAi, Some(chat_id));
        let json = create_rich_message_with_ai(
            extra,
            prompt,
            &self.session.settings.language_prefs.system_language_code,
            false,
        );
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Slice msg-richtext-ai-tools: `fixRichMessageWithAi` (TDLib 1.8.67,
    /// `schema/td_api.tl:12176`) on the composer's parsed blocks. The
    /// `richMessage` answer replaces the draft as editor markup.
    pub fn fix_rich_message_with_ai(
        &mut self,
        chat_id: ChatId,
        blocks: &[RichBlock],
    ) -> Result<RequestId, ConnectSendError> {
        let message =
            crate::rich::input_rich_message(blocks).ok_or(ConnectSendError::InvalidRequest)?;
        self.ai_compose_guard(chat_id)?;
        let extra = self
            .session
            .request(RequestPurpose::FixRichMessageWithAi, Some(chat_id));
        let json = fix_rich_message_with_ai(extra, &message);
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }
}
