//! Connect driver: pause, resume and remove the business bot that manages a
//! chat (tdesktop `Data::Chatbots::togglePaused` / `removeFrom`).
use super::*;
use crate::business_bot_bar as bar;
use crate::ids::{ChatId, RequestId};
use crate::state::RequestPurpose;

impl<S: JsonSender> ConnectDriver<S> {
    /// `toggleBusinessConnectedBotChatIsPaused`. The bar flips locally at
    /// once; `updateChatBusinessBotManageBar` confirms it.
    pub fn toggle_business_bot_paused(
        &mut self,
        chat_id: ChatId,
        paused: bool,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(current) = self.session.business_bot_bar(chat_id).cloned() else {
            return Ok(None);
        };
        let purpose = RequestPurpose::ToggleBusinessBotPaused;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &bar::toggle_paused_request(&extra.as_extra(), chat_id.0, paused),
        )?;
        self.session
            .set_business_bot_bar(chat_id.0, Some(current.with_paused(paused)));
        Ok(Some(extra))
    }

    /// `removeBusinessConnectedBotFromChat`. The bar goes away locally at
    /// once.
    pub fn remove_business_bot_from_chat(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.business_bot_bar(chat_id).is_none() {
            return Ok(None);
        }
        let purpose = RequestPurpose::RemoveBusinessBotFromChat;
        if self.session.requests.has_purpose_for_chat(purpose, chat_id) {
            return Ok(None);
        }
        let extra = self.session.request(purpose, Some(chat_id));
        self.send_json_request(
            extra,
            &bar::remove_from_chat_request(&extra.as_extra(), chat_id.0),
        )?;
        self.session.set_business_bot_bar(chat_id.0, None);
        Ok(Some(extra))
    }
}
