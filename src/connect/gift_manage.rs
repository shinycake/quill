//! Connect driver: owner actions on received gifts (pin, collections,
//! upgrade, transfer, resale) and the send-gift dialog. Every method checks
//! the same rules the UI shows (`crate::gift_manage`), so a stale button
//! can never build a request the gift does not allow. The three that spend
//! Stars (`upgradeGift`, `transferGift`, `sendGift`) are only called from a
//! confirmation button that names the price.
use super::*;
use crate::gift_manage::*;
use crate::ids::{ChatId, RequestId};
use crate::local_time::now_unix;
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests_gifts as rq;

impl<S: JsonSender> ConnectDriver<S> {
    fn gifts_owner_if_mine(&self) -> Result<MessageSender, ConnectSendError> {
        let hub = &self.session.payments.hub;
        if !self.chats_path_active()
            || hub.gift_mutating
            || !hub.gifts_are_mine(self.session.my_user_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        hub.gifts_owner.ok_or(ConnectSendError::InvalidRequest)
    }

    /// Lock the list, build the request with the owner and send it; the
    /// reducer unlocks on the answer.
    fn mutate_own_gifts(
        &mut self,
        purpose: PaymentsPurpose,
        build: impl FnOnce(RequestId, MessageSender) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        let owner = self.gifts_owner_if_mine()?;
        let extra = self
            .session
            .request(RequestPurpose::Payments(purpose), None);
        self.session.payments.hub.gift_mutating = true;
        self.session.payments.hub.gifts_error = None;
        self.session.payments.gifts_manage.note = None;
        if let Err(err) = self.send_json_request(extra, &build(extra, owner)) {
            self.session.payments.hub.gift_mutating = false;
            return Err(err);
        }
        Ok(extra)
    }

    fn can_pay(&self, stars: i64) -> bool {
        stars <= 0
            || self
                .session
                .payments
                .hub
                .balance
                .is_none_or(|balance| balance.stars >= stars)
    }

    fn own_gift(&self, id: &str) -> Result<crate::premium_hub::ReceivedGift, ConnectSendError> {
        self.session
            .payments
            .hub
            .gifts
            .iter()
            .find(|g| g.id == id)
            .cloned()
            .ok_or(ConnectSendError::InvalidRequest)
    }

    /// Called when a gifts list opens: forget the previous owner's
    /// collections, ask for the limits once and load the owner's
    /// collections.
    pub(super) fn begin_gift_manage(&mut self, owner: MessageSender) {
        self.session.payments.gifts_manage.reset_for_owner();
        if !self.session.payments.gifts_manage.limits_asked {
            self.session.payments.gifts_manage.limits_asked = true;
            for option in GiftOption::ALL {
                let extra = self.session.request(
                    RequestPurpose::Payments(PaymentsPurpose::GetGiftOption(option)),
                    None,
                );
                let _ = self.send_json_request(extra, &rq::get_option(extra, option.name()));
            }
        }
        let _ = self.fetch_gift_collections(owner);
    }

    fn fetch_gift_collections(
        &mut self,
        owner: MessageSender,
    ) -> Result<RequestId, ConnectSendError> {
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetGiftCollections),
            None,
        );
        self.session.payments.gifts_manage.collections_loading = true;
        if let Err(err) = self.send_json_request(extra, &rq::get_gift_collections(extra, owner)) {
            self.session.payments.gifts_manage.collections_loading = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Reload the collections after a collection mutation marked them stale.
    pub fn refresh_gift_collections_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.payments.gifts_manage.collections_stale {
            return Ok(None);
        }
        let Some(owner) = self.session.payments.hub.gifts_owner else {
            return Ok(None);
        };
        self.session.payments.gifts_manage.collections_stale = false;
        self.fetch_gift_collections(owner).map(Some)
    }

    /// Switch the open gifts list to a collection (`0` = all gifts).
    pub fn select_gift_collection(
        &mut self,
        id: i32,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let manage = &self.session.payments.gifts_manage;
        if !self.chats_path_active()
            || self.session.payments.hub.gifts_owner.is_none()
            || (id != 0 && manage.collection(id).is_none())
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        if manage.active_collection == id {
            return Ok(None);
        }
        let manage = &mut self.session.payments.gifts_manage;
        manage.active_collection = id;
        manage.flow = None;
        manage.note = None;
        let hub = &mut self.session.payments.hub;
        hub.gifts.clear();
        hub.gifts_total = 0;
        hub.gifts_offset.clear();
        hub.gifts_loaded = false;
        hub.gifts_error = None;
        hub.gift_selected = None;
        hub.gift_convert_confirm = None;
        self.fetch_received_gifts_page(false).map(Some)
    }

    /// Pin or unpin an upgraded gift (`setPinnedGifts` with the whole order).
    pub fn set_gift_pinned(
        &mut self,
        id: &str,
        pinned: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let gift = self.own_gift(id)?;
        let hub = &self.session.payments.hub;
        let manage = &self.session.payments.gifts_manage;
        if manage.active_collection != 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let current = pinned_ids(&hub.gifts);
        let ids = match (
            gift.pin_gate(true, current.len(), manage.limits.pinned()),
            pinned,
        ) {
            (PinGate::Pin, true) => pins_with(&current, id),
            (PinGate::Unpin, false) => pins_without(&current, id),
            _ => return Err(ConnectSendError::InvalidRequest),
        };
        self.mutate_own_gifts(PaymentsPurpose::SetPinnedGifts, |extra, owner| {
            rq::set_pinned_gifts(extra, owner, &ids)
        })
    }

    /// `upgradeGift`. Spends the upgrade price unless the sender prepaid it.
    pub fn upgrade_received_gift(
        &mut self,
        id: &str,
        keep_details: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let cost = self
            .own_gift(id)?
            .upgrade_cost(true)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !self.can_pay(cost) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.mutate_own_gifts(PaymentsPurpose::UpgradeGift, |extra, _| {
            rq::upgrade_gift(extra, id, keep_details, cost)
        })
    }

    /// `transferGift` to the user or channel of `chat_id`. Gives the gift
    /// away and may cost Stars.
    pub fn transfer_received_gift(
        &mut self,
        id: &str,
        chat_id: ChatId,
    ) -> Result<RequestId, ConnectSendError> {
        let TransferGate::Ready { star_count } = self.own_gift(id)?.transfer_gate(true, now_unix())
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let recipient = self
            .session
            .gift_recipient(chat_id)
            .filter(|r| !r.is_self)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !self.can_pay(star_count) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.mutate_own_gifts(PaymentsPurpose::TransferGift, |extra, _| {
            rq::transfer_gift(extra, id, recipient.sender, star_count)
        })
    }

    /// The Transfer button of the transfer picker's confirmation:
    /// `transferGift` to the contact the confirmation names. Refused unless
    /// that confirmation is the one on screen.
    pub fn transfer_picked_gift(&mut self) -> Result<RequestId, ConnectSendError> {
        let Some(TransferPicker {
            id,
            stage: TransferStage::ConfirmPeer { user_id, .. },
            ..
        }) = self.session.payments.gifts_manage.transfer().cloned()
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let TransferGate::Ready { star_count } =
            self.own_gift(&id)?.transfer_gate(true, now_unix())
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let person = self
            .session
            .users
            .get(&user_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        if Some(user_id) == self.session.my_user_id || person.is_bot || !self.can_pay(star_count) {
            return Err(ConnectSendError::InvalidRequest);
        }
        let to = MessageSender::User { user_id };
        self.mutate_own_gifts(PaymentsPurpose::TransferGift, |extra, _| {
            rq::transfer_gift(extra, &id, to, star_count)
        })
    }

    /// Submit the password box of "Send via Blockchain":
    /// `getUpgradedGiftWithdrawalUrl`. Only from the password stage (after
    /// "Open Fragment"), with a typed password, for a gift whose export
    /// is unlocked. The answer is a Fragment URL; nothing moves here. The
    /// password goes into this one request and is not kept.
    pub fn export_picked_gift(&mut self, password: &str) -> Result<RequestId, ConnectSendError> {
        let Some(TransferPicker {
            id,
            stage: TransferStage::Password { sending: false, .. },
            ..
        }) = self.session.payments.gifts_manage.transfer().cloned()
        else {
            return Err(ConnectSendError::InvalidRequest);
        };
        if password.is_empty()
            || !self.chats_path_active()
            || self.own_gift(&id)?.export_state(now_unix()) != ExportState::Ready
            || !self
                .session
                .payments
                .hub
                .gifts_are_mine(self.session.my_user_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GiftExportUrl),
            None,
        );
        let set_stage = |driver: &mut Self, sending: bool| {
            if let Some(picker) = driver.session.payments.gifts_manage.transfer_mut() {
                picker.stage = TransferStage::Password {
                    sending,
                    error: None,
                };
            }
        };
        set_stage(self, true);
        let json = rq::get_upgraded_gift_withdrawal_url(extra, &id, password);
        if let Err(err) = self.send_json_request(extra, &json) {
            set_stage(self, false);
            return Err(err);
        }
        Ok(extra)
    }

    /// `setGiftResalePrice`: `Some(stars)` lists the collectible, `None`
    /// takes it off sale.
    pub fn set_received_gift_resale_price(
        &mut self,
        id: &str,
        price: Option<i64>,
    ) -> Result<RequestId, ConnectSendError> {
        let gift = self.own_gift(id)?;
        let limits = self.session.payments.gifts_manage.limits;
        let ok = match (gift.resale_gate(true, now_unix()), price) {
            (ResaleGate::Ready, Some(stars)) => limits.resale_price_ok(stars),
            (ResaleGate::Ready | ResaleGate::TonOnly, None) => gift.manage.listed,
            _ => false,
        };
        if !ok {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.mutate_own_gifts(PaymentsPurpose::SetGiftResalePrice, |extra, _| {
            rq::set_gift_resale_price(extra, id, price)
        })
    }

    pub fn create_gift_collection(&mut self, name: &str) -> Result<RequestId, ConnectSendError> {
        let manage = &self.session.payments.gifts_manage;
        if !valid_collection_name(name)
            || manage.collections.len() as i64 >= manage.limits.collections()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let name = name.trim().to_string();
        self.mutate_own_gifts(PaymentsPurpose::ChangeGiftCollection, |extra, owner| {
            rq::create_gift_collection(extra, owner, &name, &[])
        })
    }

    pub fn rename_gift_collection(
        &mut self,
        id: i32,
        name: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if !valid_collection_name(name)
            || self.session.payments.gifts_manage.collection(id).is_none()
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let name = name.trim().to_string();
        self.mutate_own_gifts(PaymentsPurpose::ChangeGiftCollection, |extra, owner| {
            rq::set_gift_collection_name(extra, owner, id, &name)
        })
    }

    pub fn delete_gift_collection(&mut self, id: i32) -> Result<RequestId, ConnectSendError> {
        if self.session.payments.gifts_manage.collection(id).is_none() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.mutate_own_gifts(PaymentsPurpose::DeleteGiftCollection, |extra, owner| {
            rq::delete_gift_collection(extra, owner, id)
        })
    }

    /// Add a gift to a collection, or remove it from one.
    pub fn change_gift_collection(
        &mut self,
        collection_id: i32,
        gift_id: &str,
        add: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let gift = self.own_gift(gift_id)?;
        if self
            .session
            .payments
            .gifts_manage
            .collection(collection_id)
            .is_none()
            || gift.in_collection(collection_id) == add
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let ids = [gift_id.to_string()];
        self.mutate_own_gifts(PaymentsPurpose::ChangeGiftCollection, |extra, owner| {
            rq::change_gift_collection_gifts(extra, owner, collection_id, &ids, add)
        })
    }

    // ---- Sending ------------------------------------------------------

    /// Open the send-gift dialog for the user or channel of `chat_id` and
    /// load the gifts. Nothing is bought here.
    pub fn open_send_gift(&mut self, chat_id: ChatId) -> Result<(), ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let recipient = self
            .session
            .gift_recipient(chat_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let with_premium = !recipient.is_self && !recipient.is_channel;
        let send = &mut self.session.payments.gifts_manage.send;
        *send = SendGiftState {
            open: true,
            recipient: Some(recipient),
            gifts_loading: true,
            ..Default::default()
        };
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetAvailableGifts),
            None,
        );
        if self
            .send_json_request(extra, &rq::get_available_gifts(extra))
            .is_err()
        {
            let send = &mut self.session.payments.gifts_manage.send;
            send.gifts_loading = false;
            send.error = Some("Couldn't load the gifts.".into());
        }
        if with_premium {
            let extra = self.session.request(
                RequestPurpose::Payments(PaymentsPurpose::GetPremiumGiftOptions),
                None,
            );
            let _ = self.send_json_request(extra, &rq::get_premium_gift_payment_options(extra));
        }
        if self.session.payments.gift_text_length_max.is_none()
            && !self
                .session
                .requests
                .has_purpose(RequestPurpose::GetGiftTextLimit)
        {
            let extra = self.session.request(RequestPurpose::GetGiftTextLimit, None);
            let _ = self.send_json_request(extra, &rq::get_option(extra, "gift_text_length_max"));
        }
        // The balance arrives with the first Stars history page.
        let _ = self.maybe_fetch_star_transactions();
        Ok(())
    }

    pub fn close_send_gift(&mut self) {
        let send = &mut self.session.payments.gifts_manage.send;
        send.open = false;
        send.selected = None;
        send.error = None;
        send.done = None;
    }

    pub fn select_send_gift(&mut self, id: Option<i64>) {
        let send = &mut self.session.payments.gifts_manage.send;
        if send.sending {
            return;
        }
        send.selected = id.filter(|id| send.gifts.iter().any(|g| g.id == *id));
        send.error = None;
        send.done = None;
    }

    /// `sendGift`: pays the gift's Star price (plus the prepaid upgrade)
    /// from the balance. Only called from the confirm button, which shows
    /// the total.
    pub fn send_star_gift(
        &mut self,
        gift_id: i64,
        text: &str,
        hide_name: bool,
        pay_for_upgrade: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let send = &self.session.payments.gifts_manage.send;
        let (Some(recipient), Some(gift)) = (
            send.recipient.clone(),
            send.gifts.iter().find(|g| g.id == gift_id).cloned(),
        ) else {
            return Err(ConnectSendError::InvalidRequest);
        };
        let premium = self.session.payments.premium_option == Some(true);
        let limit = self.session.payments.gift_text_length_max;
        let text = text.trim();
        let upgrade_ok = gift.upgrade_star_count > 0 && !recipient.is_self;
        if !self.chats_path_active()
            || !send.open
            || send.sending
            || gift.gate(&recipient, premium, now_unix()) != SendGate::Ready
            || (pay_for_upgrade && !upgrade_ok)
            || (!text.is_empty() && limit.is_none_or(|max| text.chars().count() > max))
            || !self.can_pay(gift.total_cost(pay_for_upgrade))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::SendStarGift),
            None,
        );
        let send = &mut self.session.payments.gifts_manage.send;
        send.sending = true;
        send.error = None;
        send.done = None;
        let json = rq::send_gift(
            extra,
            gift_id,
            recipient.sender,
            text,
            hide_name,
            pay_for_upgrade,
        );
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.gifts_manage.send.sending = false;
            return Err(err);
        }
        Ok(extra)
    }
}
