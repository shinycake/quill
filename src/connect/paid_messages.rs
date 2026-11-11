//! Connect driver: paid messages. Asks whether the open private chat
//! charges or restricts, reads what a user paid, lets a user write for
//! free, and sets a channel's direct messages price. Nothing here spends
//! Stars: the Stars leave only with a `sendMessage` the user confirmed.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::paid_messages::PaidTarget;
use crate::paid_messages_guard::HeldSend;
use crate::state::{RequestPurpose, unix_ms_now};
use crate::telegram::envelope::CommonPayload;
use crate::telegram::envelope::{EnvelopePayload, GroupsPayload, SettingsPayload};
use crate::telegram::requests::{get_chat, get_supergroup};
use crate::telegram::requests_paid_messages::{
    allow_unpaid_messages_from_user, can_send_message_to_user, get_paid_message_revenue,
    set_chat_direct_messages_group,
};

/// Where the trusted prices live, per account.
pub(crate) const TRUST_FILE: &str = "paid_trust.json";

impl<S: JsonSender> ConnectDriver<S> {
    /// Whether an answer can change what a chat charges.
    pub(crate) fn changes_paid_prices(payload: &EnvelopePayload) -> bool {
        matches!(
            payload,
            EnvelopePayload::Settings(SettingsPayload::CanSendMessageToUser(_))
                | EnvelopePayload::Groups(
                    GroupsPayload::SupergroupFullInfo { .. }
                        | GroupsPayload::UpdateSupergroupFullInfo { .. }
                )
        ) || Self::changes_viewer_premium(payload)
    }

    /// The viewer's Premium may have changed (`updateOption is_premium`):
    /// the answers cached under the old one are old.
    pub(crate) fn changes_viewer_premium(payload: &EnvelopePayload) -> bool {
        matches!(
            payload,
            EnvelopePayload::Common(CommonPayload::UpdateOption { name, .. })
                if name == "is_premium"
        )
    }

    /// Hands the guard the prices and trusted prices it screens sends
    /// against, and writes the trusted prices when they changed.
    pub(crate) fn sync_paid_guard(&mut self) {
        let prices = self.session.paid_send_prices();
        let trusted = self.session.payments.paid_messages.trusted_prices();
        if let Some(guard) = self.sender.paid_guard() {
            guard.set_state(prices, trusted);
        }
        self.save_paid_trust();
    }

    /// Reads the account's trusted prices once.
    fn load_paid_trust(&mut self) {
        if !self.session.payments.paid_messages.trust_unread() {
            return;
        }
        let text = std::fs::read_to_string(self.paths.root.join(TRUST_FILE)).ok();
        self.session
            .payments
            .paid_messages
            .trust_loaded(text.as_deref());
        self.sync_paid_guard();
    }

    /// Writes the trusted prices when they changed; the file is clean
    /// after, or the write failed and the store says why.
    fn save_paid_trust(&mut self) {
        let Some(json) = self.session.payments.paid_messages.trust_to_write() else {
            return;
        };
        let path = self.paths.root.join(TRUST_FILE);
        let written = std::fs::create_dir_all(&self.paths.root)
            .and_then(|()| std::fs::write(path, json))
            .map_err(|err| format!("Couldn't remember this price: {err}"));
        self.session.payments.paid_messages.trust_written(written);
    }

    /// "Don't ask me again": trusts `stars` per message in a chat and
    /// keeps it across restarts.
    pub fn remember_paid_trust(&mut self, chat_id: ChatId, stars: i64) {
        self.load_paid_trust();
        self.session
            .payments
            .paid_messages
            .remember_trust(chat_id.0, stars);
        self.save_paid_trust();
        self.sync_paid_guard();
    }

    /// The send waiting for the user's confirmation, if a guard holds one.
    pub fn held_paid_send(&self) -> Option<HeldSend> {
        self.sender.paid_guard()?.held()
    }

    /// The user confirmed: sends the oldest held request with its total,
    /// remembering the price when asked to.
    pub fn release_paid_send(&mut self, remember: bool) -> Result<(), ConnectSendError> {
        let Some(held) = self.sender.paid_guard().and_then(|guard| guard.held()) else {
            return Ok(());
        };
        // The send leaves the guard only once it went out: a refused send
        // keeps it held, so the box can pay again. The price is trusted
        // only after that, so a send that never left does not skip the next
        // confirmation.
        self.sender.send_json(&held.with_price())?;
        if let Some(guard) = self.sender.paid_guard() {
            guard.take_held();
        }
        if remember {
            self.remember_paid_trust(ChatId(held.chat_id), held.per_message);
        }
        Ok(())
    }

    /// The user cancelled: drops the oldest held request and its pending
    /// entry.
    pub fn discard_paid_send(&mut self) {
        let Some(held) = self.sender.paid_guard().and_then(|guard| guard.take_held()) else {
            return;
        };
        if let Some(extra) = held.extra().and_then(|e| e.parse::<u64>().ok()) {
            self.session.requests.take(RequestId(extra));
        }
    }

    /// For the open private chat with another user: the paid-messages
    /// answer (`canSendMessageToUser`, skipped for contacts and bots, which
    /// are never gated). Cached for a few minutes; this runs on every chat
    /// open. What the user pays the account needs no request here: TDLib
    /// pushes `updateUserFullInfo` for an opened private chat.
    pub(crate) fn maybe_fetch_paid_messages_info(&mut self) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        self.load_paid_trust();
        let Some(chat_id) = self.session.open_chat_id() else {
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
        let premium = self.session.my_is_premium();
        if is_contact
            || !self
                .session
                .payments
                .paid_messages
                .should_ask(user_id, now, premium)
        {
            return Ok(());
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::CanSendMessageToUser, user_id);
        self.session
            .payments
            .paid_messages
            .mark_asking(user_id, extra, now, premium);
        if let Err(err) = self
            .sender
            .send_json(&can_send_message_to_user(extra, user_id))
        {
            self.session.requests.take(extra);
            self.session
                .payments
                .paid_messages
                .ask_failed(user_id, extra, now);
            return Err(err);
        }
        Ok(())
    }

    /// The price of a channel's direct messages group, when the channel
    /// settings open and it is not known: `getSupergroup` of the group, or
    /// `getChat` first when the group's chat is not loaded.
    pub fn fetch_direct_messages_price(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Ok(());
        }
        let Some(dm_chat) = self.session.chat_direct_messages_chat(chat_id) else {
            return Ok(());
        };
        let group = self.session.chat_supergroup(dm_chat);
        if group.is_some_and(|id| {
            self.session
                .payments
                .paid_messages
                .dm_price
                .contains_key(&id)
        }) {
            return Ok(());
        }
        let purpose = if group.is_some() {
            RequestPurpose::GetSupergroup
        } else {
            RequestPurpose::LoadDirectMessagesChat
        };
        if self.session.requests.has_purpose_for_chat(purpose, dm_chat) {
            return Ok(());
        }
        let extra = self.session.request(purpose, Some(dm_chat));
        let json = match group {
            Some(id) => get_supergroup(extra, id),
            None => get_chat(extra, dm_chat),
        };
        self.send_json_request(extra, &json).map(|_| ())
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
        let target = PaidTarget::User(user_id);
        // A save of this user already out is the one that answers.
        if let Some(out) = self.session.payments.paid_messages.save.request_for(target) {
            return Ok(out);
        }
        let extra = self
            .session
            .request_for_user(RequestPurpose::AllowUnpaidMessages, user_id);
        let json = allow_unpaid_messages_from_user(extra, user_id, refund);
        let sent = self.send_json_request(extra, &json)?;
        self.session
            .payments
            .paid_messages
            .save
            .begin(target, extra);
        Ok(sent)
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
        let target = PaidTarget::Channel(chat_id.0);
        if let Some(out) = self.session.payments.paid_messages.save.request_for(target) {
            return Ok(out);
        }
        let extra = self
            .session
            .request(RequestPurpose::SetDirectMessagesGroup, Some(chat_id));
        let json = set_chat_direct_messages_group(extra, chat_id, choice.enabled, choice.stars);
        let sent = self.send_json_request(extra, &json)?;
        self.session
            .payments
            .paid_messages
            .save
            .begin(target, extra);
        Ok(sent)
    }
}
