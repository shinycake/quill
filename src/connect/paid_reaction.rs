//! Connect driver: Star (paid) reactions and the Star pack list.
//! Never spends Stars on its own: a tap is held for the Undo window and
//! committed by [`ConnectDriver::paid_reaction_tick`].
use super::*;
use crate::ids::{ChatId, MessageId, RequestId};
use crate::paid_reaction::PaidSender;
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::requests::{
    add_pending_paid_message_reaction, commit_pending_paid_message_reactions,
    get_chat_available_paid_message_reaction_senders, get_star_payment_options,
    remove_pending_paid_message_reactions,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Adds `stars` to the pending Star reaction of a message and starts
    /// (or restarts) its Undo window. A refusal (balance, cap) is left in
    /// `session.payments.paid.note`.
    pub fn add_paid_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        stars: i64,
        sender: Option<PaidSender>,
        now_ms: u64,
    ) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() || message_id.0 <= 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let balance = self.session.payments.hub.balance.map(|b| b.stars);
        let paid = &mut self.session.payments.paid;
        if let Some(refusal) = paid.refusal(chat_id.0, message_id.0, stars, balance) {
            paid.note = Some(refusal.message().to_string());
            return Err(ConnectSendError::InvalidRequest);
        }
        paid.note = None;
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::AddPendingPaidReaction {
                message_id: message_id.0,
                stars,
            }),
            Some(chat_id),
        );
        let json = add_pending_paid_message_reaction(extra, chat_id, message_id, stars, sender);
        self.send_json_request(extra, &json)?;
        self.session
            .payments
            .paid
            .add(chat_id.0, message_id.0, stars, sender, now_ms);
        Ok(())
    }

    /// The toast's Undo: forgets the waiting Stars and tells TDLib.
    pub fn undo_paid_reaction(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
    ) -> Result<(), ConnectSendError> {
        if !self.session.payments.paid.undo(chat_id.0, message_id.0) {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::RemovePendingPaidReaction {
                message_id: message_id.0,
            }),
            Some(chat_id),
        );
        let json = remove_pending_paid_message_reactions(extra, chat_id, message_id);
        self.send_json_request(extra, &json).map(|_| ())
    }

    /// Commits every Star reaction whose Undo window is over. Returns
    /// whether anything was sent (the toast needs a redraw).
    pub fn paid_reaction_tick(&mut self, now_ms: u64) -> bool {
        let due = self.session.payments.paid.take_due(now_ms);
        let any = !due.is_empty();
        for pending in due {
            let chat_id = ChatId(pending.chat_id);
            let message_id = MessageId(pending.message_id);
            let extra = self.session.request(
                RequestPurpose::Payments(PaymentsPurpose::CommitPaidReaction {
                    message_id: pending.message_id,
                }),
                Some(chat_id),
            );
            let json = commit_pending_paid_message_reactions(extra, chat_id, message_id);
            if self.send_json_request(extra, &json).is_err() {
                self.session.payments.paid.note = Some("Stars were not sent".to_string());
            }
        }
        any
    }

    /// Asks which channels the account may send Stars as in a chat, once
    /// per chat.
    pub fn maybe_fetch_paid_reaction_senders(
        &mut self,
        chat_id: ChatId,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.payments.paid.senders.as_ref().map(|(c, _)| *c) == Some(chat_id.0) {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPaidReactionSenders, Some(chat_id));
        let json = get_chat_available_paid_message_reaction_senders(extra, chat_id);
        self.send_json_request(extra, &json).map(Some)
    }

    /// Fetches the Star packs once for the "Buy more Stars" list.
    pub fn maybe_fetch_star_packs(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let packs = &mut self.session.payments.star_packs;
        if packs.loaded || packs.loading {
            return Ok(None);
        }
        packs.loading = true;
        packs.error = None;
        let extra = self
            .session
            .request(RequestPurpose::GetStarPackOptions, None);
        let json = get_star_payment_options(extra);
        match self.send_json_request(extra, &json) {
            Ok(id) => Ok(Some(id)),
            Err(err) => {
                self.session.payments.star_packs.loading = false;
                Err(err)
            }
        }
    }
}
