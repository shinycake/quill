//! Payments, receipts, Stars subscriptions, Premium and gifts: the `payments` domain's share of `Session`.
//! Added to by features in this domain only; `Session::new` builds it
//! with `new`.
use crate::state::*;
use crate::telegram::envelope::{BankCardInfoData, OrderInfoData};

/// The checkout's order info, from typed to validated. One value: Continue
/// moves it to `Validating` (dropping the earlier order id and shipping
/// choice), and only the answer to that same request promotes the snapshot
/// that was sent to `Validated`, so Pay never uses an order id for fields
/// other than the ones it was issued for. An error, a new form and a closed
/// checkout all leave it `Unvalidated`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OrderInfoStage {
    #[default]
    Unvalidated,
    Validating {
        request: RequestId,
        order: OrderInfoData,
    },
    Validated {
        order: OrderInfoData,
        info: ValidatedOrderInfoData,
    },
}

impl OrderInfoStage {
    /// The validated answer, only while it answers `order`, the fields now
    /// typed: an edit after Continue is validated again. Render and submit
    /// both ask this.
    pub fn validated_for(&self, order: &OrderInfoData) -> Option<&ValidatedOrderInfoData> {
        match self {
            Self::Validated { order: asked, info } if asked == order => Some(info),
            _ => None,
        }
    }

    /// The validated answer, once there is one for the current fields.
    pub fn validated(&self) -> Option<&ValidatedOrderInfoData> {
        match self {
            Self::Validated { info, .. } => Some(info),
            _ => None,
        }
    }
}

/// How a [`PaymentNotice`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Error,
}

/// The outcome of a payment, a gift or a reaction whose box is gone (closed
/// while the request was out), for the status bar. The poll drains the
/// queue; Stars may have moved, so the user is always told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentNotice {
    pub kind: NoticeKind,
    pub text: String,
}

impl PaymentNotice {
    pub fn new(kind: NoticeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// The receipt dialog's stage.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ReceiptStage {
    #[default]
    Closed,
    /// `getPaymentReceipt` is out under this request.
    Loading(RequestId),
    Open(PaymentReceiptData),
}

impl ReceiptStage {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open(_))
    }

    pub fn data(&self) -> Option<&PaymentReceiptData> {
        match self {
            Self::Open(data) => Some(data),
            _ => None,
        }
    }

    /// `request` is the receipt this dialog asked for.
    pub(crate) fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!((self, request), (Self::Loading(waiting), Some(id)) if *waiting == id)
    }
}

/// A payment's verification step (`paymentResult.verification_url`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentVerification {
    pub url: String,
    /// The checkout on screen showed the paid invoice when the answer
    /// came: opening the page closes that checkout, and only that one.
    pub closes_checkout: bool,
    /// The chat of the giveaway this payment buys, when it buys one: its
    /// form closes with the checkout.
    pub giveaway_chat_id: Option<i64>,
}

/// The checkout's form, one stage at a time.
///
/// `Loading` leaves on the answer of its own `getPaymentForm` (`Ready`; an
/// answer to an earlier checkout's fetch is not this one's) or its error
/// (`Closed`, with the error in `PaymentsState::note`). `Ready` leaves on
/// the `sendPaymentForm` answer: `Paid` (Pay stays off, so one form is
/// never sent twice), `Verifying` (the answer asked for a verification
/// step; nothing drains it, a mini app's checkout reads it to report
/// `pending`) or back to `Ready` when it failed. Any stage leaves with the
/// checkout closing.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CheckoutForm {
    #[default]
    Closed,
    /// `getPaymentForm` is out as `request` (the dialog shows a spinner).
    Loading {
        request: RequestId,
    },
    Ready(PaymentFormData),
    Verifying(PaymentFormData),
    Paid(PaymentFormData),
}

impl CheckoutForm {
    /// The fetched form, in any stage that has one.
    pub fn form(&self) -> Option<&PaymentFormData> {
        match self {
            Self::Ready(form) | Self::Verifying(form) | Self::Paid(form) => Some(form),
            Self::Closed | Self::Loading { .. } => None,
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    pub fn is_paid(&self) -> bool {
        matches!(self, Self::Paid(_))
    }

    pub fn is_verifying(&self) -> bool {
        matches!(self, Self::Verifying(_))
    }

    /// `request` is the form this checkout waits for.
    pub(crate) fn waits_for(&self, request: Option<RequestId>) -> bool {
        matches!((self, request), (Self::Loading { request: asked }, Some(id)) if *asked == id)
    }

    /// The form was paid.
    pub fn mark_paid(&mut self) {
        if let Some(form) = self.form().cloned() {
            *self = Self::Paid(form);
        }
    }

    /// The payment asked for a verification step.
    pub fn mark_verifying(&mut self) {
        if let Self::Ready(form) = self {
            *self = Self::Verifying(form.clone());
        }
    }

    /// The verification step is over (the payment answered).
    pub fn end_verification(&mut self) {
        if let Self::Verifying(form) = self {
            *self = Self::Ready(form.clone());
        }
    }
}

/// How a `sendPaymentForm` for an invoice ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentVerdict {
    Paid,
    /// The payment was refused (an error, or a failed result).
    Refused,
    /// The answer asked for a verification step outside the checkout.
    NeedsVerification,
}

/// The verdict of the last finished payment of `invoice`.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentOutcome {
    pub invoice: crate::payment_invoice::InputInvoice,
    pub verdict: PaymentVerdict,
}

pub struct PaymentsState {
    /// Slice P1: the in-flight payment request context (`getPaymentForm`,
    /// `validateOrderInfo`, `sendPaymentForm`, `getPaymentReceipt`).
    pub request: Option<PaymentRequest>,
    /// Slice P1: the checkout's `paymentForm`, from `getPaymentForm` in
    /// flight to paid ([`CheckoutForm`]).
    pub checkout: CheckoutForm,
    /// Where the checkout's order info stands (`validateOrderInfo`).
    pub order_info: OrderInfoStage,
    /// Slice P1: the chosen shipping option id (default: the first).
    pub shipping_id: Option<String>,
    /// The receipt dialog: closed, waiting for the `getPaymentReceipt` it
    /// asked, or showing the answer. Closing it forgets the request, so a
    /// late answer cannot reopen it.
    pub receipt: ReceiptStage,
    /// Slice P1: latest payment error / outcome note, shown in the
    /// checkout dialog (never a secret — order fields and credentials are
    /// never echoed here).
    pub note: Option<String>,
    /// Slice P1: the `sendPaymentForm`s in flight, each with the invoice
    /// and form it pays (tdesktop keeps one checkout process per invoice).
    /// An answer goes to its own payment: the checkout on screen takes the
    /// outcome only while it shows that invoice, and the "Media Unlocked"
    /// note names what was paid for. The Pay button shows "Processing…"
    /// and is disabled while one is in flight for the invoice shown
    /// ([`PaymentsState::checkout_sending`]).
    pub paying: Vec<PaymentInFlight>,
    /// The verdict of each invoice's last finished payment, whatever
    /// checkout was on screen, so a mini app's watcher reads how its
    /// payment ended instead of diffing looks. A new payment or a new
    /// checkout of the invoice starts it over; one entry per invoice.
    pub outcomes: Vec<PaymentOutcome>,
    /// "Media Unlocked" note after a paid-media purchase, drained into the
    /// status bar by the UI poll.
    pub unlocked_notice: Option<String>,
    /// Outcomes nobody on screen would see, drained into the status bar.
    pub notices: Vec<PaymentNotice>,
    /// Slice P1: a failed `getPaymentReceipt`, drained into the status
    /// note by `poll_live` — the checkout dialog (which renders
    /// `payment_note`) may be closed when the receipt fetch fails.
    pub receipt_error: Option<String>,
    /// Slice P1: `paymentResult.verification_url` from a non-successful
    /// `sendPaymentForm` — the UI takes it on the next poll and opens it
    /// in the OS browser (3-D Secure and similar).
    pub verification_url: Option<PaymentVerification>,
    pub marketplace_gift: Option<crate::marketplace::GiftPurchase>,
    pub gift_text_length_max: Option<usize>,
    /// TDLib `poll_country_count_max`: countries a poll may be limited to.
    pub poll_countries_max: Option<usize>,
    /// `poll_open_period_max`: the furthest a poll may close, in seconds.
    pub poll_open_period_max: Option<i64>,
    /// Slice `parity:bots-payment-recurring`: the fetched
    /// `starSubscriptions`, shown in the Subscriptions dialog, with where
    /// its page and its one edit stand ([`StarSubscriptionsList`]).
    pub star_subscriptions: StarSubscriptionsList,
    /// Slice `parity:bots-payment-recurring`: the Subscriptions dialog, and
    /// the subscription awaiting cancel confirmation in it.
    pub subscriptions: SubscriptionsBox,
    /// Premium / Stars / received-gifts hub state (`crate::premium_hub`).
    pub hub: crate::premium_hub::PremiumHub,
    /// Gift management and the send-gift dialog (`crate::gift_manage`).
    pub gifts_manage: crate::gift_manage::GiftManageState,
    /// Revenue screens and affiliate programs (`crate::earn`).
    pub earn: crate::earn::EarnState,
    /// TDLib's `is_premium` option: the account's current Premium state.
    /// `None` until the option arrives.
    pub premium_option: Option<bool>,
    /// TDLib's `is_premium_available` option; `None` until it arrives.
    pub premium_available: Option<bool>,
    /// The card number the user tapped and what `getBankCardInfo` said.
    pub bank_card: Option<BankCardLookup>,
    /// The gift code link whose box is open.
    pub gift_code: Option<GiftCodeLookup>,
    /// Giveaway lookups, payment options and prepaid launches.
    pub giveaway: GiveawayState,
    /// Star reactions waiting for their Undo window to end.
    pub paid: crate::paid_reaction::PaidReactions,
    /// Paid messages: prices, gates, trust and revenue.
    pub paid_messages: crate::paid_messages::PaidMessages,
    /// The Star packs listed under "Buy more Stars".
    pub star_packs: crate::star_packs::StarPacksState,
    /// The Gift Stars dialog.
    pub stars_gift: crate::stars_gift::StarsGiftState,
    /// Suggested posts: limits, the composer's suggest bars, the Grams
    /// balance.
    pub suggested_posts: crate::suggested_posts::SuggestedPostsState,
    /// Resale listings, gift auctions and crafting.
    pub gift_market: crate::gift_market::GiftMarketState,
}

/// One `sendPaymentForm` in flight and what it pays.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentInFlight {
    pub request_id: RequestId,
    pub invoice: crate::payment_invoice::InputInvoice,
    /// The form it was sent for, read by the paid-media note.
    pub form: Option<PaymentFormData>,
}

/// The Subscriptions dialog: closed, or open with the subscription whose
/// cancel waits for its confirmation. Closing drops the confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SubscriptionsBox {
    #[default]
    Closed,
    Open {
        cancel_confirm: Option<String>,
    },
}

impl SubscriptionsBox {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open { .. })
    }

    /// The subscription id awaiting cancel confirmation.
    pub fn cancel_confirm(&self) -> Option<&str> {
        match self {
            Self::Open { cancel_confirm } => cancel_confirm.as_deref(),
            Self::Closed => None,
        }
    }

    pub fn open(&mut self) {
        if !self.is_open() {
            *self = Self::Open {
                cancel_confirm: None,
            };
        }
    }

    pub fn close(&mut self) {
        *self = Self::Closed;
    }

    /// Ask to confirm a cancel (no effect while closed).
    pub fn ask_cancel(&mut self, id: String) {
        if let Self::Open { cancel_confirm } = self {
            *cancel_confirm = Some(id);
        }
    }

    pub fn clear_confirm(&mut self) {
        if let Self::Open { cancel_confirm } = self {
            *cancel_confirm = None;
        }
    }
}

impl PaymentsState {
    /// The checkout on screen is being paid: its invoice has a
    /// `sendPaymentForm` in flight.
    pub fn checkout_sending(&self) -> bool {
        self.request
            .as_ref()
            .is_some_and(|request| self.is_paying(&request.invoice))
    }

    /// The `sendPaymentForm` in flight for `invoice`.
    pub fn paying_request(
        &self,
        invoice: &crate::payment_invoice::InputInvoice,
    ) -> Option<RequestId> {
        self.paying
            .iter()
            .find(|paying| &paying.invoice == invoice)
            .map(|paying| paying.request_id)
    }

    /// `invoice` has a `sendPaymentForm` in flight.
    pub fn is_paying(&self, invoice: &crate::payment_invoice::InputInvoice) -> bool {
        self.paying.iter().any(|paying| &paying.invoice == invoice)
    }

    /// A new giveaway for `chat_id` has a `sendPaymentForm` in flight, in
    /// whichever checkout (or none) it was sent from.
    pub fn giveaway_paying(&self, chat_id: i64) -> bool {
        self.paying
            .iter()
            .any(|paying| paying.invoice.giveaway_chat_id() == Some(chat_id))
    }

    /// The checkout on screen shows `invoice`.
    pub fn shows(&self, invoice: &crate::payment_invoice::InputInvoice) -> bool {
        self.request
            .as_ref()
            .is_some_and(|request| &request.invoice == invoice)
    }

    /// Queue an outcome for the status bar.
    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notices.push(PaymentNotice {
            kind,
            text: text.into(),
        });
    }

    /// `invoice`'s payment ended with `verdict`.
    pub fn record_outcome(
        &mut self,
        invoice: &crate::payment_invoice::InputInvoice,
        verdict: PaymentVerdict,
    ) {
        self.forget_outcome(invoice);
        self.outcomes.push(PaymentOutcome {
            invoice: invoice.clone(),
            verdict,
        });
    }

    /// A new payment or checkout of `invoice` starts over.
    pub fn forget_outcome(&mut self, invoice: &crate::payment_invoice::InputInvoice) {
        self.outcomes.retain(|outcome| &outcome.invoice != invoice);
    }

    /// How `invoice`'s last finished payment ended.
    pub fn outcome_of(
        &self,
        invoice: &crate::payment_invoice::InputInvoice,
    ) -> Option<PaymentVerdict> {
        self.outcomes
            .iter()
            .find(|outcome| &outcome.invoice == invoice)
            .map(|outcome| outcome.verdict)
    }

    /// The payment `request_id` answered: it leaves the in-flight list.
    pub(crate) fn take_paying(&mut self, request_id: Option<RequestId>) -> Option<PaymentInFlight> {
        let index = self
            .paying
            .iter()
            .position(|paying| Some(paying.request_id) == request_id)?;
        Some(self.paying.remove(index))
    }
}

/// A tapped bank card number and the lookup's progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankCardLookup {
    pub number: String,
    pub stage: BankCardStage,
}

/// Where a bank card lookup is. The answer is applied only while the stage
/// is `Loading` for the request that answers, so a lookup of an earlier
/// number never lands in a later one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankCardStage {
    Loading {
        request: RequestId,
    },
    Ready(BankCardInfoData),
    /// No card info: the menu keeps only "Copy Card Number".
    Failed,
}

impl BankCardLookup {
    /// The looked-up card info, once it arrived.
    pub fn info(&self) -> Option<&BankCardInfoData> {
        match &self.stage {
            BankCardStage::Ready(info) => Some(info),
            _ => None,
        }
    }

    /// Whether `request` is the lookup this card waits for.
    pub(crate) fn waits_for(&self, request: Option<RequestId>) -> bool {
        request.is_some_and(|id| self.stage == BankCardStage::Loading { request: id })
    }
}

impl PaymentsState {
    pub(crate) fn new() -> Self {
        Self {
            request: None,
            checkout: CheckoutForm::Closed,
            order_info: OrderInfoStage::Unvalidated,
            shipping_id: None,
            receipt: ReceiptStage::Closed,
            note: None,
            paying: Vec::new(),
            unlocked_notice: None,
            outcomes: Vec::new(),
            notices: Vec::new(),
            receipt_error: None,
            verification_url: None,
            marketplace_gift: None,
            gift_text_length_max: None,
            poll_countries_max: None,
            poll_open_period_max: None,
            star_subscriptions: StarSubscriptionsList::default(),
            subscriptions: SubscriptionsBox::Closed,
            hub: crate::premium_hub::PremiumHub::default(),
            gifts_manage: Default::default(),
            earn: Default::default(),
            premium_option: None,
            premium_available: None,
            bank_card: None,
            gift_code: None,
            giveaway: GiveawayState::default(),
            paid: Default::default(),
            paid_messages: Default::default(),
            star_packs: Default::default(),
            stars_gift: Default::default(),
            suggested_posts: Default::default(),
            gift_market: Default::default(),
        }
    }
}
