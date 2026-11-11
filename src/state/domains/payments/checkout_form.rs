//! The checkout as one value: the stage of its form, with the invoice,
//! order info, shipping choice and note that belong to that stage.
use crate::state::*;
use crate::telegram::envelope::OrderInfoData;

/// What a fetched checkout holds besides its form.
#[derive(Debug, Clone, PartialEq)]
pub struct Checkout {
    pub form: PaymentFormData,
    /// What is being paid; the same value goes into every request.
    pub invoice: PaymentRequest,
    /// Where the order info stands (`validateOrderInfo`).
    pub order_info: OrderInfoStage,
    /// The chosen shipping option id (default: the first).
    pub shipping_id: Option<String>,
    /// The latest payment error or outcome, shown in the checkout dialog
    /// (never a secret: order fields and credentials are never echoed).
    pub note: Option<String>,
}

impl Checkout {
    pub fn new(form: PaymentFormData, invoice: PaymentRequest) -> Self {
        Self {
            form,
            invoice,
            order_info: OrderInfoStage::Unvalidated,
            shipping_id: None,
            note: None,
        }
    }
}

/// The checkout, one stage at a time.
///
/// `Loading` leaves on the answer of its own `getPaymentForm` (`Ready`; an
/// answer to an earlier checkout's fetch is not this one's) or its error
/// (`Failed`, with the line the dialog shows). `Ready` leaves on the
/// `sendPaymentForm` answer: `Paid` (Pay stays off, so one form is never
/// sent twice), `Verifying` (the answer asked for a verification step;
/// nothing drains it, a mini app's checkout reads it to report `pending`)
/// or back to `Ready` when it failed. Any stage leaves with the checkout
/// closing, which takes the invoice, order info, shipping choice and note
/// with it.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CheckoutForm {
    #[default]
    Closed,
    /// `getPaymentForm` is out as `request` for `invoice` (the dialog shows
    /// a spinner).
    Loading {
        request: RequestId,
        invoice: PaymentRequest,
    },
    /// The form could not be fetched: the dialog shows this line.
    Failed(String),
    Ready(Checkout),
    Verifying(Checkout),
    Paid(Checkout),
}

impl CheckoutForm {
    /// A checkout in the `Ready` stage (for the paths that open one
    /// without a fetch, and for tests).
    pub fn ready(form: PaymentFormData, invoice: PaymentRequest) -> Self {
        Self::Ready(Checkout::new(form, invoice))
    }

    /// The fetched checkout, in any stage that has one.
    pub fn open(&self) -> Option<&Checkout> {
        match self {
            Self::Ready(checkout) | Self::Verifying(checkout) | Self::Paid(checkout) => {
                Some(checkout)
            }
            Self::Closed | Self::Loading { .. } | Self::Failed(_) => None,
        }
    }

    fn open_mut(&mut self) -> Option<&mut Checkout> {
        match self {
            Self::Ready(checkout) | Self::Verifying(checkout) | Self::Paid(checkout) => {
                Some(checkout)
            }
            Self::Closed | Self::Loading { .. } | Self::Failed(_) => None,
        }
    }

    /// The fetched form, in any stage that has one.
    pub fn form(&self) -> Option<&PaymentFormData> {
        self.open().map(|checkout| &checkout.form)
    }

    /// What the checkout on screen pays, while it loads or has its form.
    pub fn invoice(&self) -> Option<&PaymentRequest> {
        match self {
            Self::Loading { invoice, .. } => Some(invoice),
            Self::Ready(checkout) | Self::Verifying(checkout) | Self::Paid(checkout) => {
                Some(&checkout.invoice)
            }
            Self::Closed | Self::Failed(_) => None,
        }
    }

    pub fn order_info(&self) -> Option<&OrderInfoStage> {
        self.open().map(|checkout| &checkout.order_info)
    }

    /// The validated order info, once there is one.
    pub fn validated_order(&self) -> Option<&ValidatedOrderInfoData> {
        self.order_info().and_then(OrderInfoStage::validated)
    }

    pub fn shipping_id(&self) -> Option<&str> {
        self.open()
            .and_then(|checkout| checkout.shipping_id.as_deref())
    }

    /// The line the dialog shows: a failed fetch's, or the open checkout's
    /// latest note.
    pub fn note(&self) -> Option<&str> {
        match self {
            Self::Failed(line) => Some(line),
            _ => self.open().and_then(|checkout| checkout.note.as_deref()),
        }
    }

    /// Sets (or, with `None`, clears) the note. A note with no checkout to
    /// show it in is a failed one: the dialog still has something to say.
    pub fn set_note(&mut self, note: Option<String>) {
        match (&mut *self, note) {
            (Self::Closed, Some(line)) => *self = Self::Failed(line),
            (Self::Failed(_), None) => *self = Self::Closed,
            (Self::Failed(slot), Some(line)) => *slot = line,
            (_, note) => {
                if let Some(checkout) = self.open_mut() {
                    checkout.note = note;
                }
            }
        }
    }

    /// The checkout's order info starts validating `order` as `request`:
    /// the earlier order id and shipping choice are for other fields.
    pub fn begin_validating(&mut self, request: RequestId, order: OrderInfoData) {
        if let Some(checkout) = self.open_mut() {
            checkout.shipping_id = None;
            checkout.order_info = OrderInfoStage::Validating { request, order };
        }
    }

    /// The shipping option the user picked.
    pub fn choose_shipping(&mut self, id: String) {
        if let Some(checkout) = self.open_mut() {
            checkout.shipping_id = Some(id);
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
        matches!((self, request), (Self::Loading { request: asked, .. }, Some(id)) if *asked == id)
    }

    /// The form was paid.
    pub fn mark_paid(&mut self) {
        if let Self::Ready(checkout) | Self::Verifying(checkout) = self {
            *self = Self::Paid(checkout.clone());
        }
    }

    /// The payment asked for a verification step.
    pub fn mark_verifying(&mut self) {
        if let Self::Ready(checkout) = self {
            *self = Self::Verifying(checkout.clone());
        }
    }

    /// The verification step is over (the payment answered).
    pub fn end_verification(&mut self) {
        if let Self::Verifying(checkout) = self {
            *self = Self::Ready(checkout.clone());
        }
    }
}
