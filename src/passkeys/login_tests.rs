use super::*;
use serde_json::Value;

fn assertion(user_handle: &[u8]) -> Assertion {
    Assertion {
        credential_id: vec![0xFB, 0xFF, 0x01],
        authenticator_data: vec![1, 2, 3],
        signature: vec![0xFF; 4],
        user_handle: user_handle.to_vec(),
        client_data_json: r#"{"challenge":"AQ","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.get"}"#.into(),
    }
}

#[test]
fn check_request_encodes_each_field_as_tdlib_reads_it() {
    let json: Value = serde_json::from_str(&check_authentication_passkey(
        RequestId(7),
        &assertion(b"2:123456"),
    ))
    .unwrap();
    assert_eq!(json["@type"], "checkAuthenticationPasskey");
    assert_eq!(json["@extra"], RequestId(7).as_extra());
    // credential_id: base64url, no padding (tdesktop `Base64UrlEncoding |
    // OmitTrailingEquals`).
    assert_eq!(json["credential_id"], "-_8B");
    // client_data is the JSON text itself.
    assert_eq!(
        json["client_data"],
        r#"{"challenge":"AQ","crossOrigin":false,"origin":"https://telegram.org","type":"webauthn.get"}"#
    );
    // bytes: standard base64.
    assert_eq!(json["authenticator_data"], "AQID");
    assert_eq!(json["signature"], "/////w==");
    assert_eq!(json["user_handle"], "MjoxMjM0NTY=");
}

#[test]
fn parameters_request_has_no_fields() {
    let json: Value =
        serde_json::from_str(&get_authentication_passkey_parameters(RequestId(3))).unwrap();
    assert_eq!(json["@type"], "getAuthenticationPasskeyParameters");
    assert_eq!(json.as_object().map(|o| o.len()), Some(2));
}

#[test]
fn user_handle_needs_one_colon_and_a_data_centre() {
    for good in [&b"2:1234"[..], b"1:", b" 4 :x", b"+5:abc", b"9999:z"] {
        assert!(user_handle_valid(good), "{good:?}");
    }
    for bad in [
        &b""[..],
        b"1234",
        b"0:12",
        b"-1:12",
        b"10000:12",
        b"a:12",
        b"1:2:3",
        b":12",
    ] {
        assert!(!user_handle_valid(bad), "{bad:?}");
    }
}

#[test]
fn options_are_kept_until_their_timeout_and_a_failure_forgets_them() {
    let json = r#"{"publicKey":{"challenge":"AQID","rpId":"telegram.org","timeout":1000,"userVerification":"required"}}"#;
    let now = Instant::now();
    let mut login = LoginState {
        stage: LoginStage::FetchingOptions(RequestId(4)),
        ..LoginState::default()
    };
    assert!(login.accept_parameters(Some(RequestId(4)), json, now));
    let options = login.take_ceremony().unwrap();
    assert_eq!(options.rp_id, "telegram.org");
    assert_eq!(options.challenge, vec![1, 2, 3]);
    assert!(login.take_ceremony().is_none(), "the UI takes them once");
    let pending = login.pending.clone().unwrap();
    assert!(pending.fresh(now + Duration::from_millis(999)));
    assert!(!pending.fresh(now + Duration::from_millis(1000)));
    login.stage = LoginStage::Checking(RequestId(5));
    login.fail("PASSKEY_CREDENTIAL_NOT_FOUND".into());
    assert!(login.pending.is_none() && !login.is_waiting());
    assert_eq!(login.error(), Some("PASSKEY_CREDENTIAL_NOT_FOUND"));
    login.stage = LoginStage::FetchingOptions(RequestId(6));
    assert!(!login.accept_parameters(Some(RequestId(6)), r#"{"publicKey":{}}"#, now));
    assert!(login.take_ceremony().is_none());
}

/// An answer for another request than the one the stage waits for changes
/// nothing, a request lost without an answer ends the wait, and leaving
/// the sign-in screens drops the step.
#[test]
fn the_stage_follows_its_own_request_and_has_a_way_out() {
    let json = r#"{"publicKey":{"challenge":"AQID","rpId":"telegram.org","timeout":1000,"userVerification":"required"}}"#;
    let now = Instant::now();
    let mut login = LoginState {
        stage: LoginStage::FetchingOptions(RequestId(4)),
        ..LoginState::default()
    };
    // A late answer to an older ask.
    assert!(login.accept_parameters(Some(RequestId(3)), json, now));
    assert!(matches!(login.stage, LoginStage::FetchingOptions(_)));
    assert!(!login.fail_request(Some(RequestId(3)), "OLD".into()));
    assert!(login.is_waiting());
    // The request is gone with no answer.
    login.drop_lost(|id| id != RequestId(4));
    assert!(matches!(login.stage, LoginStage::Idle));
    // A step under way when the QR screen goes away.
    login.stage = LoginStage::Checking(RequestId(7));
    login.leave_sign_in();
    assert!(matches!(login.stage, LoginStage::Idle) && login.pending.is_none());
    // The answer of the check after that leaves nothing behind.
    login.finish_check(Some(RequestId(7)));
    assert!(matches!(login.stage, LoginStage::Idle));
}
