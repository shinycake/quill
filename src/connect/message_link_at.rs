//! Copy a video post's link at the playback position (tdesktop's
//! "Share at 0:12": `CopyPostLink` with a video timestamp).

use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::state::RequestPurpose;
use crate::telegram::requests::get_message_link_at;

impl<S: JsonSender> ConnectDriver<S> {
    /// `getMessageLink` with `media_timestamp`. The answer takes the same
    /// road as a plain message link (`session.messages.message_link_result`,
    /// copied to the clipboard by the poll loop; a refusal lands in
    /// `message_link_error`).
    pub fn get_message_link_at(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        media_timestamp: i32,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let sent = self
            .session
            .histories
            .get(&chat_id.0)
            .and_then(|history| history.messages.get(&message_id.0))
            .is_some_and(|message| !message.pending && message.id.0 > 0);
        if !sent {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetMessageLink, Some(chat_id));
        let json = get_message_link_at(extra, chat_id, message_id, media_timestamp);
        match self.sender.send_json(&json) {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
