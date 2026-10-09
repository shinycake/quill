//! Connect driver: the attach menu's Contact and Location sends.
use super::*;
use crate::composer::SendOptions;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::{ContactShare, SendReply, send_contact_card, send_location};

impl<S: JsonSender> ConnectDriver<S> {
    /// The guards every content send shares: chats path active, a chat the
    /// user may post to, not a closed forum topic.
    fn share_target_ok(&self, chat_id: ChatId) -> bool {
        self.chats_path_active()
            && self
                .session
                .chats
                .get(&chat_id.0)
                .is_some_and(|chat| chat.can_post())
            && !self.topic_send_is_closed(chat_id)
    }

    fn send_built(
        &mut self,
        chat_id: ChatId,
        build: impl FnOnce(RequestId, Option<i32>) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self
            .session
            .request(RequestPurpose::SendMessage, Some(chat_id));
        // Sends from a topic view address the open topic.
        let topic_id = self.send_topic(chat_id);
        let json = build(extra, topic_id);
        let json = self.thread_routed(chat_id, json);
        match self.sender.send_json(&json) {
            Ok(()) => {
                let _ = self.cancel_outgoing_typing();
                Ok(extra)
            }
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }

    /// `sendMessage` + `inputMessageContact` (the attach menu's Contact).
    /// A contact needs a phone number or a Telegram user id.
    pub fn share_contact_to_chat(
        &mut self,
        chat_id: ChatId,
        contact: &ContactShare,
        reply_to: Option<SendReply>,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.share_target_ok(chat_id)
            || (contact.phone_number.trim().is_empty() && contact.user_id == 0)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_built(chat_id, |extra, topic_id| {
            send_contact_card(
                extra,
                chat_id,
                topic_id,
                contact,
                reply_to.as_ref(),
                options,
            )
        })
    }

    /// `sendMessage` + `inputMessageLocation` (the attach menu's Location).
    pub fn share_location_to_chat(
        &mut self,
        chat_id: ChatId,
        latitude: f64,
        longitude: f64,
        reply_to: Option<SendReply>,
        options: &SendOptions,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.share_target_ok(chat_id) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_built(chat_id, |extra, topic_id| {
            send_location(
                extra,
                chat_id,
                topic_id,
                latitude,
                longitude,
                reply_to.as_ref(),
                options,
            )
        })
    }
}
