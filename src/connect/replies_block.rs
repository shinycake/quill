//! Connect driver: the Replies chat's "Block" box. Confirming sends
//! `blockMessageSenderFromReplies` with the box's three checkboxes.
use super::*;
use crate::message_menu_origin::RepliesBlockStage;
use crate::state::RequestPurpose;
use crate::telegram::requests::block_message_sender_from_replies;

impl<S: JsonSender> ConnectDriver<S> {
    /// Send the open box's block. The box stays (as `Submitting`) until the
    /// answer: ok closes it, an error gives the choices back. A no-op when
    /// no box is open or its request is already out.
    pub fn submit_replies_block(&mut self) -> Result<(), ConnectSendError> {
        let Some(block) = self.session.users_state.replies_block.clone() else {
            return Ok(());
        };
        if block.stage != RepliesBlockStage::Editing {
            return Ok(());
        }
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
        if let Some(open) = self.session.users_state.replies_block.as_mut() {
            open.stage = RepliesBlockStage::Submitting(extra);
        }
        Ok(())
    }
}
