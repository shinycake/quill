//! Connect driver: secret-chat lifecycle.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{
    close_secret_chat as close_secret_chat_request, create_new_secret_chat, get_secret_chat,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Phase B1: `createNewSecretChat` for a user. Gated on a known
    /// non-bot user — the affordance lives on the user profile panel, and
    /// secret chats are 1:1 E2E sessions (bots are cloud-side actors).
    /// The new chat arrives as `updateNewChat` (chatTypeSecret); its state
    /// arrives as `updateSecretChat`.
    pub fn start_secret_chat(&mut self, user_id: i64) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let user = self.session.user(user_id);
        let is_bot = user.is_some_and(|u| u.is_bot);
        let is_self = self.session.my_user_id.is_some_and(|me| me == user_id);
        if user.is_none() || is_bot || is_self {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::CreateNewSecretChat, None);
        if let Err(err) = self
            .sender
            .send_json(&create_new_secret_chat(extra, user_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase B1: `closeSecretChat` for a secret chat. The state change to
    /// `secretChatStateClosed` arrives as `updateSecretChat`; the composer
    /// hides then (the chat can never send again).
    pub fn close_secret_chat(&mut self, chat_id: ChatId) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let secret_chat_id = self
            .session
            .chats
            .get(&chat_id.0)
            .and_then(|chat| chat.secret_chat_id());
        let Some(secret_chat_id) = secret_chat_id else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let extra = self
            .session
            .request_for_secret_chat(RequestPurpose::CloseSecretChat, secret_chat_id);
        if let Err(err) = self
            .sender
            .send_json(&close_secret_chat_request(extra, secret_chat_id))
        {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(extra)
    }

    /// Phase B1: drain `Session::secret_chat_fetch_queue` — one
    /// `getSecretChat` (an offline method) per unknown secret-chat state,
    /// deduped against in-flight fetches. Called from `ingest`.
    pub(crate) fn maybe_fetch_secret_chat_states(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let queued: Vec<i32> =
            std::mem::take(&mut self.session.users_state.secret_chat_fetch_queue);
        for secret_chat_id in queued {
            if self
                .session
                .users_state
                .secret_chat_states
                .contains_key(&secret_chat_id)
            {
                continue;
            }
            let in_flight = self
                .session
                .requests
                .has_pending_for_secret_chat(RequestPurpose::GetSecretChat, secret_chat_id);
            if in_flight {
                self.session
                    .users_state
                    .secret_chat_fetch_queue
                    .push(secret_chat_id);
                continue;
            }
            let extra = self
                .session
                .request_for_secret_chat(RequestPurpose::GetSecretChat, secret_chat_id);
            if let Err(err) = self
                .sender
                .send_json(&get_secret_chat(extra, secret_chat_id))
            {
                self.session.requests.take(extra);
                self.session
                    .users_state
                    .secret_chat_fetch_queue
                    .push(secret_chat_id);
                return Err(err);
            }
        }
        Ok(())
    }
}
