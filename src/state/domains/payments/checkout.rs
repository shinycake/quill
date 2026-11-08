//! Closing the checkout: what goes at once and what waits for a payment
//! still in flight.
use crate::state::Session;

impl Session {
    /// The checkout closed: its form answers go. A `sendPaymentForm` still
    /// in flight keeps the invoice and form it pays until TDLib answers,
    /// so a paid-media purchase still unlocks
    /// (`note_paid_media_unlocked`); `settle_closed_checkout` drops them
    /// then.
    pub fn close_checkout(&mut self) {
        let payments = &mut self.payments;
        payments.form_loading = false;
        payments.note = None;
        payments.validated = None;
        payments.shipping_id = None;
        if payments.sending {
            payments.checkout_closed = true;
        } else {
            payments.form = None;
            payments.request = None;
            payments.form_paid = false;
        }
    }

    /// The payment a closed checkout left in flight answered: what it kept
    /// for the answer goes.
    pub(crate) fn settle_closed_checkout(&mut self) {
        let payments = &mut self.payments;
        if std::mem::take(&mut payments.checkout_closed) {
            payments.form = None;
            payments.request = None;
            payments.form_paid = false;
            payments.note = None;
        }
    }
}
