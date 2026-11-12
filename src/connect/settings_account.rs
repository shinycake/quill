//! Connect driver: Settings > Ask a Question (`getSupportUser`).
use super::*;
use crate::state::RequestPurpose;
use crate::telegram::envelope::{EnvelopePayload, UsersPayload};
use crate::telegram::requests::get_support_user;

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask Telegram for the support account. The `user` answer parks its id
    /// names the chat `ingest` then opens.
    /// `Ok(false)` means a request is already in flight.
    pub fn open_support_chat(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.settings.support_user.is_loading() {
            return Ok(false);
        }
        let extra = self.session.request(RequestPurpose::GetSupportUser, None);
        if let Err(err) = self.sender.send_json(&get_support_user(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        let ask = &mut self.session.settings.support_user;
        ask.begin();
        ask.sent(extra);
        Ok(true)
    }

    /// Settings > Ask a Question: the `getSupportUser` answer names the
    /// user whose chat opens after apply. Read before `apply` takes the
    /// pending request.
    pub(crate) fn support_chat_answer(
        &self,
        envelope: &crate::telegram::envelope::Envelope,
    ) -> Option<i64> {
        let EnvelopePayload::Users(UsersPayload::Me { user_id, .. }) = &envelope.payload else {
            return None;
        };
        // Only the answer of the ask the row holds opens the chat.
        let asked = envelope
            .extra
            .filter(|id| self.session.settings.support_user.waits_for(*id))?;
        (self.session.requests.purpose(asked)? == RequestPurpose::GetSupportUser)
            .then_some(*user_id)
    }

    /// Open the private chats an answer named: the support user's, and a
    /// joined phone's of the New Contact box (which is told once it is
    /// done). `let _` on purpose: a failed open must not fail the ingest.
    pub(crate) fn open_chats_named_by_answers(&mut self, support_chat: Option<i64>) {
        if let Some(user_id) = support_chat {
            let _ = self.create_private_chat_for(user_id);
        }
        if let Some(user_id) = self.session.users_state.new_contact.take_opening() {
            let _ = self.create_private_chat_for(user_id);
        }
    }
}
