//! Reducer for Settings → My Grams (`crate::premium_hub::ton`). Answers
//! land only when they match the request that asked for them; a stale
//! page (the tab changed in flight) is dropped.
use super::gift_manage::{PasswordCheckFailure, password_check_failure};
use crate::earn::EarnErrorHint;
use crate::premium_hub::ton::{GramWithdraw, GramsPayload, GramsPurpose, WithdrawOutcome};
use crate::state::*;

impl Session {
    pub(crate) fn apply_grams(&mut self, payload: GramsPayload, pending: Option<&PendingRequest>) {
        let purpose = match pending.map(|p| (p.purpose, p.id)) {
            Some((RequestPurpose::Payments(PaymentsPurpose::Grams(purpose)), id)) => {
                Some((purpose, id))
            }
            _ => None,
        };
        match payload {
            GramsPayload::Transactions(page) => {
                let files = page.files.clone();
                match purpose {
                    Some((GramsPurpose::Probe, request)) => {
                        let request = Some(request);
                        let grams = &mut self.payments.hub.grams;
                        if grams.probe.answered(request, ()) {
                            grams.balance_nano = Some(page.balance_nano);
                        }
                    }
                    Some((GramsPurpose::Page { append }, id))
                        if self.payments.hub.grams.load.waits_for(id) =>
                    {
                        self.remember_files(&files);
                        self.payments.hub.grams.apply_page(page, append);
                    }
                    _ => {}
                }
            }
            GramsPayload::Revenue(revenue) => {
                if matches!(purpose, Some((GramsPurpose::Revenue, _))) {
                    self.payments.hub.grams.revenue = Some(revenue);
                }
            }
            GramsPayload::RevenueStatus(status) => {
                let grams = &mut self.payments.hub.grams;
                let usd_rate = grams.revenue.map_or(0.0, |known| known.usd_rate);
                grams.revenue = Some(crate::premium_hub::ton::GramRevenue { usd_rate, ..status });
            }
        }
    }

    /// A Grams request failed. A page error counts only for the live
    /// request, so a stale one (the tab changed in flight) cannot stop the
    /// newer page's loading state.
    pub(crate) fn apply_grams_error(
        &mut self,
        err: &TdError,
        purpose: GramsPurpose,
        pending: Option<&PendingRequest>,
    ) {
        let grams = &mut self.payments.hub.grams;
        let request = pending.map(|p| p.id);
        match purpose {
            // The Settings row just stays hidden until Settings opens again.
            GramsPurpose::Probe => {
                grams.probe.failed(request, error_reason(err), false);
            }
            GramsPurpose::Revenue => {}
            GramsPurpose::PeerUser | GramsPurpose::PeerChat => {
                if let Some(request) = request {
                    grams.peer_refused(request);
                }
            }
            GramsPurpose::Page { .. } => {
                if pending.is_some_and(|p| grams.load.waits_for(p.id)) {
                    grams.load = crate::gift_market::PageLoad::Failed(error_reason(err));
                }
            }
            GramsPurpose::WithdrawalUrl => {
                Self::apply_gram_withdrawal_error(grams, err, pending.map(|p| p.id))
            }
        }
    }

    /// The `httpUrl` of `getGramWithdrawalUrl`: tdesktop opens it and
    /// closes the password box. Only an answer to the box's own request
    /// counts (matched by `@extra`); after Cancel, or for an earlier try,
    /// it is dropped.
    pub(crate) fn apply_gram_withdrawal_url(&mut self, url: String, id: Option<RequestId>) {
        let grams = &mut self.payments.hub.grams;
        if !grams.withdraw().is_some_and(|w| w.waits_for(id)) {
            return;
        }
        if url.is_empty() {
            grams.cancel_withdraw();
        } else {
            grams.put_withdraw(GramWithdraw::Finished(WithdrawOutcome::Url(url)));
        }
    }

    /// A fresh `passwordState` arrived: with no password, the idle
    /// password box becomes the security check.
    pub(crate) fn route_gram_withdrawal_on_password(&mut self, has_password: bool) {
        if let Some(withdraw) = self.payments.hub.grams.withdraw_mut() {
            withdraw.password_state_known(has_password);
        }
    }

    /// tdesktop `HandleWithdrawalButton`'s `fail`: `AMOUNT_TOO_SMALL`
    /// closes the box with a toast; the rest go through the password box's
    /// shared mapping.
    fn apply_gram_withdrawal_error(
        grams: &mut crate::premium_hub::ton::GramsHub,
        err: &TdError,
        id: Option<RequestId>,
    ) {
        if !grams.withdraw().is_some_and(|w| w.waits_for(id)) {
            return;
        }
        if err.earn_hint == Some(EarnErrorHint::AmountTooSmall) {
            grams.put_withdraw(GramWithdraw::Finished(WithdrawOutcome::TooSmall));
            return;
        }
        grams.enter_withdraw(match password_check_failure(err, &error_reason(err)) {
            PasswordCheckFailure::Field(error) => GramWithdraw::idle(Some(error)),
            PasswordCheckFailure::SecurityCheck { later } => GramWithdraw::SecurityCheck { later },
        });
    }
}
