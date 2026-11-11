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

pub struct PaymentsState {
    /// The checkout, one value: its invoice, form, order info, shipping
    /// choice and note live in the stage that owns them ([`CheckoutForm`]),
    /// so closing it takes them all.
    pub checkout: CheckoutForm,
    /// The receipt dialog: closed, waiting for the `getPaymentReceipt` it
    /// asked, or showing the answer. Closing it forgets the request, so a
    /// late answer cannot reopen it.
    pub receipt: ReceiptStage,
    /// Slice P1: the `sendPaymentForm`s in flight, each with the invoice
    /// and form it pays (tdesktop keeps one checkout process per invoice).
    /// An answer goes to its own payment: the checkout on screen takes the
    /// outcome only while it shows that invoice, and the "Media Unlocked"
    /// note names what was paid for. The Pay button shows "Processing…"
    /// and is disabled while one is in flight for the invoice shown
    /// ([`PaymentsState::checkout_sending`]).
    pub paying: Vec<PaymentInFlight>,
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
        self.checkout
            .invoice()
            .is_some_and(|request| self.is_paying(&request.invoice))
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
        self.checkout
            .invoice()
            .is_some_and(|request| &request.invoice == invoice)
    }

    /// Queue an outcome for the status bar.
    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notices.push(PaymentNotice {
            kind,
            text: text.into(),
        });
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
            checkout: CheckoutForm::Closed,
            receipt: ReceiptStage::Closed,
            paying: Vec::new(),
            unlocked_notice: None,
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

impl PaymentsState {
    /// The one way a gift's panel opens or closes from the UI. What was open
    /// for another gift goes with it: the convert confirmation (`GiftView`),
    /// and an Upgrade or resale price form that names a different gift (it
    /// would hide this gift's buttons behind it).
    pub fn select_gift(&mut self, id: Option<String>) {
        self.gifts_manage.flow.take_if(|flow| match flow {
            crate::gift_manage::GiftFlow::Upgrade { id: open, .. }
            | crate::gift_manage::GiftFlow::Resale { id: open } => {
                id.as_deref() != Some(open.as_str())
            }
            _ => false,
        });
        self.hub.gift_view.select(id);
    }
}

#[cfg(test)]
mod select_gift_tests {
    use super::*;
    use crate::gift_manage::GiftFlow;

    fn state_with(flow: GiftFlow, selected: &str) -> PaymentsState {
        let mut payments = PaymentsState::new();
        payments.hub.gift_view.select(Some(selected.into()));
        payments
            .hub
            .gift_view
            .confirm_convert(Some(selected.into()));
        payments.gifts_manage.flow = Some(flow);
        payments
    }

    #[test]
    fn selecting_another_gift_drops_the_flow_and_confirm_of_the_first() {
        let mut payments = state_with(
            GiftFlow::Upgrade {
                id: "a".into(),
                keep_details: false,
            },
            "a",
        );
        payments.select_gift(Some("b".into()));
        assert_eq!(payments.hub.gift_view.selected(), Some("b"));
        assert!(payments.gifts_manage.flow.is_none());
        assert_eq!(payments.hub.gift_view.confirming(), None);
    }

    #[test]
    fn selecting_the_same_gift_keeps_its_flow_and_other_flows_stay() {
        let mut payments = state_with(GiftFlow::Resale { id: "a".into() }, "a");
        payments.select_gift(Some("a".into()));
        assert!(payments.gifts_manage.flow.is_some());
        payments.gifts_manage.flow = Some(GiftFlow::NewCollection);
        payments.select_gift(None);
        assert_eq!(payments.gifts_manage.flow, Some(GiftFlow::NewCollection));
    }
}
