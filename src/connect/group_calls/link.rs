//! Connect driver: offering the call a voice chat, video chat or live
//! stream link points at (`crate::call_link`).
use super::*;
use crate::call_link::{CallLinkStep, CurrentCall, PendingCallLink, plan};
use crate::ids::ChatId;

impl<S: JsonSender> ConnectDriver<S> {
    /// A call link opened `chat_id`: decide what it does and, when the chat
    /// has a call, fetch it so the join screen shows. The link's hash is kept
    /// for the join. Nothing is joined here: the Join button on the call
    /// screen is tdesktop's `JoinConfirm::Always` box.
    pub fn offer_call_link(
        &mut self,
        chat_id: ChatId,
        invite_hash: &str,
    ) -> Result<CallLinkStep, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Ok(CallLinkStep::JustOpen);
        };
        let current = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| CurrentCall {
                group_call_id: call.id,
                joined: call.is_joined,
            });
        let step = plan(
            &chat.kind,
            chat.video_chat.as_ref().map(|chat| chat.group_call_id),
            current,
            self.session.calls.active_call.is_some(),
        );
        if let CallLinkStep::Offer { group_call_id } = step {
            self.session.calls.call_link = Some(PendingCallLink {
                group_call_id,
                invite_hash: invite_hash.to_string(),
            });
            self.fetch_group_call(group_call_id)?;
        }
        Ok(step)
    }
}
