//! Connect-driver tests: signing in with a passkey from the QR screen
//! (recorded JSON only; no account is signed in and no key is touched).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use crate::webauthn::Assertion;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;
struct Parts {
    driver: ConnectDriver<Recorder>,
    recorder: Recorder,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

const OPTIONS: &str = r#"{"publicKey":{"challenge":"AQID","rpId":"telegram.org","timeout":60000,"userVerification":"preferred"}}"#;

fn feed(p: &mut Parts, json: Value) {
    let parsed = copy_and_parse(&json.to_string(), &p.seq, &p.sink).unwrap();
    p.driver.ingest(parsed).unwrap();
}

fn count(recorder: &RecordingSender, type_name: &str) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| serde_json::from_str::<Value>(json).is_ok_and(|v| v["@type"] == type_name))
        .count()
}

fn answer(p: &mut Parts, request: &str, mut body: Value) {
    let sent = sent_request(&p.recorder, request);
    body["@extra"] = sent["@extra"].clone();
    feed(p, body);
}

fn auth(p: &mut Parts, state: Value) {
    feed(
        p,
        json!({"@type": "updateAuthorizationState", "authorization_state": state}),
    );
}

fn display_passkeys(value: bool) -> Value {
    json!({"@type": "jsonValueObject", "members": [
        {"@type": "jsonObjectMember", "key": "settings_display_passkeys",
         "value": {"@type": "jsonValueBoolean", "value": value}}]})
}

/// Signed out on the QR screen.
fn on_qr_screen() -> Parts {
    let (_dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let mut p = Parts {
        driver,
        recorder,
        sink,
        seq: AtomicU64::new(0),
    };
    auth(
        &mut p,
        json!({"@type": "authorizationStateWaitOtherDeviceConfirmation",
               "link": "tg://login?token=unit-test"}),
    );
    p
}

fn assertion(user_handle: &[u8]) -> Assertion {
    Assertion {
        credential_id: vec![9, 9, 9],
        authenticator_data: vec![1; 37],
        signature: vec![2; 70],
        user_handle: user_handle.to_vec(),
        client_data_json: crate::webauthn::options::client_data_json("webauthn.get", &[1, 2, 3]),
    }
}

#[test]
fn the_app_config_is_asked_for_once_while_signed_out_and_turns_the_link_on() {
    let mut p = on_qr_screen();
    assert_eq!(count(&p.recorder, "getApplicationConfig"), 1);
    assert!(!p.driver.session.settings.passkeys.login_link_shown());
    // The link is off until the server says so: no request.
    assert!(p.driver.begin_login().is_err());
    answer(&mut p, "getApplicationConfig", display_passkeys(true));
    assert_eq!(
        p.driver.session.settings.passkeys.login_link_shown(),
        crate::passkeys::webauthn_supported()
    );
    auth(
        &mut p,
        json!({"@type": "authorizationStateWaitPhoneNumber"}),
    );
    assert_eq!(count(&p.recorder, "getApplicationConfig"), 1);
}

#[test]
fn a_switched_off_config_keeps_the_link_hidden() {
    let mut p = on_qr_screen();
    answer(&mut p, "getApplicationConfig", display_passkeys(false));
    assert!(!p.driver.session.settings.passkeys.login_link_shown());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
mod with_webauthn {
    use super::*;

    fn link_on() -> Parts {
        let mut p = on_qr_screen();
        answer(&mut p, "getApplicationConfig", display_passkeys(true));
        p
    }

    #[test]
    fn options_start_the_ceremony_and_are_reused_while_fresh() {
        let mut p = link_on();
        assert!(p.driver.begin_login().unwrap().is_some());
        assert!(p.driver.passkey_login_pending());
        // A second click while the options are on their way sends nothing.
        assert!(p.driver.begin_login().unwrap().is_none());
        assert_eq!(count(&p.recorder, "getAuthenticationPasskeyParameters"), 1);
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "text", "text": OPTIONS}),
        );
        let login = &mut p.driver.session.settings.passkeys.login;
        let options = login.ceremony.take().expect("the ceremony starts");
        assert_eq!(options.rp_id, "telegram.org");
        assert_eq!(options.challenge, vec![1, 2, 3]);
        assert!(!p.driver.passkey_login_pending());
        // Cancelled and tried again within the timeout: the same options.
        assert!(p.driver.begin_login().unwrap().is_none());
        assert!(p.driver.session.settings.passkeys.login.ceremony.is_some());
        assert_eq!(count(&p.recorder, "getAuthenticationPasskeyParameters"), 1);
    }

    #[test]
    fn the_assertion_goes_to_check_authentication_passkey() {
        let mut p = link_on();
        p.driver.begin_login().unwrap();
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "text", "text": OPTIONS}),
        );
        let sent = p.driver.finish_login(&assertion(b"2:777")).unwrap();
        assert!(sent.is_some());
        let check = sent_request(&p.recorder, "checkAuthenticationPasskey");
        assert_eq!(check["credential_id"], "CQkJ");
        assert_eq!(
            check["client_data"],
            r#"{"challenge":"AQID","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.get"}"#
        );
        assert_eq!(check["user_handle"], "Mjo3Nzc=");
        assert!(p.driver.passkey_login_pending());
        // One at a time.
        assert!(p.driver.finish_login(&assertion(b"2:777")).is_err());
        answer(&mut p, "checkAuthenticationPasskey", json!({"@type": "ok"}));
        let login = &p.driver.session.settings.passkeys.login;
        assert!(!login.checking && login.error.is_none() && login.pending.is_none());
        // A password-protected account: TDLib moves to the password screen.
        auth(
            &mut p,
            json!({"@type": "authorizationStateWaitPassword", "has_recovery_email_address": false,
                   "password_hint": "", "recovery_email_address_pattern": ""}),
        );
        assert!(matches!(
            p.driver.session.auth,
            AuthorizationState::WaitPassword { .. }
        ));
        assert!(p.driver.begin_login().is_err());
    }

    #[test]
    fn a_refused_assertion_shows_the_server_error_and_forgets_the_options() {
        let mut p = link_on();
        p.driver.begin_login().unwrap();
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "text", "text": OPTIONS}),
        );
        p.driver.finish_login(&assertion(b"2:777")).unwrap();
        answer(
            &mut p,
            "checkAuthenticationPasskey",
            json!({"@type": "error", "code": 400, "message": "PASSKEY_CREDENTIAL_NOT_FOUND"}),
        );
        let login = &p.driver.session.settings.passkeys.login;
        assert_eq!(login.error.as_deref(), Some("PASSKEY_CREDENTIAL_NOT_FOUND"));
        assert!(login.pending.is_none() && !login.checking);
        // The next click asks for new options and clears the error.
        assert!(p.driver.begin_login().unwrap().is_some());
        assert_eq!(count(&p.recorder, "getAuthenticationPasskeyParameters"), 2);
        assert!(p.driver.session.settings.passkeys.login.error.is_none());
    }

    #[test]
    fn a_handle_without_a_data_centre_fails_before_sending() {
        let mut p = link_on();
        p.driver.begin_login().unwrap();
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "text", "text": OPTIONS}),
        );
        assert_eq!(p.driver.finish_login(&assertion(b"777")).unwrap(), None);
        assert_eq!(count(&p.recorder, "checkAuthenticationPasskey"), 0);
        let login = &p.driver.session.settings.passkeys.login;
        assert_eq!(login.error.as_deref(), Some("PASSKEY_HANDLE_INVALID"));
        assert!(login.pending.is_none());
    }

    #[test]
    fn failed_or_unreadable_options_are_shown() {
        let mut p = link_on();
        p.driver.begin_login().unwrap();
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "text", "text": "{}"}),
        );
        let login = &p.driver.session.settings.passkeys.login;
        assert!(login.ceremony.is_none());
        assert_eq!(
            login.error.as_deref(),
            Some(crate::passkeys::login::UNREADABLE_OPTIONS)
        );
        p.driver.begin_login().unwrap();
        answer(
            &mut p,
            "getAuthenticationPasskeyParameters",
            json!({"@type": "error", "code": 429, "message": "FLOOD_WAIT_30"}),
        );
        let login = &p.driver.session.settings.passkeys.login;
        assert_eq!(login.error.as_deref(), Some("FLOOD_WAIT_30"));
    }

    #[test]
    fn not_offered_once_signed_in() {
        let mut p = link_on();
        auth(&mut p, json!({"@type": "authorizationStateReady"}));
        assert!(p.driver.begin_login().is_err());
        assert!(p.driver.finish_login(&assertion(b"2:1")).is_err());
    }
}
