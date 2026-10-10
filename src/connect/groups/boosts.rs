//! Connect driver: chat boosts.
use super::*;

impl<S: JsonSender> ConnectDriver<S> {
    /// Slice G2: `getChatBoostStatus` (schema 1.8.67, line 13917) —
    /// cached per chat (level + boost count) for the boost dialog.
    pub fn fetch_chat_boost_status(
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
        if self.session.chat_boost_status.contains_key(&chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetChatBoostStatus, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetChatBoostStatus, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_chat_boost_status(extra, chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Slice G2: boost the chat. Sends `getAvailableChatBoostSlots`
    /// (schema 1.8.67, line 13914); the driver chains `boostChat` with
    /// the first slot once the answer arrives (`maybe_continue_boost`).
    /// Deduped while an intent or either request is in flight.
    pub fn request_chat_boost(
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
        if self.session.boost_intent == Some(chat_id.0)
            || self
                .session
                .requests
                .has_purpose_for_chat(RequestPurpose::GetBoostSlotsForBoost, chat_id)
        {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetBoostSlotsForBoost, Some(chat_id));
        if let Err(err) = self
            .sender
            .send_json(&get_available_chat_boost_slots(extra))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        self.session.boost_intent = Some(chat_id.0);
        Ok(Some(extra))
    }

    /// Slice G2: `boostChat` chain — once the slots answer for a pending
    /// boost intent arrives, send `boostChat` with the first slot id.
    /// An empty slot list (or a failed slots request) just drops the
    /// intent; `boostChat` errors are reported by the reducer.
    pub(crate) fn maybe_continue_boost(&mut self) -> Result<(), ConnectSendError> {
        let Some(chat_id) = self.session.boost_intent else {
            return Ok(());
        };
        let Some(slots) = self.session.boost_slots_by_chat.remove(&chat_id) else {
            return Ok(());
        };
        self.session.boost_intent = None;
        let Some(slot_id) = slots.into_iter().next() else {
            return Ok(());
        };
        let chat = ChatId(chat_id);
        let extra = self.session.request(RequestPurpose::BoostChat, Some(chat));
        if let Err(err) = self.sender.send_json(&boost_chat(extra, chat, &[slot_id])) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }
}
