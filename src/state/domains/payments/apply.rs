//! Applies TDLib updates and answers for payments, Premium, Stars and gifts.
use crate::state::*;
use crate::telegram::envelope::PaymentsPayload;

impl Session {
    /// Applies one payments payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_payments_payload(
        &mut self,
        payload: PaymentsPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            PaymentsPayload::PaymentForm(form) => {
                // Slice P1: `getPaymentForm` answer to our own Buy press
                // (matched by `@extra`). Opens the checkout dialog.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentForm) {
                    self.payments.form = Some(form);
                    self.payments.form_loading = false;
                    self.payments.validated = None;
                    self.payments.shipping_id = None;
                    self.payments.note = None;
                }
            }
            PaymentsPayload::ValidatedOrderInfo(validated) => {
                // Slice P1: `validateOrderInfo` answer (matched by `@extra`).
                // The first shipping option is pre-selected, like the
                // official clients.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ValidateOrderInfo) {
                    self.payments.shipping_id =
                        validated.shipping_options.first().map(|o| o.id.clone());
                    self.payments.validated = Some(validated);
                    self.payments.note = None;
                }
            }
            PaymentsPayload::PaymentResult(result) => {
                // Slice P1: `sendPaymentForm` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendPaymentForm) {
                    self.payments.sending = false;
                    if result.success {
                        self.payments.note = Some("✅ Payment successful".to_string());
                    } else if !result.verification_url.is_empty() {
                        // Schema: the URL is for additional payment
                        // credentials verification (e.g. 3-D Secure) — the
                        // UI opens it in the OS browser.
                        self.payments.verification_url = Some(result.verification_url);
                    } else {
                        self.payments.note = Some("Payment failed".to_string());
                    }
                }
            }
            PaymentsPayload::MarketplaceGift(quote) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMarketplaceGift)
                    && let Some(gift) = self.payments.marketplace_gift.as_mut()
                {
                    gift.loading = false;
                    if let Some(quote) = quote.filter(|q| q.name == gift.requested_name) {
                        gift.price = quote.stars.or(quote.ton);
                        gift.note = gift
                            .price
                            .is_none()
                            .then(|| "This gift is not available for resale.".into());
                        gift.quote = Some(quote);
                    } else {
                        gift.note =
                            Some("Could not verify the returned gift. Load the gift again.".into());
                    }
                }
            }
            PaymentsPayload::GiftTextLimit(limit) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGiftTextLimit) {
                    self.payments.gift_text_length_max = usize::try_from(limit).ok();
                }
            }
            PaymentsPayload::GiftPurchaseResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMarketplaceGift)
                    && let Some(gift) = self.payments.marketplace_gift.as_mut()
                {
                    gift.sending = false;
                    match result {
                        crate::marketplace::GiftPurchaseResult::Sent(_) => {
                            // The receipt id is intentionally empty for gifts sent to others.
                            gift.completed = true;
                            gift.note = Some("Gift sent successfully.".into());
                            gift.price = None;
                        }
                        crate::marketplace::GiftPurchaseResult::PriceIncreased(price) => {
                            gift.price = price;
                            if price.is_none() {
                                gift.quote = None;
                            }
                            if let (Some(q), Some(price)) = (gift.quote.as_mut(), price) {
                                match price {
                                    crate::marketplace::GiftPrice::Stars(_) => {
                                        q.stars = Some(price)
                                    }
                                    crate::marketplace::GiftPrice::TonCents(_) => {
                                        q.ton = Some(price)
                                    }
                                }
                            }
                            gift.note=Some("The price increased. Review the new amount and confirm again; nothing was purchased.".into());
                        }
                    }
                }
            }
            PaymentsPayload::PaymentReceipt(receipt) => {
                // Slice P1: `getPaymentReceipt` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentReceipt) {
                    self.payments.receipt = Some(receipt);
                    self.payments.receipt_open = true;
                }
            }
            PaymentsPayload::StarSubscriptions(subs) => {
                // Slice `parity:bots-payment-recurring`:
                // `getStarSubscriptions` answer to our own fetch (matched
                // by `@extra`). Follow-up pages append; a fresh fetch
                // replaces.
                if let Some(p) = pending
                    && let RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions {
                        append,
                    }) = p.purpose
                {
                    self.payments.star_subscriptions_loading = false;
                    self.payments.star_subscriptions_error = None;
                    self.payments.star_subscriptions_stale = false;
                    self.payments.star_subscriptions_offset = subs.next_offset.clone();
                    if append && let Some(existing) = self.payments.star_subscriptions.as_mut() {
                        existing.star_amount = subs.star_amount;
                        existing.required_star_count = subs.required_star_count;
                        existing.subscriptions.extend(subs.subscriptions);
                    } else {
                        self.payments.star_subscriptions = Some(subs);
                    }
                }
            }
            PaymentsPayload::StarTransactions(page) => {
                if let Some(p) = pending
                    && let RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append }) =
                        p.purpose
                    && p.id.0 == self.payments.hub.tx_request
                {
                    self.payments.hub.apply_transactions(page, append);
                }
            }
            PaymentsPayload::ReceivedGifts(page) => {
                if let Some(p) = pending
                    && let RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append }) =
                        p.purpose
                    && p.id.0 == self.payments.hub.gifts_request
                {
                    let files: Vec<ParsedFile> = page
                        .gifts
                        .iter()
                        .flat_map(|gift| gift.gift.files.iter().cloned())
                        .collect();
                    self.remember_files(&files);
                    self.payments.hub.apply_gifts(page, append);
                }
            }
            PaymentsPayload::PremiumFeatures(info) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPremiumFeatures) {
                    self.payments.hub.premium_loading = false;
                    self.payments.hub.premium_error = None;
                    self.payments.hub.premium = Some(info);
                }
            }
            PaymentsPayload::PremiumState(info) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPremiumState) {
                    self.payments.hub.premium_state = Some(info);
                }
            }
            PaymentsPayload::UpdateOwnedStarCount(amount) => {
                self.payments.hub.balance = Some(amount);
            }
        }
    }
}
