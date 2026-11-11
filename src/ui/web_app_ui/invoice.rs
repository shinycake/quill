//! `web_app_open_invoice`, as tdesktop answers it
//! (`WebViewInstance::botHandleInvoice` and `CheckoutProcess`): the checkout
//! for the invoice slug opens, and when it ends the app hears `invoice_closed`
//! with `paid`, `failed`, `pending` or `cancelled`.
//!
//! - `paid`: the payment went through (tdesktop closes the checkout then).
//! - `failed`: the last payment attempt was refused and the checkout was
//!   closed afterwards (`_sendFormFailed`).
//! - `pending`: the payment needs a verification step outside the checkout
//!   (3-D Secure) and the checkout was closed afterwards
//!   (`_sendFormPending`).
//! - `cancelled`: closed without paying.
//!
//! The checkout is Quill's payment dialog in the main window, so opening it
//! brings that window in front of the mini app's.

use super::*;
use quill::ids::RequestId;
use quill::payment_invoice::invoice_link_invoice;
use quill::state::PaymentVerdict;

/// The checkout a mini app opened, by the invoice slug it was opened for.
#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct InvoiceWatch {
    slug: String,
}

/// Where the watched invoice's payment stands, read from the payments
/// state: the verdict is recorded when the answer is applied, so a payment
/// refused between two looks is still seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PayStage {
    /// No payment sent since the checkout opened.
    Untouched,
    /// `sendPaymentForm` is out as this request.
    Sending(RequestId),
    Settled(PaymentVerdict),
}

/// What the payment state shows right now.
#[derive(Clone, Copy, Debug)]
pub(super) struct CheckoutView {
    pub(super) dialog_open: bool,
    pub(super) stage: PayStage,
}

impl InvoiceWatch {
    pub(super) fn new(slug: &str) -> Self {
        Self {
            slug: slug.to_string(),
        }
    }

    /// What the app hears for this look at the checkout; `Some` once it has
    /// ended.
    pub(super) fn verdict(view: CheckoutView) -> Option<&'static str> {
        match (view.dialog_open, view.stage) {
            (_, PayStage::Settled(PaymentVerdict::Paid)) => Some("paid"),
            (true, _) => None,
            (false, PayStage::Settled(PaymentVerdict::Refused)) => Some("failed"),
            // A payment still being sent when the checkout closed (Esc, or
            // the 3-D Secure page taking over) may still go through: that
            // is pending, never cancelled.
            (
                false,
                PayStage::Settled(PaymentVerdict::NeedsVerification) | PayStage::Sending(_),
            ) => Some("pending"),
            (false, PayStage::Untouched) => Some("cancelled"),
        }
    }
}

impl QuillApp {
    /// `web_app_open_invoice`: open the checkout at render time (it needs the
    /// window) and bring Quill in front of the mini app.
    pub(super) fn request_mini_app_invoice(&mut self, slug: String, cx: &mut Context<Self>) {
        self.mini_apps
            .render_actions
            .push(RenderAction::OpenInvoice { slug });
        self.raise_main_window_later(cx);
        cx.notify();
    }

    /// Render-time half: start the checkout and watch it.
    pub(super) fn open_mini_app_invoice(
        &mut self,
        slug: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let busy = self.payments.dialog.is_some()
            || self
                .mini_apps
                .window
                .as_ref()
                .is_none_or(|app| app.invoice.is_some());
        if busy || invoice_link_invoice(&slug).is_none() {
            self.with_window(|app| app.emit(BridgeReply::invoice_closed(&slug, "failed")));
            return;
        }
        self.open_invoice_link(&slug, window, cx);
        if self.payments.dialog.is_some() {
            self.with_window(|app| app.invoice = Some(InvoiceWatch::new(&slug)));
        } else {
            self.with_window(|app| app.emit(BridgeReply::invoice_closed(&slug, "failed")));
        }
    }

    /// Reports the checkout's end to the app; called by the mini app poll.
    pub(super) fn poll_invoice(&mut self, cx: &mut Context<Self>) {
        // The watched invoice's own payment: a checkout opened for another
        // invoice, or a payment of one closed before, says nothing here.
        let Some(invoice) = self
            .mini_apps
            .window
            .as_ref()
            .and_then(|window| window.invoice.as_ref())
            .and_then(|watch| invoice_link_invoice(&watch.slug))
        else {
            return;
        };
        let payments = self.session().map(|s| &s.payments);
        let stage = match payments {
            Some(p) => match p.paying_request(&invoice) {
                Some(request) => PayStage::Sending(request),
                None => p
                    .outcome_of(&invoice)
                    .map_or(PayStage::Untouched, PayStage::Settled),
            },
            None => PayStage::Untouched,
        };
        let view = CheckoutView {
            dialog_open: self.payments.dialog.is_some(),
            stage,
        };
        let Some(window) = self.mini_apps.window.as_mut() else {
            return;
        };
        let Some(watch) = window.invoice.as_mut() else {
            return;
        };
        let Some(status) = InvoiceWatch::verdict(view) else {
            return;
        };
        let slug = std::mem::take(&mut watch.slug);
        window.invoice = None;
        window.emit(BridgeReply::invoice_closed(&slug, status));
        if view.dialog_open {
            // tdesktop closes the checkout once the payment is done.
            self.close_payment_dialog(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    // No `super::*`: the kit's glob brings its own `test` attribute.
    use super::{CheckoutView, InvoiceWatch, PayStage};
    use quill::ids::RequestId;
    use quill::state::PaymentVerdict;

    fn look(dialog_open: bool, stage: PayStage) -> Option<&'static str> {
        InvoiceWatch::verdict(CheckoutView { dialog_open, stage })
    }

    #[test]
    fn closing_an_untouched_checkout_is_cancelled() {
        assert_eq!(look(true, PayStage::Untouched), None);
        assert_eq!(look(false, PayStage::Untouched), Some("cancelled"));
    }

    #[test]
    fn a_paid_form_is_paid_open_or_closed() {
        let paid = PayStage::Settled(PaymentVerdict::Paid);
        assert_eq!(look(true, paid), Some("paid"));
        assert_eq!(look(false, paid), Some("paid"));
    }

    #[test]
    fn a_refused_payment_is_failed_once_the_checkout_closes() {
        let refused = PayStage::Settled(PaymentVerdict::Refused);
        assert_eq!(look(true, refused), None, "the form stays for a retry");
        assert_eq!(look(false, refused), Some("failed"));
    }

    #[test]
    fn a_payment_sent_off_to_verify_is_pending() {
        let verifying = PayStage::Settled(PaymentVerdict::NeedsVerification);
        assert_eq!(look(true, verifying), None);
        assert_eq!(look(false, verifying), Some("pending"));
    }

    #[test]
    fn a_checkout_closed_while_the_payment_is_sent_is_pending() {
        assert_eq!(look(true, PayStage::Sending(RequestId(1))), None);
        assert_eq!(
            look(false, PayStage::Sending(RequestId(1))),
            Some("pending")
        );
    }
}

#[cfg(test)]
#[cfg(all(feature = "demo-capture", unix))]
#[path = "mini_app_flow_tests.rs"]
mod flow_tests;

/// The checkout a mini app's `web_app_open_invoice` opens, for `/verify`.
#[cfg(feature = "demo-capture")]
mod demo {
    use super::*;
    use crate::ui::screenshot_demo::{DemoSpec, register_demos};
    use quill::telegram::envelope::{PaymentFormData, PaymentFormTypeData};

    register_demos![
        DemoSpec::chats(
            "ready-mini-app-invoice",
            "screenshot demo - the checkout a mini app opened (injected form, nothing is paid)"
        )
        .setup(QuillApp::demo_mini_app_invoice),
    ];

    impl QuillApp {
        fn demo_mini_app_invoice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
            if let Some(session) = self.demo_session.as_mut() {
                // box-host: demo
                session.payments.checkout = quill::state::CheckoutForm::Ready(PaymentFormData {
                    id: 1,
                    form_type: PaymentFormTypeData::Stars { star_count: 50 },
                    seller_bot_user_id: 900,
                    product_title: "Premium skin pack".into(),
                    product_description: "Ordered from the Quillbot mini app.".into(),
                });
            }
            self.open_payment_dialog(window, cx);
        }
    }
}
