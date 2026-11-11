//! The checkout's lifetime: closing it, and the payments it sent.
use crate::ids::RequestId;
use crate::state::{CheckoutForm, PaymentInFlight, Session};

impl Session {
    /// The checkout closed: its form and answers go. A `sendPaymentForm`
    /// still in flight keeps its own invoice and form
    /// (`PaymentsState::paying`) until TDLib answers, so a paid-media
    /// purchase still unlocks and a checkout opened meanwhile does not take
    /// its answer.
    pub fn close_checkout(&mut self) {
        let payments = &mut self.payments;
        payments.note = None;
        payments.checkout = CheckoutForm::Closed;
    }

    /// `sendPaymentForm` went out as `request_id` for the checkout on
    /// screen: remember what it pays, so its answer goes to it.
    pub(crate) fn payment_sent(&mut self, request_id: RequestId) {
        let payments = &mut self.payments;
        let Some(checkout) = payments.checkout.checkout() else {
            return;
        };
        let (invoice, form) = (checkout.invoice.invoice.clone(), checkout.form.clone());
        payments.paying.push(PaymentInFlight {
            request_id,
            invoice,
            form: Some(form),
        });
    }
}
