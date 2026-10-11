//! Reducer for received-gift management and the send-gift dialog
//! (`crate::gift_manage`). Nothing here is optimistic: a success marks the
//! lists stale and the connect driver refetches them from the server.
use crate::gift_manage::GiftManagePayload;
use crate::state::*;
use crate::telegram::envelope::ParsedFile;

impl Session {
    /// A `getOption` answer for one of the gift limits.
    pub(crate) fn apply_gift_option(&mut self, value: i64, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Payments(PaymentsPurpose::GetGiftOption(option))) =
            pending.map(|p| p.purpose)
        {
            self.payments.gifts_manage.limits.set(option, value);
        }
    }

    pub(crate) fn apply_gift_manage(
        &mut self,
        payload: GiftManagePayload,
        pending: Option<&PendingRequest>,
    ) {
        let Some(RequestPurpose::Payments(purpose)) = pending.map(|p| p.purpose) else {
            return;
        };
        match (payload, purpose) {
            (GiftManagePayload::Collections(list), PaymentsPurpose::GetGiftCollections) => {
                let files: Vec<ParsedFile> =
                    list.iter().flat_map(|c| c.files.iter().cloned()).collect();
                self.remember_files(&files);
                let manage = &mut self.payments.gifts_manage;
                manage.collections_loading = false;
                manage.collections = list;
                if manage.active_collection != 0
                    && manage.collection(manage.active_collection).is_none()
                {
                    manage.active_collection = 0;
                    self.payments.hub.gifts_stale = true;
                }
            }
            (GiftManagePayload::Collection(_), PaymentsPurpose::ChangeGiftCollection) => {
                self.payments.gifts_manage.collections_stale = true;
                self.payments.gifts_manage.flow = None;
                self.payments.hub.gift_mutating = false;
                self.payments.hub.gifts_error = None;
                self.payments.hub.gifts_stale = true;
            }
            (GiftManagePayload::Upgraded(summary), PaymentsPurpose::UpgradeGift) => {
                let hub = &mut self.payments.hub;
                hub.gift_mutating = false;
                hub.gifts_error = None;
                hub.gift_selected = None;
                hub.gifts_stale = true;
                hub.tx_loaded = false;
                let manage = &mut self.payments.gifts_manage;
                manage.flow = None;
                manage.note = Some(if summary.title.is_empty() {
                    "Gift upgraded.".to_string()
                } else {
                    format!("Upgraded to {} #{}.", summary.title, summary.number)
                });
            }
            (GiftManagePayload::Available(gifts), PaymentsPurpose::GetAvailableGifts) => {
                let files: Vec<ParsedFile> =
                    gifts.iter().flat_map(|g| g.files.iter().cloned()).collect();
                self.remember_files(&files);
                let send = &mut self.payments.gifts_manage.send;
                send.gifts_loading = false;
                send.gifts_loaded = true;
                send.gifts = gifts;
            }
            (
                GiftManagePayload::PremiumOptions(options),
                PaymentsPurpose::GetPremiumGiftOptions,
            ) => {
                let send = &mut self.payments.gifts_manage.send;
                send.premium_loaded = true;
                send.premium_options = options;
            }
            _ => {}
        }
    }

    /// An `ok` for one of the gift management requests.
    pub(crate) fn apply_gift_manage_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Payments(purpose)) = pending.map(|p| p.purpose) else {
            return;
        };
        let hub = &mut self.payments.hub;
        let manage = &mut self.payments.gifts_manage;
        match purpose {
            PaymentsPurpose::SetPinnedGifts => {
                hub.gift_mutating = false;
                hub.gifts_error = None;
                hub.gifts_stale = true;
                manage.note = Some("Pinned gifts updated.".into());
            }
            PaymentsPurpose::TransferGift => {
                hub.gift_mutating = false;
                hub.gifts_error = None;
                hub.gift_selected = None;
                hub.gifts_stale = true;
                hub.tx_loaded = false;
                manage.flow = None;
                manage.note = Some("Gift transferred.".into());
            }
            PaymentsPurpose::SetGiftResalePrice => {
                hub.gift_mutating = false;
                hub.gifts_error = None;
                hub.gifts_stale = true;
                manage.flow = None;
                manage.note = Some("Resale price updated.".into());
            }
            PaymentsPurpose::DeleteGiftCollection => {
                hub.gift_mutating = false;
                hub.gifts_error = None;
                hub.gifts_stale = true;
                manage.flow = None;
                manage.active_collection = 0;
                manage.collections_stale = true;
                manage.note = Some("Collection deleted.".into());
            }
            PaymentsPurpose::SendStarGift => {
                hub.tx_loaded = false;
                manage.send.sending = false;
                manage.send.error = None;
                manage.send.selected = None;
                manage.send.done = Some("Gift sent.".into());
            }
            _ => {}
        }
    }

    /// A failed gift management request: release the lock and say why.
    pub(crate) fn apply_gift_manage_error(&mut self, err: &TdError, purpose: PaymentsPurpose) {
        let reason = error_reason(err);
        let manage = &mut self.payments.gifts_manage;
        match purpose {
            PaymentsPurpose::GetGiftOption(_) => {}
            PaymentsPurpose::GetGiftCollections => manage.collections_loading = false,
            PaymentsPurpose::GetAvailableGifts => {
                manage.send.gifts_loading = false;
                manage.send.gifts_loaded = true;
                manage.send.error = Some(format!("Couldn't load the gifts: {reason}"));
            }
            PaymentsPurpose::GetPremiumGiftOptions => manage.send.premium_loaded = true,
            PaymentsPurpose::SendStarGift => {
                manage.send.sending = false;
                manage.send.error = Some(format!(
                    "Couldn't send the gift: {reason}. The gift may be sold out."
                ));
            }
            _ => {
                self.payments.hub.gift_mutating = false;
                self.payments.hub.gifts_error = Some(format!("Couldn't update the gift: {reason}"));
            }
        }
    }
}
