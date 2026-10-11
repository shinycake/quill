//! Failed requests for payments, Premium, Stars and gifts.
use crate::state::*;

impl Session {
    /// Reacts to a failed payments request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_payments_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            // Slice P1: a payment request failed — surface the
            // reason in the checkout dialog instead of spinning
            // forever. The receipt fetch has its own error field:
            // the checkout dialog may be closed, so `payment_note`
            // (rendered only there) would stay invisible.
            Some(
                RequestPurpose::GetPaymentForm
                | RequestPurpose::ValidateOrderInfo
                | RequestPurpose::SendPaymentForm,
            ) => {
                self.payments.form_loading = false;
                self.payments.sending = false;
                self.payments.note = Some(format!("Payment failed: {}", error_reason(err)));
            }
            Some(RequestPurpose::GetPaymentReceipt) => {
                self.payments.receipt_error =
                    Some(format!("Receipt failed: {}", error_reason(err)));
            }
            Some(RequestPurpose::GetMarketplaceGift | RequestPurpose::SendMarketplaceGift) => {
                if let Some(gift) = self.payments.marketplace_gift.as_mut() {
                    gift.loading = false;
                    gift.sending = false;
                    gift.note = Some(format!("Gift request failed: {}", error_reason(err)));
                }
            }
            Some(RequestPurpose::GetGiftTextLimit) => {
                self.payments.gift_text_length_max = None;
            }
            // Slice `parity:bots-payment-recurring`: a subscriptions
            // request failed — surface the reason in the dialog
            // instead of spinning forever; a failed mutation also
            // releases the disabled buttons.
            Some(RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { .. })) => {
                self.payments.hub.tx_loading = false;
                self.payments.hub.tx_error =
                    Some(format!("Couldn't load transactions: {}", error_reason(err)));
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { .. })) => {
                self.payments.hub.gifts_loading = false;
                self.payments.hub.gifts_error =
                    Some(format!("Couldn't load gifts: {}", error_reason(err)));
            }
            Some(
                RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { .. })
                | RequestPurpose::SellGift,
            ) => {
                self.payments.hub.gift_mutating = false;
                self.payments.hub.gift_convert_confirm = None;
                self.payments.hub.gifts_error =
                    Some(format!("Couldn't update the gift: {}", error_reason(err)));
            }
            Some(RequestPurpose::GetPremiumFeatures) => {
                self.payments.hub.premium_loading = false;
                self.payments.hub.premium_error = Some(format!(
                    "Couldn't load Premium features: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { .. })) => {
                self.payments.star_subscriptions_loading = false;
                self.payments.star_subscriptions_error = Some(format!(
                    "Couldn't load subscriptions: {}",
                    error_reason(err)
                ));
            }
            Some(RequestPurpose::EditStarSubscription | RequestPurpose::ReuseStarSubscription) => {
                self.payments.star_subscriptions_mutating = false;
                self.payments.star_subscriptions_error = Some(format!(
                    "Couldn't update the subscription: {}",
                    error_reason(err)
                ));
            }
            // Slice payments: a refused `deleteSavedOrderInfo` /
            // `deleteSavedCredentials` surfaces in the status note (the
            // UI drains `chat_action_error`); the optimistic state was
            // never changed, so nothing to roll back.
            Some(RequestPurpose::DeleteSavedOrderInfo | RequestPurpose::DeleteSavedCredentials) => {
                self.chat_action_error = Some(format!(
                    "could not clear saved payment info (error {})",
                    err.code
                ));
            }
            _ => {}
        }
    }
}
