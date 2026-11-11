//! The checkout's lifetime: closing it, and the payments it sent.
use crate::ids::RequestId;
use crate::state::{OrderInfoStage, PaymentInFlight, PaymentsState, Session};
use crate::telegram::envelope::PaymentFormData;

/// The checkout's form, one value. `LoadingForm` leaves when its own
/// `getPaymentForm` answers (`Open`) or fails (`Idle`, with a note), and
/// with a closed checkout (`Idle`); an answer to an earlier one (another
/// invoice, opened before) is not this checkout's form. `Paid` keeps Pay
/// off, so one form is never sent twice; `Verifying` is a `sendPaymentForm`
/// answer that asked for a verification step (3-D Secure): nothing drains
/// it, a mini app's checkout reads it to report `pending`.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Checkout {
    /// No checkout, or it was closed.
    #[default]
    Idle,
    /// `getPaymentForm` is in flight (the dialog shows a spinner);
    /// `request` is `None` until the send went out.
    LoadingForm { request: Option<RequestId> },
    /// The form is shown, waiting for Pay.
    Open(PaymentFormData),
    /// The payment asked for a verification step.
    Verifying(PaymentFormData),
    /// The payment succeeded.
    Paid(PaymentFormData),
}

impl Checkout {
    /// The form shown, in whichever stage holds one.
    pub fn form(&self) -> Option<&PaymentFormData> {
        match self {
            Self::Idle | Self::LoadingForm { .. } => None,
            Self::Open(form) | Self::Verifying(form) | Self::Paid(form) => Some(form),
        }
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::LoadingForm { .. })
    }

    pub fn is_paid(&self) -> bool {
        matches!(self, Self::Paid(_))
    }

    pub fn verification_started(&self) -> bool {
        matches!(self, Self::Verifying(_))
    }

    /// The `getPaymentForm` this checkout waits for.
    pub fn waiting_for(&self) -> Option<RequestId> {
        match self {
            Self::LoadingForm { request } => *request,
            _ => None,
        }
    }

    /// The form was paid: Pay stays off.
    fn paid(&mut self) {
        *self = match std::mem::take(self) {
            Self::Open(form) | Self::Verifying(form) => Self::Paid(form),
            other => other,
        };
    }

    /// The verification step started for the shown form.
    fn verifying(&mut self) {
        *self = match std::mem::take(self) {
            Self::Open(form) => Self::Verifying(form),
            other => other,
        };
    }

    /// A later answer for the shown form ends its verification step.
    fn verification_over(&mut self) {
        *self = match std::mem::take(self) {
            Self::Verifying(form) => Self::Open(form),
            other => other,
        };
    }
}

impl PaymentsState {
    /// The form shown, if any.
    pub fn form(&self) -> Option<&PaymentFormData> {
        self.checkout.form()
    }

    /// A form is on screen (loaded elsewhere, or a demo's).
    pub fn set_form(&mut self, form: PaymentFormData) {
        self.checkout = Checkout::Open(form);
    }

    /// The form was paid.
    pub(crate) fn mark_paid(&mut self) {
        self.checkout.paid();
    }

    pub(crate) fn mark_verifying(&mut self) {
        self.checkout.verifying();
    }

    pub(crate) fn end_verification(&mut self) {
        self.checkout.verification_over();
    }
}

impl Session {
    /// The checkout closed: its form and answers go. A `sendPaymentForm`
    /// still in flight keeps its own invoice and form
    /// (`PaymentsState::paying`) until TDLib answers, so a paid-media
    /// purchase still unlocks and a checkout opened meanwhile does not take
    /// its answer.
    pub fn close_checkout(&mut self) {
        let payments = &mut self.payments;
        payments.note = None;
        payments.order_info = OrderInfoStage::Unvalidated;
        payments.shipping_id = None;
        payments.checkout = Checkout::Idle;
        payments.request = None;
    }

    /// `sendPaymentForm` went out as `request_id` for the checkout on
    /// screen: remember what it pays, so its answer goes to it.
    pub(crate) fn payment_sent(&mut self, request_id: RequestId) {
        let payments = &mut self.payments;
        let Some(request) = payments.request.as_ref() else {
            return;
        };
        payments.paying.push(PaymentInFlight {
            request_id,
            invoice: request.invoice.clone(),
            form: payments.form().cloned(),
        });
    }
}
