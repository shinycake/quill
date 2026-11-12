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
        extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            PaymentsPayload::GiftCodeInfo(info) => self.apply_gift_code_info(info, pending),
            PaymentsPayload::BankCardInfo(info) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetBankCardInfo)
                    && let Some(lookup) = self.payments.bank_card.as_mut()
                    && lookup.waits_for(pending.map(|p| p.id))
                {
                    lookup.stage = BankCardStage::Ready(info);
                }
            }
            PaymentsPayload::CollectibleInfo(info) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetCollectibleInfo)
                    && let Some(lookup) = self.payments.hub.collectible.as_mut()
                {
                    lookup.answer(extra, info);
                }
            }
            PaymentsPayload::PaymentForm(form) => {
                // Slice P1: `getPaymentForm` answer to our own Buy press
                // (matched by `@extra`). Opens the checkout dialog. An
                // answer to an earlier checkout's fetch is not this one's.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentForm)
                    && extra.is_some()
                    && self.payments.checkout.waits_for(extra)
                    && let CheckoutForm::Loading { invoice, .. } = &self.payments.checkout
                {
                    self.payments.checkout = CheckoutForm::ready(form, invoice.clone());
                }
            }
            PaymentsPayload::ValidatedOrderInfo(validated) => {
                // Slice P1: `validateOrderInfo` answer (matched by `@extra`).
                // The first shipping option is pre-selected, like the
                // official clients. An answer for a checkout since closed
                // is not this one's.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::ValidateOrderInfo)
                    && extra.is_some()
                    && let CheckoutForm::Ready(checkout) = &mut self.payments.checkout
                    && let OrderInfoStage::Validating { request, order } = &checkout.order_info
                    && Some(*request) == extra
                {
                    let order = order.clone();
                    checkout.shipping_id = validated.shipping_options.first().map(|o| o.id.clone());
                    checkout.order_info = OrderInfoStage::Validated {
                        order,
                        info: validated,
                    };
                    checkout.note = None;
                }
            }
            PaymentsPayload::PaymentResult(result) => {
                // Slice P1: `sendPaymentForm` answer (matched by `@extra`),
                // for the payment that sent it. The checkout on screen
                // takes the outcome only while it shows that invoice.
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendPaymentForm)
                    && let Some(paid) = self.payments.take_paying(extra)
                {
                    let shown = self.payments.shows(&paid.invoice);
                    self.payments.record_outcome(
                        &paid.invoice,
                        if result.success {
                            PaymentVerdict::Paid
                        } else if !result.verification_url.is_empty() {
                            PaymentVerdict::NeedsVerification
                        } else {
                            PaymentVerdict::Refused
                        },
                    );
                    if shown {
                        self.payments.checkout.end_verification();
                    }
                    if result.success {
                        self.note_paid_media_unlocked(&paid);
                        if let Some(chat_id) = paid.invoice.giveaway_chat_id() {
                            self.payments
                                .giveaway
                                .launches
                                .set(chat_id, GiveawayLaunch::Paid);
                        }
                        if !shown && self.payments.unlocked_notice.is_none() {
                            // The checkout was closed while it paid.
                            self.payments.notify(
                                NoticeKind::Info,
                                crate::lang::tr(crate::lang::keys::Q_MONEY_PAYMENT_SUCCESS_NOTE),
                            );
                        }
                        if shown {
                            self.payments.checkout.mark_paid();
                            self.payments.checkout.set_note(Some(format!(
                                "✅ {}",
                                crate::lang::tr(crate::lang::keys::Q_MONEY_PAYMENT_SUCCESS_NOTE)
                            )));
                        }
                    } else if !result.verification_url.is_empty() {
                        // Schema: the URL is for additional payment
                        // credentials verification (e.g. 3-D Secure) — the
                        // UI opens it in the OS browser. It is this
                        // payment's own step, shown or not.
                        // Whatever the checkout shows, the giveaway's
                        // payment is now in the provider's hands.
                        if let Some(chat_id) = paid.invoice.giveaway_chat_id() {
                            self.payments
                                .giveaway
                                .launches
                                .set(chat_id, GiveawayLaunch::Verifying);
                        }
                        self.payments.verification_url = Some(PaymentVerification {
                            url: result.verification_url,
                            closes_checkout: shown,
                            giveaway_chat_id: paid.invoice.giveaway_chat_id(),
                        });
                        if shown {
                            self.payments.checkout.mark_verifying();
                        }
                    } else if shown {
                        self.payments.checkout.set_note(Some(
                            crate::lang::tr(crate::lang::keys::Q_MONEY_PAYMENT_FAILED).to_string(),
                        ));
                    } else {
                        self.payments.notify(
                            NoticeKind::Error,
                            crate::lang::tr(crate::lang::keys::Q_MONEY_PAYMENT_FAILED),
                        );
                    }
                }
            }
            PaymentsPayload::MarketplaceGift(quote) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetMarketplaceGift)
                    && let Some(gift) = self.payments.marketplace_gift.as_mut()
                    && pending.is_some_and(|p| gift.waits_for(p.id))
                {
                    gift.accept_quote(quote);
                }
            }
            PaymentsPayload::GiftTextLimit(limit) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetGiftTextLimit) {
                    self.payments.gift_text_length_max = usize::try_from(limit).ok();
                } else {
                    self.apply_gift_option(limit, pending);
                }
            }
            PaymentsPayload::GiftPurchaseResult(result) if self.is_gift_market_buy(pending) => {
                self.apply_gift_market_purchase(result, pending.map(|p| p.id));
            }
            PaymentsPayload::GiftPurchaseResult(result) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::SendMarketplaceGift)
                    && let Some(gift) = self.payments.marketplace_gift.as_mut()
                    && pending.is_some_and(|p| gift.waits_for(p.id))
                {
                    gift.accept_result(result);
                }
            }
            PaymentsPayload::PaymentReceipt(receipt) => {
                // Slice P1: `getPaymentReceipt` answer (matched by `@extra`).
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetPaymentReceipt)
                    && self.payments.receipt.waits_for(extra)
                {
                    self.payments.receipt = ReceiptStage::Open(receipt);
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
                    // Only the page in flight lands (older ones are
                    // dropped); only a first page answers a refetch.
                    self.payments
                        .star_subscriptions
                        .page_answered(p.id, append, subs);
                }
            }
            PaymentsPayload::StarTransactions(page) if self.is_earn_request(pending) => {
                self.apply_earn_star_transactions(page, pending);
            }
            PaymentsPayload::StarTransactions(page) => {
                if let Some(p) = pending
                    && let RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append }) =
                        p.purpose
                    && self.payments.hub.stars.page.waits_for(p.id)
                {
                    self.payments.hub.apply_transactions(page, append);
                }
            }
            PaymentsPayload::ReceivedGifts(page) => {
                if let Some(p) = pending
                    && let RequestPurpose::Users(UsersPurpose::GetShareThemeGifts { first }) =
                        p.purpose
                {
                    self.apply_share_theme_gifts(page, p.id, first);
                } else if let Some(p) = pending
                    && matches!(
                        p.purpose,
                        RequestPurpose::Media(MediaPurpose::GetProfileGifts { .. })
                    )
                {
                    self.apply_profile_gifts(page, p);
                } else if let Some(p) = pending
                    && p.purpose == RequestPurpose::Payments(PaymentsPurpose::GetWearableGifts)
                {
                    let files: Vec<ParsedFile> = page
                        .gifts
                        .iter()
                        .flat_map(|gift| gift.gift.files.iter().cloned())
                        .collect();
                    self.remember_files(&files);
                    // Only the request the box waits for lands: an older
                    // answer of a closed-and-reopened box is dropped.
                    self.users_state.wearable_gifts.answered(
                        Some(p.id),
                        page.gifts
                            .into_iter()
                            .filter(|gift| gift.gift.colors.is_some())
                            .collect(),
                    );
                } else if let Some(p) = pending
                    && let RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append }) =
                        p.purpose
                    && self.payments.hub.received.page.waits_for(p.id)
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
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetPremiumFeatures
                {
                    self.payments.hub.premium.answered(Some(pending.id), info);
                }
            }
            PaymentsPayload::PremiumState(info) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetPremiumState
                {
                    self.payments
                        .hub
                        .premium_state
                        .answered(Some(pending.id), info);
                }
            }
            PaymentsPayload::BusinessFeatures(features) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetBusinessFeatures
                {
                    self.payments
                        .hub
                        .business_features
                        .answered(Some(pending.id), features);
                }
            }
            PaymentsPayload::GiveawayInfo(info) => self.apply_giveaway_info(info, pending),
            PaymentsPayload::PremiumGiveawayOptions(options) => {
                self.apply_premium_giveaway_options(options, pending)
            }
            PaymentsPayload::StarGiveawayOptions(options) => {
                self.apply_star_giveaway_options(options, pending)
            }
            PaymentsPayload::StarPackOptions(options) => {
                if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStarGiftOptions) {
                    self.payments.stars_gift.answer(extra, options);
                } else if pending.map(|p| p.purpose) == Some(RequestPurpose::GetStarPackOptions) {
                    self.payments.star_packs.options.answered(extra, options);
                }
            }
            PaymentsPayload::GiftManage(payload) => self.apply_gift_manage(payload, pending),
            PaymentsPayload::TonWallet(payload) => self.apply_ton_wallet(payload, pending),
            PaymentsPayload::Grams(payload) => self.apply_grams(payload, pending),
            PaymentsPayload::GiftValue(payload) => self.apply_gift_value(payload, pending),
            PaymentsPayload::Earn(payload) => self.apply_earn(payload, pending),
            PaymentsPayload::SuggestedPost(payload) => self.apply_suggested_post(payload),
            PaymentsPayload::GiftMarket(payload) => self.apply_gift_market(payload, pending),
            PaymentsPayload::UpdateOwnedStarCount(amount) => {
                // The subscriptions list carries its own copy of the balance.
                if let Some(subs) = self.payments.star_subscriptions.data_mut() {
                    subs.star_amount = amount.stars;
                }
                self.payments.hub.balance = Some(amount);
            }
        }
    }
}
