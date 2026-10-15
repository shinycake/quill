//! Reducer for received-gift management and the send-gift dialog
//! (`crate::gift_manage`). Nothing here is optimistic: a success marks the
//! lists stale and the connect driver refetches them from the server.
use crate::earn::EarnErrorHint;
use crate::gift_manage::{
    ExportError, GiftManagePayload, GiftManageState, GiftTransferred, TransferPicker, TransferStage,
};
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
                // tdesktop `ShowGiftTransferredToast`, then the recipient's
                // chat (`showPeerHistory`).
                manage.transferred = match manage.transfer() {
                    Some(TransferPicker {
                        gift_name,
                        stage: TransferStage::ConfirmPeer { user_id, name },
                        ..
                    }) => Some(GiftTransferred {
                        gift_name: gift_name.clone(),
                        recipient: name.clone(),
                        user_id: *user_id,
                    }),
                    _ => None,
                };
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

    /// The `httpUrl` of `getUpgradedGiftWithdrawalUrl`: tdesktop opens it
    /// and closes the picker (`ExportOnBlockchain`).
    pub(crate) fn apply_gift_export_url(&mut self, url: String) {
        let manage = &mut self.payments.gifts_manage;
        if !manage.transfer().is_some_and(TransferPicker::exporting) {
            return;
        }
        manage.flow = None;
        if !url.is_empty() {
            manage.export_url = Some(url);
        }
    }

    /// tdesktop `ExportOnBlockchain`'s failures: the password box shows a
    /// wrong password or a flood wait under the field
    /// (`handleCustomCheckError`); a missing or too fresh password opens
    /// the security check (`PrePasswordErrorBox`); anything else is a
    /// toast with the box left open.
    fn apply_gift_export_error(manage: &mut GiftManageState, err: &TdError, reason: &str) {
        if !manage.transfer().is_some_and(TransferPicker::exporting) {
            return;
        }
        let stage = match (err.earn_hint, err.class) {
            (Some(EarnErrorHint::PasswordInvalid), _) => TransferStage::Password {
                sending: false,
                error: Some(ExportError::WrongPassword),
            },
            (Some(EarnErrorHint::PasswordMissing), _) => {
                TransferStage::SecurityCheck { later: false }
            }
            (Some(EarnErrorHint::TooFresh), _) => TransferStage::SecurityCheck { later: true },
            (_, ErrorClass::Flood) => TransferStage::Password {
                sending: false,
                error: Some(ExportError::Flood),
            },
            _ => {
                manage.note = Some(format!("Couldn't open Fragment: {reason}"));
                TransferStage::Password {
                    sending: false,
                    error: None,
                }
            }
        };
        if let Some(picker) = manage.transfer_mut() {
            picker.stage = stage;
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
            PaymentsPurpose::GiftExportUrl => Self::apply_gift_export_error(manage, err, &reason),
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
