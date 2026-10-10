use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{AuthorizationState, CodeKind};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicU64};

fn auth_state_after(state: Value) -> AuthorizationState {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver: ConnectDriver<Arc<RecordingSender>> =
        ready_driver(&recorder, prepared, &sink, &seq);
    let payload = json!({"@type":"updateAuthorizationState","authorization_state":state});
    driver
        .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
        .unwrap();
    driver.session.auth.clone()
}

fn code_state(ty: Value) -> AuthorizationState {
    auth_state_after(json!({
        "@type":"authorizationStateWaitCode",
        "code_info":{"@type":"authenticationCodeInfo","phone_number":"+15550100199","type":ty,"timeout":30}
    }))
}

#[test]
fn flash_and_missed_call_details_are_parsed() {
    let AuthorizationState::WaitCode {
        delivery,
        code_length,
    } = code_state(json!({"@type":"authenticationCodeTypeFlashCall","pattern":"+155501*****"}))
    else {
        panic!("expected WaitCode");
    };
    assert_eq!(delivery.kind, CodeKind::FlashCall);
    assert_eq!(delivery.detail.call_number, "+155501*****");
    assert_eq!(code_length, None);

    let AuthorizationState::WaitCode {
        delivery,
        code_length,
    } = code_state(
        json!({"@type":"authenticationCodeTypeMissedCall","phone_number_prefix":"+155501","length":6}),
    )
    else {
        panic!("expected WaitCode");
    };
    assert_eq!(delivery.kind, CodeKind::MissedCall);
    assert_eq!(delivery.detail.call_number, "+155501");
    assert_eq!(delivery.detail.missed_digits, Some(6));
    assert_eq!(code_length, Some(6));
}

#[test]
fn fragment_url_is_kept_only_when_it_is_plain_https() {
    for (url, kept) in [
        ("https://fragment.example.invalid/login", true),
        ("http://fragment.example.invalid/login", false),
        ("javascript:alert(1)", false),
        ("https://a.invalid/x y", false),
        ("", false),
    ] {
        let AuthorizationState::WaitCode {
            delivery,
            code_length,
        } = code_state(json!({"@type":"authenticationCodeTypeFragment","url":url,"length":5}))
        else {
            panic!("expected WaitCode");
        };
        assert_eq!(delivery.kind, CodeKind::Fragment);
        if kept {
            assert_eq!(delivery.detail.url, url);
        } else {
            assert!(delivery.detail.url.is_empty(), "{url}");
        }
        assert_eq!(code_length, Some(5));
    }
}

#[test]
fn firebase_types_keep_no_device_secrets() {
    for ty in [
        json!({"@type":"authenticationCodeTypeFirebaseAndroid","device_verification_parameters":{"@type":"firebaseDeviceVerificationParametersSafetyNet","nonce":"c2VjcmV0"},"length":6}),
        json!({"@type":"authenticationCodeTypeFirebaseIos","receipt":"secret-receipt","push_timeout":30,"length":6}),
    ] {
        let AuthorizationState::WaitCode {
            delivery,
            code_length,
        } = code_state(ty)
        else {
            panic!("expected WaitCode");
        };
        assert_eq!(delivery.kind, CodeKind::Firebase);
        assert_eq!(delivery.detail, Default::default());
        assert_eq!(code_length, Some(6));
    }
}

#[test]
fn premium_purchase_state_keeps_only_the_explainer_fields() {
    let state = auth_state_after(json!({
        "@type":"authorizationStateWaitPremiumPurchase",
        "store_product_id":"premium.fake",
        "premium_day_count":30,
        "support_email_address":"premium-support@example.invalid",
        "support_email_subject":"Premium sign-in"
    }));
    assert_eq!(
        state,
        AuthorizationState::WaitPremiumPurchase {
            premium_day_count: 30,
            support_email_address: "premium-support@example.invalid".into(),
            support_email_subject: "Premium sign-in".into(),
        }
    );
    // A bare state still parses.
    assert_eq!(
        auth_state_after(json!({"@type":"authorizationStateWaitPremiumPurchase"})),
        AuthorizationState::WaitPremiumPurchase {
            premium_day_count: 0,
            support_email_address: String::new(),
            support_email_subject: String::new(),
        }
    );
}
