//! Driver tests: TDLib parameters, sign-in states, QR login, 2FA, close and logout.
use super::*;

#[test]
fn driver_sends_parameters_then_reaches_wait_phone_via_injection() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let kick = driver.kickoff().unwrap();
    assert_eq!(kick.0, 1);

    let seq = AtomicU64::new(0);
    let wait_params = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_params).unwrap();
    assert!(driver.parameters_sent());

    let sent = recorder.snapshot();
    assert_eq!(sent.len(), 2); // getAuthorizationState + setTdlibParameters
    assert!(sent[0].contains("getAuthorizationState"));
    assert!(sent[1].contains("setTdlibParameters"));
    assert!(sent[1].contains("\"api_id\":99"));
    assert!(sent[1].contains("unit-test-hash-not-for-network"));
    assert!(!sink.rendered().contains("unit-test-hash"));

    let ok = copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap();
    driver.ingest(ok).unwrap();
    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitPhoneNumber
    ));
    assert_eq!(
        driver.session.auth_view.action,
        crate::auth::AuthAction::EnterPhone
    );

    let phone_extra = driver.submit_phone("+15551212").unwrap();
    let sent = recorder.snapshot();
    let phone_json = sent.last().unwrap();
    assert!(phone_json.contains("setAuthenticationPhoneNumber"));
    assert!(phone_json.contains(&format!("\"@extra\":\"{}\"", phone_extra.0)));
    assert!(phone_json.contains("+15551212"));
    assert!(!sink.rendered().contains("+15551212"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_submits_code_and_password_only_in_matching_states() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    assert_eq!(
        driver.submit_code("12345"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.submit_password("secret"),
        Err(ConnectSendError::InvalidRequest)
    );

    let seq = AtomicU64::new(0);
    let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_code).unwrap();
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitCode {
            code_length: Some(5),
            ..
        }
    ));
    assert_eq!(
        driver.submit_password("secret"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.submit_code("  "),
        Err(ConnectSendError::InvalidRequest)
    );
    let code_extra = driver.submit_code("  12345 ").unwrap();
    let sent = recorder.snapshot();
    let code_json = sent.last().unwrap();
    assert!(code_json.contains("checkAuthenticationCode"));
    assert!(code_json.contains(&format!("\"@extra\":\"{}\"", code_extra.0)));
    assert!(code_json.contains("\"code\":\"12345\""));
    assert!(!sink.rendered().contains("12345"));

    let wait_password = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_password).unwrap();
    assert_eq!(
        driver.submit_code("12345"),
        Err(ConnectSendError::InvalidRequest)
    );
    let pw_extra = driver.submit_password(" unit-pw ").unwrap();
    let sent = recorder.snapshot();
    let pw_json = sent.last().unwrap();
    assert!(pw_json.contains("checkAuthenticationPassword"));
    assert!(pw_json.contains(&format!("\"@extra\":\"{}\"", pw_extra.0)));
    // Password is not trimmed.
    assert!(pw_json.contains("\"password\":\" unit-pw \""));
    assert!(!sink.rendered().contains("unit-pw"));
    assert!(!sink.rendered().contains("CANARY_HINT"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_resend_code_and_qr_login_only_in_matching_states() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);

    // Both actions are gated: nothing valid to do in the initial state.
    assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
    assert_eq!(
        driver.request_qr_login(),
        Err(ConnectSendError::InvalidRequest)
    );

    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();
    // Resend needs WaitCode; QR login can start at the phone screen.
    assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
    let qr_extra = driver.request_qr_login().unwrap();
    let sent = recorder.snapshot();
    let qr_json = sent.last().unwrap();
    assert!(qr_json.contains("requestQrCodeAuthentication"));
    assert!(qr_json.contains("\"other_user_ids\":[]"));
    assert!(qr_json.contains(&format!("\"@extra\":\"{}\"", qr_extra.0)));

    let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_code).unwrap();
    // Clear the earlier QR query: switching states doesn't imply its answer arrived.
    driver.session.requests.take(qr_extra);
    let code_qr = driver.request_qr_login().unwrap();
    driver.session.requests.take(code_qr);
    let resend_extra = driver.resend_code().unwrap();
    let sent = recorder.snapshot();
    let resend_json = sent.last().unwrap();
    assert!(resend_json.contains("resendAuthenticationCode"));
    assert!(resend_json.contains("resendCodeReasonUserRequest"));
    assert!(resend_json.contains(&format!("\"@extra\":\"{}\"", resend_extra.0)));

    // The QR link from the auth update lands on the session state and is
    // never written to diagnostics.
    let wait_qr = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitOtherDeviceConfirmation","link":"tg://login/?token=unit-test-token"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_qr).unwrap();
    assert!(matches!(
        &driver.session.auth,
        AuthorizationState::WaitOtherDeviceConfirmation { link }
        if link == "tg://login/?token=unit-test-token"
    ));
    assert!(!sink.rendered().contains("unit-test-token"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn qr_recovery_accepts_supported_states_and_waits_for_pending_auth() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for state in [
        AuthorizationState::WaitPhoneNumber,
        AuthorizationState::WaitPremiumPurchase {
            premium_day_count: 0,
            support_email_address: String::new(),
            support_email_subject: String::new(),
        },
        AuthorizationState::WaitEmailAddress,
        AuthorizationState::WaitEmailCode {
            email_pattern: "u***@example.com".into(),
            code_length: Some(6),
            reset: Default::default(),
        },
        AuthorizationState::WaitCode {
            code_length: Some(5),
            delivery: Default::default(),
        },
        AuthorizationState::WaitPassword {
            has_recovery_email: false,
        },
        AuthorizationState::WaitRegistration { terms: None },
    ] {
        driver.session.auth = state;
        let pending = driver.session.request(RequestPurpose::SetPhoneNumber, None);
        let before = recorder.snapshot().len();
        assert_eq!(
            driver.request_qr_login(),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(recorder.snapshot().len(), before);
        driver.session.requests.take(pending);
        let qr = driver.request_qr_login().unwrap();
        assert_eq!(
            driver.request_qr_login(),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(recorder.snapshot().len(), before + 1);
        let error = copy_and_parse(
            &format!(r#"{{"@type":"error","code":429,"message":"FLOOD_WAIT_5 CANARY_AUTH_TOKEN","@extra":"{}"}}"#, qr.0),
            &seq, &dyn_sink,
        ).unwrap();
        driver.ingest(error).unwrap();
        assert!(!driver.session.requests.has_auth_submit());
        assert!(driver.session.last_auth_error.is_some());
        let retry = driver.request_qr_login().unwrap();
        assert!(driver.session.last_auth_error.is_none());
        driver.session.requests.take(retry);
    }
    for state in [
        AuthorizationState::Ready,
        AuthorizationState::Closed,
        AuthorizationState::WaitOtherDeviceConfirmation {
            link: "tg://login?token=CANARY_AUTH_TOKEN".into(),
        },
        AuthorizationState::Unknown("future".into()),
    ] {
        driver.session.auth = state;
        let before = recorder.snapshot().len();
        assert_eq!(
            driver.request_qr_login(),
            Err(ConnectSendError::InvalidRequest)
        );
        assert_eq!(recorder.snapshot().len(), before);
    }
    assert!(!sink.rendered().contains("CANARY_AUTH_TOKEN"));
    std::fs::remove_dir_all(dir).unwrap();
}

/// Slice A2: the 2FA driver gates sends, dedupes the fetch, shapes
/// `setPassword` correctly, and never leaks passwords into
/// diagnostics.
#[test]
fn driver_two_step_password_ops_gate_dedupe_and_shape() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);

    // Gated before the chats path is active.
    assert_eq!(
        driver.fetch_password_state(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_two_step_password("", "s3cret", "", None),
        Err(ConnectSendError::InvalidRequest)
    );

    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    // Fetch sends `getPasswordState` once; a second fetch dedupes.
    let fetch_extra = driver.fetch_password_state().unwrap().unwrap();
    assert_eq!(driver.fetch_password_state(), Ok(None));
    let sent = recorder.snapshot();
    let fetch_json = sent.last().unwrap();
    assert!(fetch_json.contains("\"getPasswordState\""));
    assert!(fetch_json.contains(&format!("\"@extra\":\"{}\"", fetch_extra.0)));

    // Doomed requests are rejected before leaving: disable needs the
    // current password; recovery email needs password + address.
    assert_eq!(
        driver.set_two_step_password("", "", "", None),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_recovery_email("", "me@example.com"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_recovery_email("s3cret", ""),
        Err(ConnectSendError::InvalidRequest)
    );
    // One op in flight: a second send is rejected (no double-send).
    assert_eq!(
        driver.set_two_step_password("", "s3cret", "hint", Some("me@example.com")),
        Err(ConnectSendError::InvalidRequest)
    );

    // The `passwordState` answer lands on the session, clears loading,
    // and the password never reaches diagnostics.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":false,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        fetch_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.password_state.is_some());
    assert!(!driver.session.password_state_loading);

    // Enable: empty old password, email in the same call.
    let enable_extra = driver
        .set_two_step_password("", "s3cret", "hint", Some("me@example.com"))
        .unwrap();
    let sent = recorder.snapshot();
    let enable_json: serde_json::Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(enable_json["@type"], "setPassword");
    assert_eq!(enable_json["old_password"], "");
    assert_eq!(enable_json["new_password"], "s3cret");
    assert_eq!(enable_json["new_hint"], "hint");
    assert_eq!(enable_json["set_recovery_email_address"], true);
    assert_eq!(enable_json["new_recovery_email_address"], "me@example.com");
    assert_eq!(
        enable_json["@extra"],
        serde_json::Value::String(enable_extra.0.to_string())
    );

    // Answer the enable: password now set, recovery email confirmed.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        enable_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        driver
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.has_password)
    );

    // Resend/abort are meaningless without a pending confirmation.
    assert_eq!(
        driver.resend_recovery_email_code(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.cancel_recovery_email_setup(),
        Err(ConnectSendError::InvalidRequest)
    );

    // New recovery email → pending confirmation state.
    let email_extra = driver
        .set_recovery_email("s3cret", "new@example.com")
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver
            .session
            .password_state
            .as_ref()
            .and_then(|s| s.pending_email_pattern.clone())
            .as_deref(),
        Some("n***@example.com")
    );

    // Now resend and abort send their requests.
    let resend_extra = driver.resend_recovery_email_code().unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.last()
            .unwrap()
            .contains("resendRecoveryEmailAddressCode")
    );
    // One in flight blocks the abort until the resend answers.
    assert_eq!(
        driver.cancel_recovery_email_setup(),
        Err(ConnectSendError::InvalidRequest)
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        resend_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Pending again → abort sends `cancelRecoveryEmailAddressVerification`.
    let email_extra2 = driver
        .set_recovery_email("s3cret", "new@example.com")
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra2.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.cancel_recovery_email_setup().unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.last()
            .unwrap()
            .contains("cancelRecoveryEmailAddressVerification")
    );

    // Passwords ride request JSON only — never diagnostics.
    for token in ["s3cret", "me@example.com", "new@example.com"] {
        assert!(
            !sink.rendered().contains(token),
            "secret leaked to diagnostics: {token}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wait_closed_sends_close_and_reaches_closed_via_injection() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    let seq = AtomicU64::new(0);
    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();

    let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
    let mut iter = envelopes.into_iter();
    assert!(wait_closed(
        &mut driver,
        |_| iter.next(),
        Duration::from_secs(2)
    ));
    assert!(matches!(driver.session.auth, AuthorizationState::Closed));
    let sent = recorder.snapshot();
    let close_json = sent.last().expect("close request");
    assert!(close_json.contains("\"@type\":\"close\""));
    assert!(!sink.rendered().contains("unit-test-hash"));
    // Second call is a no-op once Closed (no extra send).
    let before = sent.len();
    assert!(wait_closed(&mut driver, |_| None, Duration::ZERO));
    assert_eq!(recorder.snapshot().len(), before);
    drop(driver);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice auth-logout-warning: `request_logout` sends `logOut` (not
/// `close`), is guarded on Ready, and the
/// `LoggingOut → Closed` transition lands the session in Closed.
#[test]
fn request_logout_sends_logout_then_logging_out_then_closed() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    // Guard: no logout before authorization.
    assert!(driver.request_logout().is_err());

    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    let extra = driver.request_logout().unwrap();
    assert_eq!(
        driver.session.requests.purpose(extra),
        Some(RequestPurpose::LogOut)
    );
    let sent = recorder.snapshot();
    let logout_json = sent.last().expect("logOut request");
    assert!(logout_json.contains("\"@type\":\"logOut\""));
    assert!(logout_json.contains(&format!("\"@extra\":\"{}\"", extra.0)));
    assert!(!sink.rendered().contains("unit-test-hash"));

    // TDLib's answer is `ok`; the teardown arrives as auth-state updates.
    driver
        .ingest(
            copy_and_parse(
                &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.as_extra()),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateLoggingOut"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::LoggingOut
    ));
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(matches!(driver.session.auth, AuthorizationState::Closed));

    // No second logout once closed (guarded: Ready only).
    assert!(driver.request_logout().is_err());
    drop(driver);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_tdjson_message_is_actionable() {
    let msg = ConnectBlocker::MissingTdjson.user_message();
    assert!(msg.contains("QUILL_TDJSON_PATH"));
    assert!(msg.contains("never searched"));
}
