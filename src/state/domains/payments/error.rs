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
            // No card info: the menu keeps only "Copy Card Number".
            Some(RequestPurpose::GetBankCardInfo) => {
                if let Some(lookup) = self.payments.bank_card.as_mut() {
                    lookup.loading = false;
                    lookup.info = None;
                }
            }
            // Nothing to show: a username is copied as a link (tdesktop's
            // `COLLECTIBLE_NOT_FOUND`), a phone number just stays as it is.
            Some(RequestPurpose::GetCollectibleInfo) => {
                if let Some(lookup) = self.payments.hub.collectible.as_mut() {
                    lookup.loading = false;
                    lookup.failed = Some(match err.class {
                        ErrorClass::CollectibleNotFound => "COLLECTIBLE_NOT_FOUND".to_string(),
                        _ => error_reason(err),
                    });
                }
            }
            Some(RequestPurpose::GetGiftValueInfo) => self.apply_gift_value_error(err, pending),
            Some(RequestPurpose::CheckPremiumGiftCode | RequestPurpose::ApplyPremiumGiftCode) => {
                self.apply_gift_code_error(err, pending);
            }
            Some(
                RequestPurpose::GetGiveawayInfo
                | RequestPurpose::GetPremiumGiveawayOptions
                | RequestPurpose::GetStarGiveawayOptions
                | RequestPurpose::LaunchPrepaidGiveaway,
            ) => self.apply_giveaway_error(err, pending),
            Some(RequestPurpose::Payments(PaymentsPurpose::GetAwardMembers { generation })) => {
                self.payments.giveaway.award_members.fail(
                    generation,
                    format!("Couldn't load the members: {}", error_reason(err)),
                );
            }
            Some(RequestPurpose::GetStarGiftOptions) => {
                let gift = &mut self.payments.stars_gift;
                gift.loading = false;
                gift.error = Some(format!("Couldn't load Star packs: {}", error_reason(err)));
            }
            Some(RequestPurpose::GetStarPackOptions) => {
                let packs = &mut self.payments.star_packs;
                packs.loading = false;
                packs.error = Some(format!("Couldn't load Star packs: {}", error_reason(err)));
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::AddPendingPaidReaction {
                message_id,
                stars,
            })) => {
                if let Some(chat) = pending.and_then(|p| p.chat_id) {
                    self.payments.paid.rollback(chat.0, message_id, stars);
                }
                self.payments.paid.note =
                    Some(format!("Couldn't send Stars: {}", error_reason(err)));
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::CommitPaidReaction { .. })) => {
                self.payments.paid.note =
                    Some(format!("Stars were not sent: {}", error_reason(err)));
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::RemovePendingPaidReaction {
                ..
            })) => {
                self.payments.paid.note =
                    Some(format!("Couldn't undo the Stars: {}", error_reason(err)));
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
            Some(RequestPurpose::Payments(PaymentsPurpose::GetWearableGifts)) => {
                self.users_state.wearable_gifts_loaded = true;
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
            // The Business screen keeps tdesktop's fallback order.
            Some(RequestPurpose::GetBusinessFeatures) => {
                self.payments.hub.business_features_loading = false;
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
                self.chats_state.chat_action_error = Some(format!(
                    "could not clear saved payment info (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::Payments(purpose)) if purpose.is_gift_manage() => {
                self.apply_gift_manage_error(err, purpose)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::Grams(purpose))) => {
                self.apply_grams_error(err, purpose, pending)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::TonWallet(purpose))) => {
                self.apply_ton_wallet_error(err, purpose, pending.and_then(|p| p.user_id))
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::Earn(purpose))) => {
                self.apply_earn_error(err, purpose)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(purpose))) => {
                self.apply_suggested_post_error(err, purpose)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GiftMarket(purpose))) => {
                self.apply_gift_market_error(err, purpose)
            }
            _ => {}
        }
    }
}
