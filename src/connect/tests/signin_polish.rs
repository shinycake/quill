use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::RequestPurpose;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{
    AuthorizationState, CodeDelivery, CodeKind, EmailResetState, ErrorClass,
};
use serde_json::json;
use std::sync::{Arc, atomic::AtomicU64};

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    payload: serde_json::Value,
) {
    driver
        .ingest(copy_and_parse(&payload.to_string(), seq, sink).unwrap())
        .unwrap();
}

#[test]
fn code_state_carries_delivery_and_next_type() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","phone_number":"+15550100199","type":{"@type":"authenticationCodeTypeTelegramMessage","length":5},"next_type":{"@type":"authenticationCodeTypeSms","length":0},"timeout":60}}}),
    );
    assert_eq!(
        driver.session.auth,
        AuthorizationState::WaitCode {
            code_length: Some(5),
            delivery: CodeDelivery {
                kind: CodeKind::TelegramMessage,
                next: Some(CodeKind::Sms),
                timeout_secs: 60,
                detail: Default::default(),
            },
        }
    );
    // A null next_type means "no resend": the UI hides the control.
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"type":{"@type":"authenticationCodeTypeCall","length":6},"next_type":null,"timeout":0}}}),
    );
    let AuthorizationState::WaitCode { delivery, .. } = &driver.session.auth else {
        panic!("expected WaitCode");
    };
    assert_eq!(delivery.kind, CodeKind::Call);
    assert_eq!(delivery.next, None);
}

#[test]
fn wrong_number_resubmits_the_phone_from_the_code_step() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"type":{"length":5}}}}),
    );
    let id = driver.submit_phone("+15550100198").unwrap();
    assert_eq!(
        sent_request(&recorder, "setAuthenticationPhoneNumber")["phone_number"],
        "+15550100198"
    );
    // One authentication query at a time.
    assert!(driver.submit_phone("+15550100197").is_err());
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"ok","@extra":id.as_extra()}),
    );
    // Not from steps that do not accept a phone.
    driver.session.auth = AuthorizationState::WaitPassword {
        has_recovery_email: false,
    };
    assert!(driver.submit_phone("+15550100196").is_err());
}

#[test]
fn countries_and_default_code_are_fetched_once_and_stored() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let countries = driver.fetch_countries().unwrap().unwrap();
    assert_eq!(driver.fetch_countries().unwrap(), None, "one in flight");
    let code = driver.fetch_country_code().unwrap().unwrap();
    assert_eq!(driver.fetch_country_code().unwrap(), None);
    sent_request(&recorder, "getCountries");
    sent_request(&recorder, "getCountryCode");
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"countries","@extra":countries.as_extra(),"countries":[
            {"@type":"countryInfo","country_code":"US","name":"United States","english_name":"United States","flag_emoji":"US","is_hidden":false,"calling_codes":["1"]},
            {"@type":"countryInfo","country_code":"GB","name":"United Kingdom","english_name":"United Kingdom","flag_emoji":"GB","is_hidden":false,"calling_codes":["44"]}
        ]}),
    );
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"text","@extra":code.as_extra(),"text":"gb"}),
    );
    assert_eq!(driver.session.countries.as_ref().unwrap().len(), 2);
    assert_eq!(driver.session.guessed_country_iso.as_deref(), Some("GB"));
    assert_eq!(driver.fetch_countries().unwrap(), None, "cached");
    assert_eq!(driver.fetch_country_code().unwrap(), None, "cached");
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::GetCountries)
    );
}

#[test]
fn email_reset_only_where_tdlib_offers_it() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitEmailCode","code_info":{"email_address_pattern":"a***@example.com","length":6},"email_address_reset_state":null}}),
    );
    assert!(driver.reset_login_email().is_err(), "no reset offered");
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitEmailCode","code_info":{"email_address_pattern":"a***@example.com","length":6},"email_address_reset_state":{"@type":"emailAddressResetStateAvailable","wait_period":86400}}}),
    );
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitEmailCode {
            reset: EmailResetState::Available { wait_period: 86400 },
            ..
        }
    ));
    let id = driver.reset_login_email().unwrap();
    sent_request(&recorder, "resetAuthenticationEmailAddress");
    // TASK_ALREADY_EXISTS means a reset is already pending.
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":id.as_extra(),"code":400,"message":"TASK_ALREADY_EXISTS"}),
    );
    let err = driver.session.last_auth_error.unwrap();
    assert_eq!(err.class, ErrorClass::TaskAlreadyExists);
    assert_eq!(err.user_message(), "an email reset is already pending");
}

#[test]
fn phone_errors_are_classified_for_the_banned_box_and_hints() {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}),
    );
    for (message, class, line) in [
        (
            "PHONE_NUMBER_BANNED",
            ErrorClass::PhoneBanned,
            "This phone number is banned.",
        ),
        (
            "PHONE_NUMBER_INVALID",
            ErrorClass::PhoneInvalid,
            "Invalid phone number. Please try again.",
        ),
    ] {
        let id = driver.submit_phone("+15550100199").unwrap();
        ingest(
            &mut driver,
            &seq,
            &sink,
            json!({"@type":"error","@extra":id.as_extra(),"code":400,"message":message}),
        );
        let err = driver.session.last_auth_error.unwrap();
        assert_eq!(err.class, class);
        assert_eq!(err.user_message(), line);
    }
    let id = driver.submit_phone("+15550100199").unwrap();
    ingest(
        &mut driver,
        &seq,
        &sink,
        json!({"@type":"error","@extra":id.as_extra(),"code":400,"message":"PHONE_NUMBER_FLOOD"}),
    );
    assert!(
        driver
            .session
            .last_auth_error
            .unwrap()
            .user_message()
            .contains("too many times")
    );
}
