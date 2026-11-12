//! Reducer for received-gift management and the send-gift dialog
//! (`crate::gift_manage`). Nothing here is optimistic: a success marks the
//! lists stale and the connect driver refetches them from the server.
use crate::earn::EarnErrorHint;
use crate::gift_manage::{
    ExportError, ExportSend, GiftEvent, GiftListFetch, GiftManagePayload, GiftManageState,
    ProbeOutcome, SendStage, SendStep, TransferStage,
};
use crate::lang::{keys, tr_with};
use crate::state::*;
use crate::telegram::envelope::ParsedFile;

impl Session {
    /// A `getOption` answer for one of the gift limits.
    pub(crate) fn apply_gift_option(&mut self, value: i64, pending: Option<&PendingRequest>) {
        if let Some(RequestPurpose::Payments(PaymentsPurpose::GetGiftOption(option))) =
            pending.map(|p| p.purpose)
        {
            self.payments.gifts_manage.limits.set(option, value);
            self.payments
                .gifts_manage
                .limits_ask
                .settle(pending.map(|p| p.id));
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
            (
                GiftManagePayload::Collections(list),
                PaymentsPurpose::GetGiftCollections { owner },
            ) => {
                if self.payments.hub.received.owner != Some(owner)
                    || !self
                        .payments
                        .gifts_manage
                        .collections_fetch
                        .waits_for(pending.map(|p| p.id))
                {
                    return;
                }
                let files: Vec<ParsedFile> =
                    list.iter().flat_map(|c| c.files.iter().cloned()).collect();
                self.remember_files(&files);
                let manage = &mut self.payments.gifts_manage;
                manage
                    .collections_fetch
                    .answered(pending.map(|p| p.id), list);
                if manage.active_collection != 0
                    && manage.collection(manage.active_collection).is_none()
                {
                    manage.active_collection = 0;
                    self.payments.hub.received.op.mark_stale();
                }
            }
            (GiftManagePayload::Collection(_), PaymentsPurpose::ChangeGiftCollection) => {
                self.payments.gifts_manage.collections_fetch.mark_stale();
                self.payments
                    .gifts_manage
                    .close_flow_answered_by(pending.map(|p| p.id));
                self.payments.hub.received.op.changed(pending.map(|p| p.id));
                self.payments.hub.received.notice = None;
            }
            (GiftManagePayload::Upgraded(summary), PaymentsPurpose::UpgradeGift) => {
                let hub = &mut self.payments.hub;
                hub.received.op.changed(pending.map(|p| p.id));
                hub.received.notice = None;
                hub.received.view.select(None);
                hub.stars.page.invalidate();
                let manage = &mut self.payments.gifts_manage;
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
                if !send.asked_gifts(pending) {
                    return;
                }
                send.set_gifts_fetch(GiftListFetch::Answered(gifts));
            }
            (
                GiftManagePayload::PremiumOptions(options),
                PaymentsPurpose::GetPremiumGiftOptions,
            ) => {
                let send = &mut self.payments.gifts_manage.send;
                if !send.asked_premium(pending) {
                    return;
                }
                send.set_premium_fetch(GiftListFetch::Answered(options));
            }
            (
                GiftManagePayload::TemporaryPassword { has_password },
                PaymentsPurpose::GiftExportTempPassword,
            ) => self.apply_gift_export_temp_password(has_password, pending.map(|p| p.id)),
            _ => {}
        }
    }

    /// An `ok` for one of the gift management requests.
    pub(crate) fn apply_gift_manage_ok(&mut self, pending: Option<&PendingRequest>) {
        let Some(RequestPurpose::Payments(purpose)) = pending.map(|p| p.purpose) else {
            return;
        };
        let request = pending.map(|p| p.id);
        let hub = &mut self.payments.hub;
        let manage = &mut self.payments.gifts_manage;
        match purpose {
            PaymentsPurpose::SetPinnedGifts => {
                hub.received.op.changed(pending.map(|p| p.id));
                hub.received.notice = None;
                manage.note = Some("Pinned gifts updated.".into());
            }
            PaymentsPurpose::TransferGift => {
                hub.received.op.changed(pending.map(|p| p.id));
                hub.received.notice = None;
                hub.stars.page.invalidate();
                // tdesktop `ShowGiftTransferredToast`, then the recipient's
                // chat (`showPeerHistory`). Only the picker that sent this
                // transfer closes and names its recipient; another flow
                // open by now stays.
                let sent_by = match (manage.transfer(), pending.map(|p| p.id)) {
                    (Some(picker), Some(request)) => picker
                        .transferred_by(request)
                        .map(|done| (done, picker.id.clone())),
                    _ => None,
                };
                if let Some((done, gift_id)) = sent_by {
                    manage.events.push(GiftEvent::Transferred(done));
                    manage.flow = None;
                    hub.received.view.close_if_selected(Some(gift_id.as_str()));
                }
                manage.note = Some("Gift transferred.".into());
            }
            PaymentsPurpose::SetGiftResalePrice => {
                hub.received.op.changed(pending.map(|p| p.id));
                hub.received.notice = None;
                hub.received
                    .view
                    .close_form_answered_by(pending.map(|p| p.id));
                manage.note = Some("Resale price updated.".into());
            }
            PaymentsPurpose::DeleteGiftCollection => {
                hub.received.op.changed(pending.map(|p| p.id));
                hub.received.notice = None;
                manage.close_flow_answered_by(pending.map(|p| p.id));
                manage.active_collection = 0;
                manage.collections_fetch.mark_stale();
                manage.note = Some("Collection deleted.".into());
            }
            PaymentsPurpose::DeleteGift => self.finish_gift_delete(
                pending.and_then(|p| p.subject_id().map(str::to_owned)),
                request,
            ),
            PaymentsPurpose::SendStarGift => {
                // Stars were spent whichever dialog asked.
                hub.stars.page.invalidate();
                let ours = manage.send.waits_for(pending);
                let shown = ours && matches!(manage.send.step(), Some(SendStep::Sending(_)));
                if ours {
                    if shown {
                        manage.send.set_step(SendStep::Idle);
                    } else {
                        manage.send.stage = SendStage::Closed;
                    }
                }
                // A box closed or rebuilt for another recipient while it was
                // out has nobody to show it: Stars moved, so say so.
                if !shown {
                    self.payments.notices.push(PaymentNotice::new(
                        NoticeKind::Info,
                        crate::lang::tr(crate::lang::keys::Q_MONEY_GIFT_SENT),
                    ));
                    return;
                }
                manage.send.select(None);
                manage.send.set_step(SendStep::Done(
                    crate::lang::tr(crate::lang::keys::Q_MONEY_GIFT_SENT).into(),
                ));
            }
            _ => {}
        }
    }

    /// Whether the open gifts list is one the viewer may delete from:
    /// their own, or a channel they created (tdesktop `canTransferGifts`).
    pub fn gifts_deletable(&self) -> bool {
        let hub = &self.payments.hub;
        match hub.received.owner {
            Some(crate::telegram::envelope::MessageSender::Chat { chat_id }) => {
                self.chats.get(&chat_id).is_some_and(|chat| {
                    chat.is_channel()
                        && chat.member_status()
                            == Some(crate::telegram::envelope::ChannelMemberStatus::Creator)
                })
            }
            _ => hub.gifts_are_mine(self.my_user_id),
        }
    }

    /// `deleteGift` succeeded. tdesktop `DeleteStarGift`:
    /// `notifyGiftUpdate(Delete)` drops the gift from the grid and the
    /// box, then the toast.
    pub fn finish_gift_delete(&mut self, gift_id: Option<String>, request: Option<RequestId>) {
        let hub = &mut self.payments.hub;
        let manage = &mut self.payments.gifts_manage;
        hub.received.op.changed(request);
        hub.received.notice = None;
        if let Some(id) = gift_id {
            let before = hub.received.rows.len();
            hub.received.rows.retain(|g| g.id != id);
            if hub.received.rows.len() < before {
                hub.received.total = hub.received.total.saturating_sub(1);
            }
            hub.received.view.close_if_selected(Some(id.as_str()));
        }
        manage.collections_fetch.mark_stale();
        manage.events.push(GiftEvent::Deleted);
    }

    /// The `httpUrl` of `getUpgradedGiftWithdrawalUrl`: tdesktop opens it
    /// and closes the picker (`ExportOnBlockchain`).
    pub(crate) fn apply_gift_export_url(&mut self, url: String, request: Option<RequestId>) {
        let manage = &mut self.payments.gifts_manage;
        if !manage
            .transfer()
            .is_some_and(|picker| picker.export_waits_for(request))
        {
            return;
        }
        manage.flow = None;
        if !url.is_empty() {
            manage.events.push(GiftEvent::ExportUrl(url));
        }
    }

    /// The temporary password for "Send via Blockchain" exists: the driver
    /// asks for the URL next. Without one (TDLib should not answer that
    /// way) the box says the server failed.
    fn apply_gift_export_temp_password(&mut self, has_password: bool, request: Option<RequestId>) {
        let manage = &mut self.payments.gifts_manage;
        if !manage
            .transfer()
            .is_some_and(|picker| picker.export_waits_for(request))
        {
            return;
        }
        if has_password {
            if let Some(picker) = manage.transfer_mut() {
                picker.stage = TransferStage::Password {
                    send: ExportSend::Unlocked,
                    error: None,
                };
            }
        } else if let Some(picker) = manage.transfer_mut() {
            picker.stage = TransferStage::Password {
                send: ExportSend::Idle,
                error: Some(ExportError::ServerError),
            };
        }
        manage.password_ready();
    }

    /// The password-less probe returned a URL. It is never opened: the
    /// password box opens as after any other answer.
    pub(crate) fn apply_gift_export_probe_answer(&mut self, request: Option<RequestId>) {
        if let Some(picker) = self.payments.gifts_manage.transfer_mut() {
            picker.probe_answered(request);
        }
        self.payments.gifts_manage.password_ready();
    }

    /// A fresh `passwordState` arrived: the export step routes on it, since
    /// TDLib answers the empty-password probe itself.
    pub(crate) fn route_gift_export_on_password(&mut self, has_password: bool) {
        if let Some(picker) = self.payments.gifts_manage.transfer_mut() {
            picker.password_state_known(has_password);
        }
    }

    /// tdesktop `ExportOnBlockchain`'s probe failure: `PrePasswordErrorBox`
    /// or the password box.
    fn apply_gift_export_probe_error(&mut self, err: &TdError, request: Option<RequestId>) {
        let outcome = match err.earn_hint {
            Some(EarnErrorHint::PasswordMissing) => ProbeOutcome::NoPassword,
            Some(EarnErrorHint::TooFresh) => ProbeOutcome::TooFresh,
            _ => ProbeOutcome::Other,
        };
        // Only a state that is not being reloaded counts; the reload sent
        // with the probe routes when it lands.
        let has_password = (!self.auth_state.password_state_loading())
            .then(|| self.auth_state.password_state.get().map(|s| s.has_password))
            .flatten();
        if let Some(picker) = self.payments.gifts_manage.transfer_mut() {
            picker.probe_failed(request, outcome, has_password);
        }
        self.payments.gifts_manage.password_ready();
    }

    /// tdesktop `ExportOnBlockchain`'s failures: the password box shows a
    /// wrong password, a changed password, an expired check or a flood
    /// wait under the field (`handleCustomCheckError`); a missing or too
    /// fresh password opens the security check (`PrePasswordErrorBox`);
    /// anything else, which tdesktop toasts, also shows under the field so
    /// it is not hidden behind the picker.
    fn apply_gift_export_error(
        manage: &mut GiftManageState,
        err: &TdError,
        reason: &str,
        request: Option<RequestId>,
    ) {
        if !manage
            .transfer()
            .is_some_and(|picker| picker.export_waits_for(request))
        {
            return;
        }
        let stage = match password_check_failure(err, reason) {
            PasswordCheckFailure::Field(error) => TransferStage::Password {
                send: ExportSend::Idle,
                error: Some(error),
            },
            PasswordCheckFailure::SecurityCheck { later } => TransferStage::SecurityCheck { later },
        };
        if let Some(picker) = manage.transfer_mut() {
            picker.stage = stage;
        }
        manage.password_ready();
    }

    /// A failed gift management request: release the lock and say why.
    pub(crate) fn apply_gift_manage_error(
        &mut self,
        err: &TdError,
        purpose: PaymentsPurpose,
        pending: Option<&PendingRequest>,
    ) {
        let reason = error_reason(err);
        let manage = &mut self.payments.gifts_manage;
        manage.flow_refused(pending.map(|p| p.id));
        self.payments
            .hub
            .received
            .view
            .form_refused(pending.map(|p| p.id));
        match purpose {
            PaymentsPurpose::GetGiftOption(_) => manage.limits_ask.fail(pending.map(|p| p.id)),
            PaymentsPurpose::GetGiftCollections { owner } => {
                if self.payments.hub.received.owner == Some(owner) {
                    manage.collections_fetch.failed(pending.map(|p| p.id));
                }
            }
            PaymentsPurpose::GetAvailableGifts if !manage.send.asked_gifts(pending) => {}
            PaymentsPurpose::GetPremiumGiftOptions if !manage.send.asked_premium(pending) => {}
            PaymentsPurpose::GetAvailableGifts => {
                manage.send.set_gifts_fetch(GiftListFetch::Failed(format!(
                    "Couldn't load the gifts: {reason}"
                )));
            }
            PaymentsPurpose::GetPremiumGiftOptions => {
                manage.send.set_premium_fetch(GiftListFetch::Failed(format!(
                    "Couldn't load the Premium options: {reason}"
                )));
            }
            PaymentsPurpose::TransferGift => {
                if let (Some(picker), Some(request)) =
                    (manage.transfer_mut(), pending.map(|p| p.id))
                {
                    picker
                        .transfer_failed(request, format!("Couldn't transfer the gift: {reason}"));
                }
                self.payments.hub.received.op.failed(pending.map(|p| p.id));
            }
            PaymentsPurpose::DeleteGift => {
                self.payments.hub.received.op.failed(pending.map(|p| p.id));
                manage.note = Some(tr_with(
                    keys::Q_MONEY_COULD_NOT_DELETE_THE_GIFT,
                    &[("reason", &reason)],
                ));
            }
            PaymentsPurpose::GiftExportTempPassword | PaymentsPurpose::GiftExportUrl => {
                Self::apply_gift_export_error(manage, err, &reason, pending.map(|p| p.id))
            }
            PaymentsPurpose::GiftExportProbe => {
                self.apply_gift_export_probe_error(err, pending.map(|p| p.id))
            }
            PaymentsPurpose::SendStarGift => {
                let ours = manage.send.waits_for(pending);
                let shown = ours && matches!(manage.send.step(), Some(SendStep::Sending(_)));
                if ours && !shown {
                    manage.send.stage = SendStage::Closed;
                }
                let text = format!("Couldn't send the gift: {reason}. The gift may be sold out.");
                if shown {
                    manage.send.set_step(SendStep::Failed(text));
                } else {
                    self.payments
                        .notices
                        .push(PaymentNotice::new(NoticeKind::Error, text));
                }
            }
            _ => {
                self.payments.hub.received.op.failed(pending.map(|p| p.id));
                self.payments.hub.received.notice =
                    Some(format!("Couldn't update the gift: {reason}"));
            }
        }
    }
}

/// Where a failed request sent from a password box lands (gift export,
/// Gram withdrawal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PasswordCheckFailure {
    /// Under the password field; the box stays open.
    Field(ExportError),
    /// tdesktop `PrePasswordErrorBox`: no password or a too fresh one.
    SecurityCheck { later: bool },
}

/// tdesktop `PasscodeBox::handleCustomCheckError` and
/// `PrePasswordErrorBox`: a wrong or changed password, an expired check or
/// a flood wait show under the field; a missing or too fresh password
/// opens the security check; anything else, which tdesktop toasts, also
/// shows under the field so it is not hidden behind the box.
pub(crate) fn password_check_failure(err: &TdError, reason: &str) -> PasswordCheckFailure {
    use PasswordCheckFailure::{Field, SecurityCheck};
    match (err.earn_hint, err.class) {
        (Some(EarnErrorHint::PasswordInvalid | EarnErrorHint::SrpPasswordChanged), _) => {
            Field(ExportError::WrongPassword)
        }
        (Some(EarnErrorHint::SrpIdInvalid), _) => Field(ExportError::ServerError),
        (Some(EarnErrorHint::PasswordMissing), _) => SecurityCheck { later: false },
        (Some(EarnErrorHint::TooFresh), _) => SecurityCheck { later: true },
        (_, ErrorClass::Flood) => Field(ExportError::Flood),
        _ => Field(ExportError::Other(reason.to_string())),
    }
}
