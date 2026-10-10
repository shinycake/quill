//! Connect driver: Settings > Ask a Question (`getSupportUser`).
use super::*;
use crate::state::RequestPurpose;
use crate::telegram::requests::get_support_user;

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask Telegram for the support account. The `user` answer parks its id
    /// in `Session::support_user_ready` and `ingest` then opens the chat.
    /// `Ok(false)` means a request is already in flight.
    pub fn open_support_chat(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose(RequestPurpose::GetSupportUser)
        {
            return Ok(false);
        }
        let extra = self.session.request(RequestPurpose::GetSupportUser, None);
        if let Err(err) = self.sender.send_json(&get_support_user(extra)) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(true)
    }
}
