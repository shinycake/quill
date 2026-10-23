//! Tokenization against recorded provider answers. No test touches the
//! network: the transport records the request and replays a response.

use super::transport::{HttpRequest, HttpResponse, Transport, TransportError};
use super::validate::YearMonth;
use super::*;
use std::sync::Mutex;

/// Stripe's API reference example token for the 4242 test card.
const STRIPE_TOKEN: &str = r#"{"id":"tok_1N3T00LkdIwHu7ixt44h1F8k","object":"token","card":{"id":"card_1N3T00LkdIwHu7ixRdxpVI1Q","object":"card","address_city":null,"address_country":null,"address_line1":null,"address_zip":null,"brand":"Visa","country":"US","cvc_check":"unchecked","exp_month":8,"exp_year":2027,"fingerprint":"mToisGZ01V71BCos","funding":"credit","last4":"4242","name":null},"client_ip":null,"created":1683071586,"livemode":false,"type":"card","used":false}"#;
const STRIPE_DECLINED: &str = r#"{"error":{"code":"card_declined","decline_code":"generic_decline","doc_url":"https://stripe.com/docs/error-codes/card-declined","message":"Your card was declined.","type":"card_error"}}"#;
const STRIPE_BAD_KEY: &str = r#"{"error":{"message":"Invalid API Key provided: pk_test_****","type":"invalid_request_error"}}"#;
const SMART_GLOCAL_TOKEN: &str = r#"{"status":"ok","data":{"token":"sg_tok_123","info":{"card_network":"visa","card_type":"VISA","masked_card_number":"424242******4242"}}}"#;
const SMART_GLOCAL_ERROR: &str =
    r#"{"status":"error","error":{"code":"invalid_card","description":"Card number is invalid"}}"#;

/// A request as the transport saw it: URL, headers, body.
type Seen = (String, Vec<(&'static str, String)>, Vec<u8>);

struct Recorded {
    answer: Result<HttpResponse, TransportError>,
    seen: Mutex<Vec<Seen>>,
}

impl Recorded {
    fn answering(status: u16, body: &str) -> Self {
        Self::with(Ok(HttpResponse {
            status,
            body: body.as_bytes().to_vec(),
        }))
    }

    fn with(answer: Result<HttpResponse, TransportError>) -> Self {
        Self {
            answer,
            seen: Mutex::new(Vec::new()),
        }
    }

    fn request(&self) -> (String, Vec<(&'static str, String)>, String) {
        let seen = self.seen.lock().unwrap();
        let (url, headers, body) = seen.first().cloned().unwrap();
        (url, headers, String::from_utf8(body).unwrap())
    }
}

impl Transport for Recorded {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.seen.lock().unwrap().push((
            request.url.clone(),
            request.headers.clone(),
            request.body.to_vec(),
        ));
        self.answer.clone()
    }
}

fn stripe(need: bool) -> NativeProvider {
    NativeProvider::Stripe {
        publishable_key: "pk_test_quill".into(),
        need_country: need,
        need_zip: need,
        need_cardholder_name: need,
    }
}

fn smart_glocal(url: &str, is_test: bool) -> NativeProvider {
    NativeProvider::SmartGlocal {
        public_token: "pub_quill".into(),
        tokenize_url: url.into(),
        is_test,
    }
}

fn test_card() -> CardDetails {
    CardDetails::new(
        "4242424242424242".into(),
        "123".into(),
        4,
        2030,
        "JANE DOE".into(),
        "US".into(),
        "10001".into(),
    )
}

#[test]
fn stripe_form_matches_tdesktops_encoder() {
    let transport = Recorded::answering(200, STRIPE_TOKEN);
    let credentials = tokenize(&stripe(true), &test_card(), &transport).unwrap();
    let (url, headers, body) = transport.request();
    assert_eq!(url, "https://api.stripe.com/v1/tokens");
    assert_eq!(
        body,
        "card%5Baddress_country%5D=US&card%5Baddress_zip%5D=10001&card%5Bcvc%5D=123\
         &card%5Bexp_month%5D=4&card%5Bexp_year%5D=2030&card%5Bname%5D=JANE%20DOE\
         &card%5Bnumber%5D=4242424242424242"
    );
    assert!(headers.contains(&("Authorization", "Bearer pk_test_quill".into())));
    assert!(headers.contains(&("Stripe-Version", "2015-10-12".into())));
    assert!(headers.contains(&(
        "X-Stripe-User-Agent",
        r#"{"bindings_version":"9.1.0","lang":"objective-c"}"#.into()
    )));
    assert_eq!(credentials.title(), "visa *4242");
    assert_eq!(
        credentials.data(),
        r#"{"id":"tok_1N3T00LkdIwHu7ixt44h1F8k","type":"card"}"#
    );
}

#[test]
fn stripe_leaves_out_fields_it_did_not_ask_for() {
    let transport = Recorded::answering(200, STRIPE_TOKEN);
    let card = CardDetails::new(
        "4242424242424242".into(),
        "123".into(),
        12,
        2031,
        String::new(),
        String::new(),
        String::new(),
    );
    tokenize(&stripe(false), &card, &transport).unwrap();
    assert_eq!(
        transport.request().2,
        "card%5Bcvc%5D=123&card%5Bexp_month%5D=12&card%5Bexp_year%5D=2031\
         &card%5Bnumber%5D=4242424242424242"
    );
}

#[test]
fn stripe_errors_map_to_fields_and_notices() {
    let notice = |status: u16, body: &str| {
        let transport = Recorded::answering(status, body);
        tokenize(&stripe(false), &test_card(), &transport)
            .unwrap_err()
            .notice()
    };
    assert_eq!(notice(402, STRIPE_DECLINED), CardErrorNotice::Declined);
    let code = |code: &str| {
        format!(r#"{{"error":{{"code":"{code}","message":"No.","type":"card_error"}}}}"#)
    };
    assert_eq!(
        notice(402, &code("incorrect_number")),
        CardErrorNotice::Field(CardField::Number)
    );
    assert_eq!(
        notice(402, &code("invalid_cvc")),
        CardErrorNotice::Field(CardField::Cvc)
    );
    assert_eq!(
        notice(402, &code("expired_card")),
        CardErrorNotice::Field(CardField::Expiry)
    );
    assert_eq!(
        notice(402, &code("processing_error")),
        CardErrorNotice::ProcessingError
    );
    assert_eq!(
        notice(402, &code("insufficient_funds")),
        CardErrorNotice::Stripe("insufficient_funds".into())
    );
    assert_eq!(
        notice(401, STRIPE_BAD_KEY),
        CardErrorNotice::Stripe("GenericError".into())
    );
    assert_eq!(
        notice(404, ""),
        CardErrorNotice::Stripe("RequestError203".into())
    );
    assert_eq!(
        notice(200, "not json"),
        CardErrorNotice::Stripe("InvalidJson".into())
    );
    assert_eq!(
        notice(200, "[1]"),
        CardErrorNotice::Stripe("InvalidJsonRoot".into())
    );
    assert_eq!(
        notice(200, r#"{"id":"tok_1"}"#),
        CardErrorNotice::Stripe("InvalidTokenJson".into())
    );
    let offline = Recorded::with(Err(TransportError::HostNotFound));
    assert_eq!(
        tokenize(&stripe(false), &test_card(), &offline)
            .unwrap_err()
            .notice(),
        CardErrorNotice::Stripe("RequestError3".into())
    );
}

#[test]
fn smart_glocal_sends_json_with_its_public_token() {
    let url = "https://tgb.smart-glocal.com/cds/v1/tokenize/card";
    let transport = Recorded::answering(200, SMART_GLOCAL_TOKEN);
    let credentials = tokenize(&smart_glocal(url, false), &test_card(), &transport).unwrap();
    let (sent_url, headers, body) = transport.request();
    assert_eq!(sent_url, url);
    assert_eq!(
        body,
        r#"{"card":{"expiration_month":"04","expiration_year":"30","number":"4242424242424242","security_code":"123"}}"#
    );
    assert!(headers.contains(&("X-PUBLIC-TOKEN", "pub_quill".into())));
    assert!(headers.contains(&("Content-Type", "application/json".into())));
    assert_eq!(credentials.title(), "visa *4242");
    assert_eq!(
        credentials.data(),
        r#"{"token":"sg_tok_123","type":"card"}"#
    );

    let failing = Recorded::answering(400, SMART_GLOCAL_ERROR);
    assert_eq!(
        tokenize(&smart_glocal(url, false), &test_card(), &failing)
            .unwrap_err()
            .notice(),
        CardErrorNotice::SmartGlocal("invalid_card".into())
    );
}

#[test]
fn smart_glocal_only_trusts_its_own_tokenize_url() {
    use super::smartglocal::tokenize_url;
    let good = "https://tgb.smart-glocal.com/cds/v1/tokenize/card";
    assert_eq!(tokenize_url(good, false), good);
    for bad in [
        "http://tgb.smart-glocal.com/cds/v1/tokenize/card",
        "https://evil.example/cds/v1/tokenize/card",
        "https://smart-glocal.com.evil.example/cds/v1/tokenize/card",
        "https://tgb.smart-glocal.com:8443/cds/v1/tokenize/card",
        "https://user@tgb.smart-glocal.com/cds/v1/tokenize/card",
        "https://tgb.smart-glocal.com/cds/v1/tokenize/card?x=1",
        "https://.smart-glocal.com/cds/v1/tokenize/card",
        "",
    ] {
        assert_eq!(
            tokenize_url(bad, false),
            "https://tgb.smart-glocal.com/cds/v1/tokenize/card",
            "{bad}"
        );
    }
    assert_eq!(
        tokenize_url("", true),
        "https://tgb-playground.smart-glocal.com/cds/v1/tokenize/card"
    );
}

#[test]
fn local_checks_report_the_first_bad_field() {
    let today = YearMonth {
        year: 2026,
        month: 10,
    };
    assert_eq!(local_error(&test_card(), &stripe(true), today), None);
    let card = |number: &str, month: u32, cvc: &str, name: &str| {
        CardDetails::new(
            number.into(),
            cvc.into(),
            month,
            2030,
            name.into(),
            "US".into(),
            "10001".into(),
        )
    };
    assert_eq!(
        local_error(
            &card("4242424242424241", 4, "123", "J"),
            &stripe(true),
            today
        ),
        Some(CardField::Number)
    );
    assert_eq!(
        local_error(
            &card("4242424242424242", 13, "1", "J"),
            &stripe(true),
            today
        ),
        Some(CardField::Expiry)
    );
    assert_eq!(
        local_error(&card("4242424242424242", 4, "1", "J"), &stripe(true), today),
        Some(CardField::Cvc)
    );
    assert_eq!(
        local_error(
            &card("4242424242424242", 4, "123", ""),
            &stripe(true),
            today
        ),
        Some(CardField::Name)
    );
    // The name is not asked for, so its absence is fine.
    assert_eq!(
        local_error(
            &card("4242424242424242", 4, "123", ""),
            &stripe(false),
            today
        ),
        None
    );
}

#[test]
fn nothing_prints_card_data() {
    let card = test_card();
    assert_eq!(format!("{card:?}"), "CardDetails(..)");
    let credentials = NewCredentials::new("visa *4242".into(), r#"{"id":"tok_x"}"#.into());
    assert!(!format!("{credentials:?}").contains("tok_x"));
}

#[test]
fn providers_from_the_form() {
    use crate::telegram::envelope::PaymentProviderKind;
    let stripe_kind = PaymentProviderKind::Stripe {
        publishable_key: "pk".into(),
        need_country: true,
        need_postal_code: false,
        need_cardholder_name: true,
    };
    let provider = NativeProvider::for_form(&stripe_kind, false).unwrap();
    assert!(provider.needs(CardField::Country));
    assert!(!provider.needs(CardField::Zip));
    assert!(provider.needs(CardField::Name));
    let keyless = PaymentProviderKind::Stripe {
        publishable_key: String::new(),
        need_country: false,
        need_postal_code: false,
        need_cardholder_name: false,
    };
    assert_eq!(NativeProvider::for_form(&keyless, false), None);
    let web = PaymentProviderKind::Web {
        url: "https://pay.example".into(),
    };
    assert_eq!(NativeProvider::for_form(&web, false), None);
    let sg = PaymentProviderKind::SmartGlocal {
        public_token: "t".into(),
        tokenize_url: String::new(),
    };
    let provider = NativeProvider::for_form(&sg, true).unwrap();
    assert!(!provider.needs(CardField::Name));
    assert!(provider.needs(CardField::Cvc));
}
