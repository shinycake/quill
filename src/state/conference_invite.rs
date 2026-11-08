//! An incoming conference call invitation (tdesktop
//! `Instance::showConferenceInvite`): a fresh `messageGroupCall` from
//! someone else that you haven't joined or missed rings like a call, with
//! Accept and Decline, until it is answered, joined elsewhere, or missed.
use super::*;
use crate::telegram::envelope::ParsedMessage;

/// tdesktop declines an invitation older than the server's call ring
/// timeout (`callRingTimeoutMs`, 90 s by default) instead of ringing.
pub const CONFERENCE_INVITE_RING_SECS: i64 = 90;

/// The invitation ringing now. While it rings it is the current call, as
/// tdesktop's invite `Call` is `_currentCall` (`Instance::inCall`): an
/// incoming 1:1 call and a second invitation are declined as busy, and
/// placing a call or joining a voice chat declines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConferenceInvite {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    /// The conference call (`messageGroupCall.unique_id`).
    pub unique_id: i64,
    /// Who invites you.
    pub user_id: i64,
    pub is_video: bool,
    /// Some of the people already in the call.
    pub others: Vec<i64>,
}

impl Session {
    /// `updateNewMessage`: an incoming invitation starts ringing, unless
    /// you are in a call (a ringing invitation counts) or it is too old,
    /// when it is declined (tdesktop `showConferenceInvite`). An
    /// invitation to the conference that rings or that you are in is
    /// left alone.
    pub(crate) fn note_conference_invite(&mut self, message: &ParsedMessage, now_unix: i64) {
        let MessageContent::GroupCallInvitation {
            unique_id,
            is_active: false,
            was_missed: false,
            is_video,
            other_participants,
            ..
        } = &message.content
        else {
            return;
        };
        let Some(MessageSender::User { user_id }) = message.sender else {
            return;
        };
        if message.is_outgoing || Some(user_id) == self.my_user_id {
            return;
        }
        if self.is_current_conference(message.chat_id, message.id, *unique_id) {
            return;
        }
        let busy = self.calls.active_call.is_some()
            || self.calls.conference_invite.is_some()
            || self.calls.pending_conference.is_some()
            || self
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.is_joined);
        let too_old = i64::from(message.date) + CONFERENCE_INVITE_RING_SECS < now_unix;
        if busy || too_old {
            self.calls
                .conference_invite_declines
                .push((message.chat_id, message.id));
            return;
        }
        self.calls.conference_invite = Some(ConferenceInvite {
            chat_id: message.chat_id,
            message_id: message.id,
            unique_id: *unique_id,
            user_id,
            is_video: *is_video,
            others: other_participants
                .iter()
                .copied()
                .filter(|other| *other != user_id)
                .collect(),
        });
    }

    /// The invitation is for the conference already current: the one
    /// ringing (the same message again, or another invitation to it), or
    /// the conference call you are in (tdesktop returns for both).
    fn is_current_conference(
        &self,
        chat_id: ChatId,
        message_id: MessageId,
        unique_id: i64,
    ) -> bool {
        let ringing = self.calls.conference_invite.as_ref().is_some_and(|invite| {
            (invite.chat_id == chat_id && invite.message_id == message_id)
                || (unique_id != 0 && invite.unique_id == unique_id)
        });
        let in_it = unique_id != 0
            && self
                .calls
                .active_group_call
                .as_ref()
                .is_some_and(|call| call.is_conference() && call.unique_id == unique_id);
        ringing || in_it
    }

    /// An incoming 1:1 call while an invitation rings: declined as busy
    /// (tdesktop's `inCall()` busy path). Returns whether it was.
    pub(crate) fn decline_call_while_invite_rings(&mut self, call: &ParsedCall) -> bool {
        let ringing_in = self.calls.conference_invite.is_some()
            && !call.is_outgoing
            && matches!(call.state, CallState::Pending { .. });
        if !ringing_in {
            return false;
        }
        if !self
            .calls
            .busy_decline_queue
            .iter()
            .any(|(id, _, _)| *id == call.id)
        {
            self.calls
                .busy_decline_queue
                .push((call.id, call.user_id, call.is_video));
        }
        true
    }

    /// `updateMessageContent`: the ringing invitation stops once you
    /// joined (`is_active`) or it was missed or declined elsewhere.
    pub(crate) fn note_conference_invite_content(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        content: &MessageContent,
    ) {
        let settled = matches!(
            content,
            MessageContent::GroupCallInvitation {
                is_active,
                was_missed,
                ..
            } if *is_active || *was_missed
        );
        if settled {
            self.forget_conference_invite(chat_id, &[message_id]);
        }
    }

    /// The invitation's message is gone, or it was answered here.
    pub fn forget_conference_invite(&mut self, chat_id: ChatId, message_ids: &[MessageId]) {
        if self.calls.conference_invite.as_ref().is_some_and(|invite| {
            invite.chat_id == chat_id && message_ids.contains(&invite.message_id)
        }) {
            self.calls.conference_invite = None;
        }
    }
}

#[cfg(test)]
#[path = "conference_invite_tests.rs"]
mod tests;
