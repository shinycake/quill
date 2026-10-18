//! Connect driver: the Replies chat's "Block" box. Confirming sends
//! `blockMessageSenderFromReplies` with the box's three checkboxes.
use super::*;
use crate::state::RequestPurpose;
use crate::telegram::requests::block_message_sender_from_replies;

impl<S: JsonSender> ConnectDriver<S> {
    /// Close the open box and block its sender. A no-op when no box is open.
    pub fn submit_replies_block(&mut self) -> Result<(), ConnectSendError> {
        let Some(block) = self.session.users_state.replies_block.take() else {
            return Ok(());
        };
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::BlockSenderFromReplies, None);
        let json = block_message_sender_from_replies(
            extra,
            block.message_id.0,
            block.delete_message,
            block.delete_all,
            block.report_spam,
        );
        if let Err(err) = self.sender.send_json(&json) {
            self.session.requests.take(extra);
            return Err(err);
        }
        Ok(())
    }
}
