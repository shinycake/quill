//! Failed requests for payments, Premium, Stars and gifts.
use crate::state::*;

impl Session {
    /// Reacts to a failed payments request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_payments_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match pending.map(|p| p.purpose) {
            // Slice P1: a payment request failed — surface the
            // reason in the checkout dialog instead of spinning
            // forever. The receipt fetch has its own error field:
            // the checkout dialog may be closed, so `payment_note`
            // (rendered only there) would stay invisible.
            // An earlier checkout's form fetch failing is not this one's.
            Some(RequestPurpose::GetPaymentForm) => {
                if extra.is_some() && self.payments.checkout.waits_for(extra) {
                    self.payments.checkout = CheckoutForm::Failed(crate::lang::tr_with(
                        crate::lang::keys::Q_MONEY_PAYMENT_FAILED_REASON,
                        &[("reason", &error_reason(err))],
                    ));
                }
            }
            Some(RequestPurpose::ValidateOrderInfo) => {
                if extra.is_some()
                    && let CheckoutForm::Ready(checkout) = &mut self.payments.checkout
                    && matches!(
                        &checkout.order_info,
                        OrderInfoStage::Validating { request, .. } if Some(*request) == extra
                    )
                {
                    checkout.order_info = OrderInfoStage::Unvalidated;
                    checkout.note = Some(crate::lang::tr_with(
                        crate::lang::keys::Q_MONEY_PAYMENT_FAILED_REASON,
                        &[("reason", &error_reason(err))],
                    ));
                }
            }
            // The payment that failed leaves the in-flight list; the note
            // is the checkout's only while it shows that invoice.
            Some(RequestPurpose::SendPaymentForm) => {
                if let Some(failed) = self.payments.take_paying(extra) {
                    self.payments
                        .record_outcome(&failed.invoice, PaymentVerdict::Refused);
                    let text = crate::lang::tr_with(
                        crate::lang::keys::Q_MONEY_PAYMENT_FAILED_REASON,
                        &[("reason", &error_reason(err))],
                    );
                    if self.payments.shows(&failed.invoice) {
                        self.payments.checkout.set_note(Some(text));
                    } else {
                        // The checkout was closed while it paid.
                        self.payments.notify(NoticeKind::Error, text);
                    }
                }
            }
            Some(RequestPurpose::GetPaymentReceipt) => {
                // A receipt the user already dismissed has nobody to tell.
                if self.payments.receipt.waits_for(extra) {
                    self.payments.receipt = ReceiptStage::Closed;
                    self.payments.receipt_error =
                        Some(format!("Receipt failed: {}", error_reason(err)));
                }
            }
            Some(RequestPurpose::GetMarketplaceGift | RequestPurpose::SendMarketplaceGift) => {
                if let Some(gift) = self.payments.marketplace_gift.as_mut()
                    && pending.is_some_and(|p| gift.waits_for(p.id))
                {
                    gift.fail(format!("Gift request failed: {}", error_reason(err)));
                }
            }
            // No card info: the menu keeps only "Copy Card Number".
            Some(RequestPurpose::GetBankCardInfo) => {
                if let Some(lookup) = self.payments.bank_card.as_mut()
                    && lookup.waits_for(pending.map(|p| p.id))
                {
                    lookup.stage = BankCardStage::Failed;
                }
            }
            // Nothing to show: a username is copied as a link (tdesktop's
            // `COLLECTIBLE_NOT_FOUND`), a phone number just stays as it is.
            Some(RequestPurpose::GetCollectibleInfo) => {
                if let Some(lookup) = self.payments.hub.collectible.as_mut() {
                    lookup.fail(
                        extra,
                        match err.class {
                            ErrorClass::CollectibleNotFound => "COLLECTIBLE_NOT_FOUND".to_string(),
                            _ => error_reason(err),
                        },
                    );
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
            Some(RequestPurpose::Payments(PaymentsPurpose::GetAwardMembers)) => {
                if let Some(request) = extra {
                    self.payments.giveaway.award_members.fail(
                        request,
                        crate::lang::tr_with(
                            crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_MEMBERS_REASON,
                            &[("reason", &error_reason(err))],
                        ),
                    );
                }
            }
            Some(RequestPurpose::GetStarGiftOptions) => {
                self.payments.stars_gift.fail(
                    extra,
                    format!("Couldn't load Star packs: {}", error_reason(err)),
                );
            }
            Some(RequestPurpose::GetStarPackOptions) => {
                self.payments.star_packs.options.failed(
                    extra,
                    format!("Couldn't load Star packs: {}", error_reason(err)),
                    true,
                );
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::AddPendingPaidReaction { .. })) => {
                if let Some(id) = extra {
                    self.payments.paid.rollback(id);
                }
                self.payments.notify(
                    NoticeKind::Error,
                    format!("Couldn't send Stars: {}", error_reason(err)),
                );
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::CommitPaidReaction { .. })) => {
                // TDLib refused the commit: its Stars are released and
                // nothing is sent again; the user is told.
                if let Some(id) = extra {
                    self.payments.paid.refused(id);
                }
                self.payments.notify(
                    NoticeKind::Error,
                    crate::lang::tr_with(
                        crate::lang::keys::Q_MONEY_STARS_NOT_SENT_REASON,
                        &[("reason", &error_reason(err))],
                    ),
                );
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::RemovePendingPaidReaction {
                ..
            })) => {
                if let Some(id) = extra {
                    self.payments.paid.refused(id);
                }
                self.payments.notify(
                    NoticeKind::Error,
                    format!("Couldn't undo the Stars: {}", error_reason(err)),
                );
            }
            // The limit is a server option, also pushed by `updateOption`:
            // a failed ask holds no value to clear (a value that arrived
            // meanwhile stays), and the next send that needs it asks again.
            Some(RequestPurpose::GetGiftTextLimit) => {}
            // Slice `parity:bots-payment-recurring`: a subscriptions
            // request failed — surface the reason in the dialog
            // instead of spinning forever; a failed mutation also
            // releases the disabled buttons.
            // Only the newest history request fails the list: an older one
            // answering late must not stop the spinner of the live one.
            Some(RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { .. })) => {
                if let Some(pending) = pending {
                    self.payments.hub.stars.page.fail(
                        pending.id,
                        format!("Couldn't load transactions: {}", error_reason(err)),
                    );
                }
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GetWearableGifts)) => {
                self.users_state
                    .wearable_gifts
                    .failed(extra, error_reason(err), false);
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { .. })) => {
                // Only the newest page's error counts (an older page's is
                // not the list on screen's).
                let counted = pending.is_some_and(|pending| {
                    self.payments.hub.received.page.fail(
                        pending.id,
                        crate::lang::tr_with(
                            crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_GIFTS_REASON,
                            &[("reason", &error_reason(err))],
                        ),
                    )
                });
                if counted {
                    // A failed refetch is not asked again every frame.
                    self.payments.hub.received.op.refetched();
                }
            }
            Some(
                RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { .. })
                | RequestPurpose::SellGift,
            ) => {
                self.payments.hub.received.op.failed(pending.map(|p| p.id));
                // Only the failed sale's own confirmation closes: a failed
                // save toggle or another gift's sale leaves it showing.
                if matches!(
                    pending.map(|p| p.purpose),
                    Some(RequestPurpose::SellGift)
                ) {
                    self.payments
                        .hub
                        .received
                        .view
                        .abandon_confirmation(pending.and_then(|p| p.subject_id.as_deref()));
                }
                self.payments.hub.received.notice =
                    Some(format!("Couldn't update the gift: {}", error_reason(err)));
            }
            // The Business screen keeps tdesktop's fallback order.
            Some(RequestPurpose::GetBusinessFeatures) => {
                self.payments
                    .hub
                    .business_features
                    .failed(extra, error_reason(err), false);
            }
            Some(RequestPurpose::GetPremiumState) => {
                self.payments.hub.premium_state.failed(
                    extra,
                    format!("Couldn't load Premium plans: {}", error_reason(err)),
                    false,
                );
            }
            Some(RequestPurpose::GetPremiumFeatures) => {
                self.payments.hub.premium.failed(
                    extra,
                    format!("Couldn't load Premium features: {}", error_reason(err)),
                    false,
                );
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GetStarSubscriptions { .. })) => {
                // Only the page in flight fails the list; an error does not
                // leave the list stale to be asked every frame.
                if let Some(request) = extra {
                    self.payments.star_subscriptions.page_failed(
                        request,
                        format!("Couldn't load subscriptions: {}", error_reason(err)),
                    );
                }
            }
            Some(RequestPurpose::EditStarSubscription | RequestPurpose::ReuseStarSubscription) => {
                self.payments.star_subscriptions.edit_failed(format!(
                    "Couldn't update the subscription: {}",
                    error_reason(err)
                ));
            }
            // Slice payments: a refused `deleteSavedOrderInfo` /
            // `deleteSavedCredentials` surfaces in the status note (the
            // UI drains `chat_action_error`); the optimistic state was
            // never changed, so nothing to roll back.
            Some(RequestPurpose::DeleteSavedOrderInfo | RequestPurpose::DeleteSavedCredentials) => {
                self.chats_state.chat_action_error.push(format!(
                    "could not clear saved payment info (error {})",
                    err.code
                ));
            }
            Some(RequestPurpose::Payments(purpose)) if purpose.is_gift_manage() => {
                self.apply_gift_manage_error(err, purpose, pending)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::Grams(purpose))) => {
                self.apply_grams_error(err, purpose, pending)
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::TonWallet(purpose))) => self
                .apply_ton_wallet_error(
                    err,
                    purpose,
                    pending.and_then(|p| p.user_id),
                    pending.map(|p| p.id),
                ),
            Some(RequestPurpose::Payments(PaymentsPurpose::Earn(purpose))) => {
                self.apply_earn_error(err, purpose, pending.map(|p| p.id))
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(purpose))) => {
                self.apply_suggested_post_error(err, purpose, pending.map(|p| p.id))
            }
            Some(RequestPurpose::Payments(PaymentsPurpose::GiftMarket(purpose))) => {
                self.apply_gift_market_error(err, purpose, pending.map(|p| p.id))
            }
            // The explorer stays the default; the next wallet or address
            // menu asks again.
            Some(RequestPurpose::GetTonExplorerUrl) => {
                self.ton_wallet
                    .explorer
                    .failed(pending.map(|p| p.id), error_reason(err), false);
            }
            // The next look at the sticker preview asks again.
            Some(RequestPurpose::GetPremiumStickerExamples) => {
                self.payments.hub.premium_stickers.failed(
                    pending.map(|p| p.id),
                    error_reason(err),
                    false,
                );
            }
            _ => {}
        }
    }
}
