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
use quill::payment_invoice::invoice_link_invoice;

/// How one watched checkout has gone so far.
#[derive(Default, Debug, PartialEq, Eq)]
pub(super) struct InvoiceWatch {
    slug: String,
    /// A payment was being sent on the last look.
    sending: bool,
    failed: bool,
    pending: bool,
}

/// What the payment state shows right now.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct CheckoutView {
    pub(super) dialog_open: bool,
    pub(super) paid: bool,
    pub(super) sending: bool,
    /// An error line is on the checkout.
    pub(super) note: bool,
    /// The last payment attempt was sent off to verify.
    pub(super) verification: bool,
}

impl InvoiceWatch {
    pub(super) fn new(slug: &str) -> Self {
        Self {
            slug: slug.to_string(),
            ..Self::default()
        }
    }

    /// Folds in one look at the checkout; `Some` once it has ended.
    pub(super) fn observe(&mut self, view: CheckoutView) -> Option<&'static str> {
        if view.paid {
            return Some("paid");
        }
        if !view.dialog_open {
            return Some(if self.failed {
                "failed"
            } else if self.pending {
                "pending"
            } else {
                "cancelled"
            });
        }
        if view.sending {
            self.sending = true;
            self.failed = false;
            self.pending = false;
        } else if self.sending {
            // The attempt ended without paying: an error on the form means it
            // failed, a verification step means it is pending, and anything
            // else leaves it cancelled.
            self.sending = false;
            self.failed = view.note;
            self.pending = view.verification && !view.note;
        }
        None
    }
}

impl QuillApp {
    /// `web_app_open_invoice`: open the checkout at render time (it needs the
    /// window) and bring Quill in front of the mini app.
    pub(super) fn request_mini_app_invoice(&mut self, slug: String, cx: &mut Context<Self>) {
        self.mini_apps.render_action = Some(RenderAction::OpenInvoice { slug });
        cx.activate(true);
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
        let view = CheckoutView {
            dialog_open: self.payments.dialog.is_some(),
            paid: self.session().is_some_and(|s| s.payments.form_paid),
            sending: self.session().is_some_and(|s| s.payments.sending),
            note: self.session().is_some_and(|s| s.payments.note.is_some()),
            verification: self
                .session()
                .is_some_and(|s| s.payments.verification_started),
        };
        let Some(window) = self.mini_apps.window.as_mut() else {
            return;
        };
        let Some(watch) = window.invoice.as_mut() else {
            return;
        };
        let Some(status) = watch.observe(view) else {
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
    use super::{CheckoutView, InvoiceWatch};

    fn open() -> CheckoutView {
        CheckoutView {
            dialog_open: true,
            ..CheckoutView::default()
        }
    }

    #[test]
    fn closing_an_untouched_checkout_is_cancelled() {
        let mut watch = InvoiceWatch::new("s");
        assert_eq!(watch.observe(open()), None);
        assert_eq!(watch.observe(CheckoutView::default()), Some("cancelled"));
    }

    #[test]
    fn a_paid_form_is_paid() {
        let mut watch = InvoiceWatch::new("s");
        let paid = CheckoutView {
            paid: true,
            ..open()
        };
        assert_eq!(watch.observe(paid), Some("paid"));
    }

    #[test]
    fn a_refused_payment_closed_afterwards_is_failed() {
        let mut watch = InvoiceWatch::new("s");
        let sending = CheckoutView {
            sending: true,
            ..open()
        };
        let refused = CheckoutView {
            note: true,
            ..open()
        };
        assert_eq!(watch.observe(sending), None);
        assert_eq!(watch.observe(refused), None);
        assert_eq!(watch.observe(CheckoutView::default()), Some("failed"));
    }

    #[test]
    fn a_payment_sent_off_to_verify_is_pending() {
        let mut watch = InvoiceWatch::new("s");
        let sending = CheckoutView {
            sending: true,
            ..open()
        };
        let verifying = CheckoutView {
            verification: true,
            ..open()
        };
        assert_eq!(watch.observe(sending), None);
        assert_eq!(watch.observe(verifying), None);
        assert_eq!(watch.observe(CheckoutView::default()), Some("pending"));
    }

    #[test]
    fn a_send_that_ends_quietly_is_cancelled() {
        let mut watch = InvoiceWatch::new("s");
        let sending = CheckoutView {
            sending: true,
            ..open()
        };
        assert_eq!(watch.observe(sending), None);
        assert_eq!(watch.observe(open()), None);
        assert_eq!(watch.observe(CheckoutView::default()), Some("cancelled"));
    }

    #[test]
    fn a_new_attempt_clears_the_old_verdict() {
        let mut watch = InvoiceWatch::new("s");
        let sending = CheckoutView {
            sending: true,
            ..open()
        };
        let refused = CheckoutView {
            note: true,
            ..open()
        };
        watch.observe(sending);
        watch.observe(refused);
        let verifying = CheckoutView {
            verification: true,
            ..open()
        };
        watch.observe(sending);
        watch.observe(verifying);
        assert_eq!(watch.observe(CheckoutView::default()), Some("pending"));
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
                session.payments.form = Some(PaymentFormData {
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
