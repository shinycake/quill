use super::super::{ConnectDriver, ConnectSendError, JsonSender, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use serde_json::json;
use std::sync::{Arc, atomic::AtomicU64};

#[test]
fn email_login_transitions_errors_resend_and_transport_retry() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap()
    };
    assert!(driver.submit_email("alice@example.com").is_err());
    ingest(
        &mut driver,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitEmailAddress"}}),
    );
    assert!(driver.submit_email("missing-at").is_err());
    assert!(driver.submit_email("a@b@c").is_err());
    assert!(driver.submit_email("a b@example.com").is_err());
    let first = driver.submit_email(" alice@example.com ").unwrap();
    assert_eq!(
        sent_request(&recorder, "setAuthenticationEmailAddress")["email_address"],
        "alice@example.com"
    );
    assert!(driver.submit_email("alice@example.com").is_err());
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":first.as_extra(),"code":429,"message":"FLOOD_WAIT_10"}),
    );
    assert_eq!(
        driver.session.last_auth_error.unwrap().user_message(),
        "too many email attempts — try again in 10 seconds"
    );
    let old = driver.submit_email("alice@example.com").unwrap();
    ingest(
        &mut driver,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitEmailCode","code_info":{"email_address_pattern":"a***@example.com","length":6}}}),
    );
    assert!(
        matches!(&driver.session.auth,AuthorizationState::WaitEmailCode { email_pattern, code_length:Some(6) } if email_pattern=="a***@example.com")
    );
    assert!(driver.session.last_auth_error.is_none());
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":old.as_extra(),"code":400,"message":"private-email"}),
    );
    assert!(driver.session.last_auth_error.is_none()); // Old phase cannot overwrite the current screen.
    assert!(driver.submit_email("alice@example.com").is_err());
    assert!(driver.submit_code(" ").is_err());
    let code = driver.submit_code("123456").unwrap();
    assert_eq!(
        sent_request(&recorder, "checkAuthenticationEmailCode")["code"],
        json!({"@type":"emailAddressAuthenticationCode","code":"123456"})
    );
    assert!(driver.submit_code("123456").is_err());
    ingest(
        &mut driver,
        json!({"@type":"error","@extra":code.as_extra(),"code":400,"message":"123456"}),
    );
    assert_eq!(
        driver.session.last_auth_error.unwrap().user_message(),
        "code not accepted"
    );
    let resend = driver.resend_code().unwrap();
    assert_eq!(
        sent_request(&recorder, "resendAuthenticationCode")["@type"],
        "resendAuthenticationCode"
    );
    assert!(driver.resend_code().is_err());
    ingest(
        &mut driver,
        json!({"@type":"ok","@extra":resend.as_extra()}),
    );
    ingest(
        &mut driver,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"type":{"length":5}}}}),
    );
    driver.submit_code("54321").unwrap();
    assert_eq!(
        sent_request(&recorder, "checkAuthenticationCode")["code"],
        "54321"
    );
    ingest(
        &mut driver,
        json!({"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}),
    );
    assert!(driver.submit_code("123456").is_err());
    assert!(driver.resend_code().is_err());

    struct Fails;
    impl JsonSender for Fails {
        fn send_json(&self, _: &str) -> Result<(), ConnectSendError> {
            Err(ConnectSendError::Native)
        }
    }
    let (failed_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let mut failing = ConnectDriver::new(
        Session::new(AccountKey::primary(), sink),
        Fails,
        support::test_credentials(),
        prepared,
    );
    failing.session.auth = AuthorizationState::WaitEmailAddress;
    assert!(failing.submit_email("a@example.com").is_err());
    assert!(
        !failing
            .session
            .requests
            .has_purpose(RequestPurpose::SetAuthenticationEmail)
    );
    failing.session.auth = AuthorizationState::WaitEmailCode {
        email_pattern: String::new(),
        code_length: None,
    };
    assert!(failing.submit_code("123456").is_err());
    assert!(
        !failing
            .session
            .requests
            .has_purpose(RequestPurpose::CheckAuthenticationEmailCode)
    );
    assert!(failing.resend_code().is_err());
    assert!(
        !failing
            .session
            .requests
            .has_purpose(RequestPurpose::ResendAuthenticationCode)
    );
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(failed_dir);
}
