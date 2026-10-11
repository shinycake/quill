//! Connect driver: paid messages. Asks whether the open private chat
//! charges or restricts, reads what a user paid, lets a user write for
//! free, and sets a channel's direct messages price. Nothing here spends
//! Stars: the Stars leave only with a `sendMessage` the user confirmed.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::state::{RequestPurpose, unix_ms_now};
use crate::telegram::requests_paid_messages::{
    allow_unpaid_messages_from_user, can_send_message_to_user, get_paid_message_revenue,
    set_chat_direct_messages_group,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// For the open private chat with another user: the paid-messages
    /// answer (`canSendMessageToUser`, skipped for contacts and bots, which
    /// are never gated). Cached for a few minutes; this runs on every chat
    /// open. What the user pays the account needs no request here: TDLib
    /// pushes `updateUserFullInfo` for an opened private chat.
    pub(crate) fn maybe_fetch_paid_messages_info(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(chat_id) = self.session.open_chat else {
            return Ok(());
        };
        let Some(user_id) = self.session.private_chat_user_id(chat_id) else {
            return Ok(());
        };
        if Some(user_id) == self.session.my_user_id || self.session.is_bot_user(user_id) {
            return Ok(());
        }
        let is_contact = self.session.user(user_id).is_some_and(|u| u.is_contact);
        let now = unix_ms_now();
        if is_contact || !self.session.payments.paid_messages.should_ask(user_id, now) {
            return Ok(());
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::CanSendMessageToUser, user_id);
        self.session
            .payments
            .paid_messages
            .mark_asking(user_id, now);
        if let Err(err) = self
            .sender
            .send_json(&can_send_message_to_user(extra, user_id))
        {
            self.session.requests.take(extra);
            self.session.payments.paid_messages.ask_failed(user_id);
            return Err(err);
        }
        Ok(())
    }

    /// `getPaidMessageRevenue` for the Remove Fee box: how many Stars the
    /// user paid, to offer the refund. One request at a time per user.
    pub fn fetch_paid_message_revenue(&mut self, user_id: i64) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self
            .session
            .requests
            .has_purpose_for_user(RequestPurpose::GetPaidMessageRevenue, user_id)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::GetPaidMessageRevenue, user_id);
        self.send_json_request(extra, &get_paid_message_revenue(extra, user_id))
            .map(|_| ())
    }

    /// `allowUnpaidMessagesFromUser`: the user writes for free from now on.
    /// With `refund`, what they paid before goes back to them.
    pub fn allow_unpaid_messages(
        &mut self,
        user_id: i64,
        refund: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::AllowUnpaidMessages, user_id);
        self.session.payments.paid_messages.saving = true;
        self.session.payments.paid_messages.error = None;
        let json = allow_unpaid_messages_from_user(extra, user_id, refund);
        self.send_json_request(extra, &json).inspect_err(|_| {
            self.session.payments.paid_messages.saving = false;
        })
    }

    /// `setChatDirectMessagesGroup` for a channel: switch the direct
    /// messages group on or off and set its price. The price is clamped to
    /// the server limit; a disabled group has none.
    pub fn set_direct_messages_group(
        &mut self,
        chat_id: ChatId,
        enabled: bool,
        stars: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let choice = crate::paid_messages::DirectMessagesChoice { enabled, stars }
            .clamped(self.session.payments.paid_messages.max());
        let extra = self
            .session
            .request(RequestPurpose::SetDirectMessagesGroup, Some(chat_id));
        self.session.payments.paid_messages.saving = true;
        self.session.payments.paid_messages.error = None;
        let json = set_chat_direct_messages_group(extra, chat_id, choice.enabled, choice.stars);
        self.send_json_request(extra, &json).inspect_err(|_| {
            self.session.payments.paid_messages.saving = false;
        })
    }
}
