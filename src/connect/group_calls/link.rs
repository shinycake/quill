//! Connect driver: offering the call a voice chat, video chat or live
//! stream link points at (`crate::call_link`).
use super::*;
use crate::call_link::{
    CallLinkConfirm, CallLinkStep, CurrentCall, LeaveKind, PendingCallLink, plan,
};
use crate::ids::ChatId;

impl<S: JsonSender> ConnectDriver<S> {
    /// A call link opened `chat_id`: decide what it does and, when the chat
    /// has a call, fetch it so the join screen shows. The link's hash is kept
    /// for the join. Nothing is joined here: the Join button on the call
    /// screen is tdesktop's `JoinConfirm::Always` box. While another call is
    /// on, the question to leave it is raised instead (`confirmLeaveCurrent`).
    pub fn offer_call_link(
        &mut self,
        chat_id: ChatId,
        invite_hash: &str,
        live_stream: bool,
    ) -> Result<CallLinkStep, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(chat) = self.session.chats.get(&chat_id.0) else {
            return Ok(CallLinkStep::JustOpen);
        };
        let target_is_channel = chat.kind.is_channel();
        let current = self
            .session
            .calls
            .active_group_call
            .as_ref()
            .map(|call| CurrentCall {
                group_call_id: call.id,
                joined: call.is_joined,
            });
        let private_call = self.session.calls.active_call.is_some();
        let step = plan(
            &chat.kind,
            chat.video_chat.as_ref().map(|chat| chat.group_call_id),
            current,
            private_call,
        );
        match step {
            CallLinkStep::Offer { group_call_id } => {
                self.session.calls.call_link = Some(PendingCallLink {
                    group_call_id,
                    invite_hash: invite_hash.to_string(),
                });
                self.fetch_group_call(group_call_id)?;
            }
            CallLinkStep::Busy => {
                let leave = if private_call {
                    LeaveKind::PrivateCall
                } else {
                    let current_is_channel = current
                        .and_then(|call| self.group_call_chat_id(call.group_call_id))
                        .and_then(|id| self.session.chats.get(&id))
                        .is_some_and(|chat| chat.kind.is_channel());
                    LeaveKind::GroupCall { current_is_channel }
                };
                self.session.calls.call_link_confirm = Some(CallLinkConfirm {
                    chat_id: chat_id.0,
                    invite_hash: invite_hash.to_string(),
                    live_stream,
                    target_is_channel,
                    leave,
                });
            }
            _ => {}
        }
        Ok(step)
    }

    /// The viewer confirmed: leave the current call, then offer the link's
    /// call once the old one is gone.
    pub fn confirm_call_link_leave(&mut self) -> Result<(), ConnectSendError> {
        let Some(confirm) = self.session.calls.call_link_confirm.take() else {
            return Ok(());
        };
        match confirm.leave {
            LeaveKind::PrivateCall => {
                let _ = self.discard_call();
            }
            LeaveKind::GroupCall { .. } => {
                let _ = self.leave_group_call();
                self.session.leave_group_call_local();
            }
        }
        self.session.calls.call_link_after_leave = Some((confirm.chat_id, confirm.invite_hash));
        self.maybe_offer_pending_call_link()
    }

    /// The viewer declined: the link does nothing more.
    pub fn cancel_call_link_leave(&mut self) {
        self.session.calls.call_link_confirm = None;
    }

    /// Offer a confirmed link's call once no call is on. Called from
    /// `ingest`, like the swap prompt's queued accept.
    pub(crate) fn maybe_offer_pending_call_link(&mut self) -> Result<(), ConnectSendError> {
        if self.session.calls.call_link_after_leave.is_none()
            || self.session.calls.active_call.is_some()
            || self
                .session
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.is_joined)
        {
            return Ok(());
        }
        let Some((chat_id, hash)) = self.session.calls.call_link_after_leave.take() else {
            return Ok(());
        };
        self.offer_call_link(ChatId(chat_id), &hash, false)
            .map(|_| ())
    }
}
