use super::*;

/// Slice A2: `passwordState` with a pending recovery-email
/// confirmation (schema 1.8.67, line 273; code info line 83).
#[test]
fn password_state_parses_pending_email() {
    let json = r#"{"@type":"passwordState","has_password":true,"password_hint":"street","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"i***@example.com","length":6},"login_email_address_pattern":"","pending_reset_date":0}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::PasswordState { state }) => {
            assert!(state.has_password);
            assert_eq!(state.password_hint, "street");
            assert!(!state.has_recovery_email_address);
            assert_eq!(
                state.pending_email_pattern.as_deref(),
                Some("i***@example.com")
            );
            assert_eq!(state.pending_email_code_length, 6);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A2: null `recovery_email_address_code_info` (no pending
/// confirmation) parses to `None`, never an error.
#[test]
fn password_state_parses_null_code_info() {
    let json = r#"{"@type":"passwordState","has_password":false,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0}"#;
    let env = parse_envelope(json).unwrap();
    match env.payload {
        EnvelopePayload::Settings(SettingsPayload::PasswordState { state }) => {
            assert!(!state.has_password);
            assert_eq!(state.pending_email_pattern, None);
            assert_eq!(state.pending_email_code_length, 0);
        }
        other => panic!("unexpected {other:?}"),
    }
}

/// Slice A2: every 2FA constructor this slice relies on must exist
/// verbatim in the pinned schema (1.8.67).
#[test]
fn schema_pins_two_step_constructors() {
    let schema = include_str!("../../../schema/td_api.tl");
    for line in [
        "passwordState has_password:Bool password_hint:string has_recovery_email_address:Bool has_passport_data:Bool recovery_email_address_code_info:emailAddressAuthenticationCodeInfo login_email_address_pattern:string pending_reset_date:int32 = PasswordState;",
        "emailAddressAuthenticationCodeInfo email_address_pattern:string length:int32 = EmailAddressAuthenticationCodeInfo;",
        "getPasswordState = PasswordState;",
        "setPassword old_password:string new_password:string new_hint:string set_recovery_email_address:Bool new_recovery_email_address:string = PasswordState;",
        "setRecoveryEmailAddress password:string new_recovery_email_address:string = PasswordState;",
        "resendRecoveryEmailAddressCode = PasswordState;",
        "cancelRecoveryEmailAddressVerification = PasswordState;",
    ] {
        assert!(
            schema.lines().any(|l| l == line),
            "schema pin missing: {line}"
        );
    }
}
