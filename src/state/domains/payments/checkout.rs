//! The checkout's lifetime: closing it, and the payments it sent.
use crate::ids::RequestId;
use crate::payment_invoice::InputInvoice;
use crate::state::{CheckoutForm, PaymentInFlight, Session};

impl Session {
    /// The checkout closed: its form and answers go. A `sendPaymentForm`
    /// still in flight keeps its own invoice and form
    /// (`PaymentsState::paying`) until TDLib answers, so a paid-media
    /// purchase still unlocks and a checkout opened meanwhile does not take
    /// its answer.
    pub fn close_checkout(&mut self) {
        self.payments.checkout = CheckoutForm::Closed;
    }

    /// `sendPaymentForm` went out as `request_id` for the checkout on
    /// screen: remember what it pays, so its answer goes to it. The form
    /// is the one on screen, only while the checkout shows that invoice.
    pub(crate) fn payment_sent(&mut self, request_id: RequestId, invoice: &InputInvoice) {
        let payments = &mut self.payments;
        let form = payments
            .shows(invoice)
            .then(|| payments.checkout.form().cloned())
            .flatten();
        payments.paying.push(PaymentInFlight {
            request_id,
            invoice: invoice.clone(),
            form,
        });
    }
}
