use super::*;

/// Slice A8: `authenticationCodeInfo` answer (schema 1.8.67, line 78).
#[test]
fn authentication_code_info_parses_phone_and_timeout() {
    let json = r#"{"@type":"authenticationCodeInfo","phone_number":"+15550199","type":{"@type":"authenticationCodeTypeSms","length":5},"next_type":null,"timeout":60}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::AuthenticationCodeInfo {
            phone_number,
            timeout,
        } => {
            assert_eq!(phone_number, "+15550199");
            assert_eq!(timeout, 60);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A8: missing fields degrade, never a parse error.
#[test]
fn authentication_code_info_missing_fields_degrade() {
    let env = parse_envelope(r#"{"@type":"authenticationCodeInfo"}"#).unwrap();
    match env.payload {
        EnvelopePayload::AuthenticationCodeInfo {
            phone_number,
            timeout,
        } => {
            assert_eq!(phone_number, "");
            assert_eq!(timeout, 0);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A8: every change-number constructor this slice relies on must
/// exist verbatim in the pinned schema (1.8.67).
#[test]
fn schema_pins_change_number_constructors() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "phoneNumberCodeTypeChange = PhoneNumberCodeType;",
        "sendPhoneNumberCode phone_number:string settings:phoneNumberAuthenticationSettings type:PhoneNumberCodeType = AuthenticationCodeInfo;",
        "resendPhoneNumberCode reason:ResendCodeReason = AuthenticationCodeInfo;",
        "checkPhoneNumberCode code:string = Ok;",
        "authenticationCodeInfo phone_number:string type:AuthenticationCodeType next_type:AuthenticationCodeType timeout:int32 = AuthenticationCodeInfo;",
        "resendCodeReasonUserRequest = ResendCodeReason;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}

/// Slice A7: `accountTtl` answer (schema 1.8.67, line 9053).
#[test]
fn account_ttl_parses_days() {
    let json = r#"{"@type":"accountTtl","days":180}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::AccountTtl { days } => assert_eq!(days, 180),
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A7: a missing `days` degrades to 0, never a parse error.
#[test]
fn account_ttl_missing_days_defaults_zero() {
    let json = r#"{"@type":"accountTtl"}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::AccountTtl { days } => assert_eq!(days, 0),
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A7: every account-lifecycle constructor this slice relies
/// on must exist verbatim in the pinned schema (1.8.67).
#[test]
fn schema_pins_account_lifecycle_constructors() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "accountTtl days:int32 = AccountTtl;",
        "setAccountTtl ttl:accountTtl = Ok;",
        "getAccountTtl = AccountTtl;",
        "deleteAccount reason:string password:string = Ok;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}
