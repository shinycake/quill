//! The card form screenshot demo, and the recorded Stripe answer offline
//! sessions tokenize against (a demo never reaches the network).
//! The demo opens on the payment method list; click "New Card..." to reach
//! the form. `QUILL_DEMO_CARD=password` is an account without Two-Step
//! Verification, so saving the card asks to turn it on.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::{DemoSpec, register_demos};
use gpui_kit::*;
use quill::payment_card::transport::{HttpRequest, HttpResponse, Transport, TransportError};
use quill::telegram::envelope::{
    InvoiceForm, LabeledPrice, OrderInfoData, PaymentFormData, PaymentFormRegular,
    PaymentFormTypeData, PaymentProviderKind, SavedCredential,
};

register_demos![DemoSpec::chat_list("ready-payment-card").setup(QuillApp::demo_payment_card)];

/// Stripe's published decline test number.
const DECLINED_NUMBER: &[u8] = b"4000000000000002";
const TOKEN: &str = r#"{"id":"tok_demo","object":"token","card":{"brand":"Visa","last4":"4242"},"created":1700000000,"livemode":false,"type":"card"}"#;
const DECLINED: &str =
    r#"{"error":{"code":"card_declined","message":"Your card was declined.","type":"card_error"}}"#;

/// Answers like Stripe's test mode: the decline test number is declined,
/// any other card gets a Visa token.
pub(super) struct RecordedStripe;

impl Transport for RecordedStripe {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let declined = request
            .body
            .windows(DECLINED_NUMBER.len())
            .any(|window| window == DECLINED_NUMBER);
        let (status, body) = if declined {
            (402, DECLINED)
        } else {
            (200, TOKEN)
        };
        Ok(HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        })
    }
}

/// A test-mode Stripe checkout that asks for every card field, with one
/// saved card.
pub(super) fn stripe_checkout(can_save_credentials: bool, need_password: bool) -> PaymentFormData {
    PaymentFormData {
        id: 1,
        form_type: PaymentFormTypeData::Regular(Box::new(PaymentFormRegular {
            invoice: InvoiceForm {
                currency: "USD".into(),
                price_parts: vec![LabeledPrice {
                    label: "Mug".into(),
                    amount: 1200,
                }],
                max_tip_amount: 0,
                suggested_tip_amounts: Vec::new(),
                recurring_terms_url: String::new(),
                terms_url: String::new(),
                is_test: true,
                need_name: false,
                need_phone_number: false,
                need_email_address: false,
                need_shipping_address: false,
                is_flexible: false,
            },
            provider: PaymentProviderKind::Stripe {
                publishable_key: "pk_test_demo".into(),
                need_country: true,
                need_postal_code: true,
                need_cardholder_name: true,
            },
            additional_options: Vec::new(),
            saved_order_info: OrderInfoData::default(),
            saved_credentials: vec![SavedCredential {
                id: "saved-1".into(),
                title: "mastercard *4444".into(),
            }],
            can_save_credentials,
            need_password,
        })),
        seller_bot_user_id: 0,
        product_title: "Ceramic mug".into(),
        product_description: "Dishwasher safe.".into(),
    }
}

impl QuillApp {
    fn demo_payment_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = std::env::var("QUILL_DEMO_CARD").unwrap_or_default();
        let password = mode == "password";
        if let Some(session) = self.demo_session.as_mut() {
            session.payments.form = Some(stripe_checkout(!password, password));
        }
        self.open_payment_dialog(window, cx);
    }
}
