//! Connect driver: verifying and unverifying accounts with an owned bot
//! (`setMessageSenderBotVerification` / `removeMessageSenderBotVerification`).
use super::*;
use crate::ids::RequestId;
use crate::state::RequestPurpose;
use crate::telegram::requests::MessageSenderRef;

impl<S: JsonSender> ConnectDriver<S> {
    /// Verify `target` with the owned bot. `description` is empty unless
    /// the bot may customise it (see `bot_verification::description_to_send`).
    pub fn set_bot_verification(
        &mut self,
        bot_user_id: i64,
        target: MessageSenderRef,
        description: &str,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_bot_verification(
            RequestPurpose::SetMessageSenderBotVerification,
            bot_user_id,
            |extra| {
                crate::bot_verification::set_request(
                    &extra.as_extra(),
                    bot_user_id,
                    target.to_value(),
                    description,
                )
            },
        )
    }

    /// Take the bot's verification off `target`.
    pub fn remove_bot_verification(
        &mut self,
        bot_user_id: i64,
        target: MessageSenderRef,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_bot_verification(
            RequestPurpose::RemoveMessageSenderBotVerification,
            bot_user_id,
            |extra| {
                crate::bot_verification::remove_request(
                    &extra.as_extra(),
                    bot_user_id,
                    target.to_value(),
                )
            },
        )
    }

    fn send_bot_verification(
        &mut self,
        purpose: RequestPurpose,
        bot_user_id: i64,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || bot_user_id <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(purpose, None);
        match self.sender.send_json(&build(extra)) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
