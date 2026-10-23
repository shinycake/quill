//! The checkout for invoices that are not chat messages: an invoice link
//! and a payment toward Telegram (recorded JSON, nothing live and no real
//! payment).
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::payment_card::transport::{HttpRequest, HttpResponse, Transport, TransportError};
use crate::payment_card::{CardDetails, NativeProvider, tokenize};
use crate::payment_invoice::{InputInvoice, TelegramPaymentPurpose};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::PaymentFormTypeData;
use crate::telegram::requests::input_credentials_new;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn driver_parts() -> (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (driver, recorder, sink, seq)
}

fn stars_form(extra: &Value) -> String {
    json!({
        "@type": "paymentForm", "@extra": extra, "id": 9,
        "type": {"@type": "paymentFormTypeStars", "star_count": 50},
        "seller_bot_user_id": 5,
        "product_info": {
            "@type": "productInfo", "title": "Sticker pack",
            "description": {"@type": "formattedText", "text": "", "entities": []},
            "photo": null
        }
    })
    .to_string()
}

#[test]
fn an_invoice_link_asks_for_its_form_by_name() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    let invoice = InputInvoice::Name("shop_1".into());
    driver.send_payment_form_request(invoice.clone()).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    assert_eq!(
        request["input_invoice"],
        json!({"@type": "inputInvoiceName", "name": "shop_1"})
    );
    assert_eq!(
        driver.session.payments.request.as_ref().map(|r| &r.invoice),
        Some(&invoice)
    );
}

#[test]
fn a_stars_pack_asks_for_its_form_with_the_stars_purpose() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    let invoice = InputInvoice::Telegram(TelegramPaymentPurpose::Stars {
        currency: "USD".into(),
        amount: 199,
        star_count: 100,
        chat_id: 0,
    });
    driver.send_payment_form_request(invoice).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    assert_eq!(request["input_invoice"]["@type"], "inputInvoiceTelegram");
    assert_eq!(
        request["input_invoice"]["purpose"],
        json!({
            "@type": "telegramPaymentPurposeStars",
            "currency": "USD", "amount": 199, "star_count": 100, "chat_id": 0
        })
    );
}

#[test]
fn a_message_invoice_still_needs_a_real_message() {
    let (mut driver, recorder, _sink, _seq) = driver_parts();
    let invoice = InputInvoice::Message {
        chat_id: ChatId(1),
        message_id: MessageId(2),
    };
    assert_eq!(
        driver.send_payment_form_request(invoice),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(driver.session.payments.request.is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("getPaymentForm"))
    );
}

#[test]
fn a_stars_form_is_paid_without_credentials_and_only_when_asked() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let invoice = InputInvoice::Name("shop_1".into());
    driver.send_payment_form_request(invoice.clone()).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    driver
        .ingest(copy_and_parse(&stars_form(&request["@extra"]), &seq, &sink).unwrap())
        .unwrap();
    let form = driver.session.payments.form.clone().expect("form");
    // Getting the form pays nothing.
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("sendPaymentForm"))
    );
    driver
        .submit_payment_form(&invoice, form.id, "", "", Value::Null, 0)
        .unwrap();
    let pay = sent_request(&recorder, "sendPaymentForm");
    assert_eq!(pay["payment_form_id"], 9);
    assert_eq!(pay["credentials"], Value::Null);
    assert_eq!(
        pay["input_invoice"],
        json!({"@type": "inputInvoiceName", "name": "shop_1"})
    );
}

#[test]
fn a_successful_payment_locks_the_form_and_a_new_form_unlocks_it() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let invoice = InputInvoice::Name("shop_1".into());
    driver.send_payment_form_request(invoice.clone()).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    driver
        .ingest(copy_and_parse(&stars_form(&request["@extra"]), &seq, &sink).unwrap())
        .unwrap();
    driver
        .submit_payment_form(&invoice, 9, "", "", Value::Null, 0)
        .unwrap();
    let pay = sent_request(&recorder, "sendPaymentForm");
    driver
        .ingest(
            copy_and_parse(
                &json!({
                    "@type": "paymentResult", "@extra": pay["@extra"],
                    "success": true, "verification_url": ""
                })
                .to_string(),
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.payments.form_paid);
    driver.send_payment_form_request(invoice).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    driver
        .ingest(copy_and_parse(&stars_form(&request["@extra"]), &seq, &sink).unwrap())
        .unwrap();
    assert!(!driver.session.payments.form_paid);
}

struct StripeReplay;

impl Transport for StripeReplay {
    fn post(&self, _: &HttpRequest) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status: 200,
            body: br#"{"id":"tok_visa_test","object":"token","card":{"brand":"Visa","last4":"4242"},"created":1683071586,"livemode":false}"#.to_vec(),
        })
    }
}

fn stripe_form(extra: &Value) -> String {
    json!({
        "@type": "paymentForm", "@extra": extra, "id": 11,
        "type": {
            "@type": "paymentFormTypeRegular",
            "invoice": {
                "@type": "invoice", "currency": "USD",
                "price_parts": [{"@type": "labeledPricePart", "label": "Mug", "amount": 1200}],
                "max_tip_amount": 0, "suggested_tip_amounts": [],
                "recurring_payment_terms_of_service_url": "", "terms_of_service_url": "",
                "is_test": true, "need_name": false, "need_phone_number": false,
                "need_email_address": false, "need_shipping_address": false,
                "send_phone_number_to_provider": false, "send_email_address_to_provider": false,
                "is_flexible": false
            },
            "payment_provider_user_id": 5,
            "payment_provider": {
                "@type": "paymentProviderStripe", "publishable_key": "pk_test_quill",
                "need_country": false, "need_postal_code": false, "need_cardholder_name": false
            },
            "additional_payment_options": [], "saved_order_info": null,
            "saved_credentials": [], "can_save_credentials": true, "need_password": false
        },
        "seller_bot_user_id": 5,
        "product_info": {
            "@type": "productInfo", "title": "Mug",
            "description": {"@type": "formattedText", "text": "", "entities": []},
            "photo": null
        }
    })
    .to_string()
}

#[test]
fn a_new_card_pays_with_the_provider_token_and_may_need_verification() {
    let (mut driver, recorder, sink, seq) = driver_parts();
    let invoice = InputInvoice::Name("shop_1".into());
    driver.send_payment_form_request(invoice.clone()).unwrap();
    let request = sent_request(&recorder, "getPaymentForm");
    driver
        .ingest(copy_and_parse(&stripe_form(&request["@extra"]), &seq, &sink).unwrap())
        .unwrap();
    let form = driver.session.payments.form.clone().expect("form");
    let PaymentFormTypeData::Regular(regular) = &form.form_type else {
        panic!("regular form");
    };
    let provider = NativeProvider::for_form(&regular.provider, true).expect("native");
    let card = CardDetails::new(
        "4242424242424242".into(),
        "123".into(),
        4,
        2030,
        String::new(),
        String::new(),
        String::new(),
    );
    let credentials = tokenize(&provider, &card, &StripeReplay).unwrap();
    driver
        .submit_payment_form(
            &invoice,
            form.id,
            "",
            "",
            input_credentials_new(credentials.data(), true),
            0,
        )
        .unwrap();
    let pay = sent_request(&recorder, "sendPaymentForm");
    assert_eq!(
        pay["credentials"],
        json!({
            "@type": "inputCredentialsNew",
            "data": r#"{"id":"tok_visa_test","type":"card"}"#,
            "allow_save": true
        })
    );
    // The card itself never reaches TDLib.
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("4242424242424242"))
    );
    let result = json!({
        "@type": "paymentResult", "@extra": pay["@extra"],
        "success": false, "verification_url": "https://hooks.stripe.com/3d_secure/x"
    });
    driver
        .ingest(copy_and_parse(&result.to_string(), &seq, &sink).unwrap())
        .unwrap();
    assert_eq!(
        driver.session.payments.verification_url.as_deref(),
        Some("https://hooks.stripe.com/3d_secure/x")
    );
    assert!(!driver.session.payments.form_paid);
}
