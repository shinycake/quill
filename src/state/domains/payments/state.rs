//! Payments, receipts, Stars subscriptions, Premium and gifts: the `payments` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use crate::telegram::envelope::BankCardInfoData;

pub struct PaymentsState {
    /// Slice P1: the in-flight payment request context (`getPaymentForm`,
    /// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`).
    pub request: Option<PaymentRequest>,
    /// Slice P1: the fetched `paymentForm`, shown in the checkout dialog.
    pub form: Option<PaymentFormData>,
    /// Slice P1: `getPaymentForm` is in flight (dialog shows a spinner).
    pub form_loading: bool,
    /// Slice P1: the validated order info + shipping options from
    /// `validateOrderInfo`.
    pub validated: Option<ValidatedOrderInfoData>,
    /// Slice P1: the chosen shipping option id (default: the first).
    pub shipping_id: Option<String>,
    /// Slice P1: the fetched `paymentReceipt`, shown in the receipt dialog.
    pub receipt: Option<PaymentReceiptData>,
    /// Slice P1: receipt dialog visibility.
    pub receipt_open: bool,
    /// Slice P1: latest payment error / outcome note, shown in the
    /// checkout dialog (never a secret — order fields and credentials are
    /// never echoed here).
    pub note: Option<String>,
    /// Slice P1: `sendPaymentForm` is in flight — the Pay button shows
    /// "Processing…" and is disabled until the `paymentResult` answer (or
    /// error) lands, so a double-click can't submit twice.
    pub sending: bool,
    /// Slice P1: a failed `getPaymentReceipt`, drained into the status
    /// note by `poll_live` — the checkout dialog (which renders
    /// `payment_note`) may be closed when the receipt fetch fails.
    pub receipt_error: Option<String>,
    /// Slice P1: `paymentResult.verification_url` from a non-successful
    /// `sendPaymentForm` — the UI takes it on the next poll and opens it
    /// in the OS browser (3-D Secure and similar).
    pub verification_url: Option<String>,
    pub marketplace_gift: Option<crate::marketplace::GiftPurchase>,
    pub gift_text_length_max: Option<usize>,
    /// Slice `parity:bots-payment-recurring`: the fetched
    /// `starSubscriptions`, shown in the Subscriptions dialog.
    pub star_subscriptions: Option<StarSubscriptionsData>,
    /// Slice `parity:bots-payment-recurring`: `getStarSubscriptions` is in
    /// flight (dialog shows a spinner).
    pub star_subscriptions_loading: bool,
    /// Slice `parity:bots-payment-recurring`: latest subscriptions error,
    /// shown in the dialog (never a secret — ids are opaque TDLib strings).
    pub star_subscriptions_error: Option<String>,
    /// Slice `parity:bots-payment-recurring`: pagination offset for the
    /// next `getStarSubscriptions` page (empty = no more pages).
    pub star_subscriptions_offset: String,
    /// Slice `parity:bots-payment-recurring`: a cancel/rejoin mutation
    /// landed — the list refetches on the next pump (the
    /// `sessions_stale` pattern; never optimistic).
    pub star_subscriptions_stale: bool,
    /// Slice `parity:bots-payment-recurring`: an `editStarSubscription` /
    /// `reuseStarSubscription` is in flight — the dialog disables its
    /// action buttons until the `ok` (or error) lands.
    pub star_subscriptions_mutating: bool,
    /// Slice `parity:bots-payment-recurring`: the Subscriptions dialog is
    /// on screen.
    pub subscriptions_open: bool,
    /// Premium / Stars / received-gifts hub state (`crate::premium_hub`).
    pub hub: crate::premium_hub::PremiumHub,
    /// Slice `parity:bots-payment-recurring`: subscription id awaiting
    /// cancel confirmation in the dialog.
    pub subscription_cancel_confirm: Option<String>,
    /// TDLib's `is_premium` option: the account's current Premium state.
    /// `None` until the option arrives.
    pub premium_option: Option<bool>,
    /// The card number the user tapped and what `getBankCardInfo` said.
    pub bank_card: Option<BankCardLookup>,
    /// The gift code link whose box is open.
    pub gift_code: Option<GiftCodeLookup>,
}

/// A tapped bank card number and the lookup's progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankCardLookup {
    pub number: String,
    /// `None` while loading or after a failure.
    pub info: Option<BankCardInfoData>,
    pub loading: bool,
}

impl PaymentsState {
    pub(crate) fn new() -> Self {
        Self {
            request: None,
            form: None,
            form_loading: false,
            validated: None,
            shipping_id: None,
            receipt: None,
            receipt_open: false,
            note: None,
            sending: false,
            receipt_error: None,
            verification_url: None,
            marketplace_gift: None,
            gift_text_length_max: None,
            star_subscriptions: None,
            star_subscriptions_loading: false,
            star_subscriptions_error: None,
            star_subscriptions_offset: String::new(),
            star_subscriptions_stale: false,
            star_subscriptions_mutating: false,
            subscriptions_open: false,
            hub: crate::premium_hub::PremiumHub::default(),
            subscription_cancel_confirm: None,
            premium_option: None,
            bank_card: None,
            gift_code: None,
        }
    }
}
