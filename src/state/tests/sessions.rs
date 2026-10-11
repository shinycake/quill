//! State reducer tests: sessions.
use super::common::*;
use super::*;

#[test]
fn password_state_response_replaces_cache_and_clears_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(
        RequestPurpose::Auth(AuthPurpose::PasswordStateOp {
            op: PasswordOp::SetPassword,
        }),
        None,
    );
    session.auth_state.password_state_loading = true;
    session.auth_state.password_op_error = Some("stale".into());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"passwordState","has_password":true,"password_hint":"street","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
            extra.0
        ),
    );
    let state = session
        .auth_state
        .password_state
        .expect("password state cached");
    assert!(state.has_password);
    assert_eq!(state.password_hint, "street");
    assert!(state.has_recovery_email_address);
    assert_eq!(state.pending_email_pattern, None);
    assert!(!session.auth_state.password_state_loading);
    assert!(session.auth_state.password_op_error.is_none());
}

#[test]
fn stray_password_state_response_is_ignored() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"passwordState","has_password":true,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0}"#,
    );
    assert!(session.auth_state.password_state.is_none());
    assert!(!session.auth_state.password_state_loading);
}

#[test]
fn password_state_error_surfaces_honest_classified_line() {
    for (op, code, expect) in [
        (PasswordOp::SetPassword, 400, "wrong password"),
        (PasswordOp::DisablePassword, 400, "wrong password"),
        (PasswordOp::SetRecoveryEmail, 400, "email was rejected"),
        (PasswordOp::ResendCode, 429, "too many attempts"),
        (PasswordOp::Fetch, 429, "too many attempts"),
    ] {
        let (mut session, sink) = session();
        let seq = AtomicU64::new(0);
        let extra = session.request(
            RequestPurpose::Auth(AuthPurpose::PasswordStateOp { op }),
            None,
        );
        session.auth_state.password_state_loading = true;
        apply_json(
            &mut session,
            &seq,
            &sink,
            &format!(
                r#"{{"@type":"error","@extra":"{}","code":{code},"message":"SOME_TDLIB_ERROR"}}"#,
                extra.0
            ),
        );
        let line = session
            .auth_state
            .password_op_error
            .expect("error line set");
        assert!(line.contains(expect), "op {op:?} code {code}: {line}");
        assert!(!session.auth_state.password_state_loading);
        assert!(session.auth_state.password_state.is_none());
    }
}

#[test]
fn auth_password_ok_clears_last_error() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","code":400,"message":"PASSWORD_HASH_INVALID CANARY_PW","@extra":"{}"}}"#,
            extra.0
        ),
    );
    assert!(session.auth_state.last_auth_error.is_some());
    let extra = session.request(RequestPurpose::CheckAuthenticationPassword, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert!(session.auth_state.last_auth_error.is_none());
    assert!(!sink.rendered().contains("CANARY_PW"));
}

#[test]
fn b1_password_callback_error_surfaces_wrong_password() {
    // B1: TDLib error 400 on a `GetCallbackQueryAnswerWithPassword`
    // request surfaces as "wrong 2-step verification password"; other
    // errors get the generic bot-timeout note.
    let (mut session, sink) = session();
    let seq = AtomicU64::new(1);
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswerWithPassword,
        Some(ChatId(21)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PASSWORD_HASH_INVALID"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session
            .bots
            .last_callback_answer
            .as_ref()
            .map(|answer| answer.text.as_str()),
        Some("wrong 2-step verification password")
    );
    let extra = session.request(
        RequestPurpose::GetCallbackQueryAnswerWithPassword,
        Some(ChatId(21)),
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":502,"message":"Bad Gateway"}}"#,
            extra.0
        ),
    );
    assert_eq!(
        session
            .bots
            .last_callback_answer
            .as_ref()
            .map(|answer| answer.text.as_str()),
        Some("bot did not answer")
    );
}

#[test]
fn sessions_answer_replaces_cache_for_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetActiveSessions, None);
    session.settings.sessions_loading = true;
    session.settings.sessions_error = Some("stale".into());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"sessions","@extra":"{}","sessions":[{{"@type":"session","id":11,"is_current":true,"device_model":"Linux desktop","application_name":"Quill","application_version":"0.1","platform":"Linux","system_version":"6.8","last_active_date":1759000000,"ip_address":"1.2.3.4","location":"Austin, United States"}},{{"@type":"session","id":33,"is_current":false,"is_password_pending":true,"device_model":"Unknown","application_name":"Telegram Desktop","platform":"Windows","last_active_date":1758800000}}]}}"#,
            extra.0,
        ),
    );
    let sessions = session.settings.sessions.as_ref().expect("cached");
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|s| s.is_current && s.id == 11));
    assert!(sessions.iter().any(|s| s.is_password_pending && s.id == 33));
    assert!(!session.settings.sessions_loading);
    assert!(session.settings.sessions_error.is_none());
    assert!(!session.settings.sessions_stale);
}

#[test]
fn sessions_answer_ignored_without_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions = Some(Vec::new());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"sessions","sessions":[{"@type":"session","id":11,"is_current":true,"device_model":"X"}]}"#,
    );
    assert_eq!(session.settings.sessions.as_ref().unwrap().len(), 0);
}

#[test]
fn terminate_session_ok_keeps_cache_and_marks_stale() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions = Some(vec![ParsedSession {
        id: 22,
        is_current: false,
        is_password_pending: false,
        is_unconfirmed: false,
        can_accept_secret_chats: true,
        can_accept_calls: true,
        device_model: "iPhone".into(),
        application_name: "Telegram iOS".into(),
        application_version: "12.0".into(),
        platform: "iOS".into(),
        system_version: "18.0".into(),
        last_active_date: 1758900000,
        ip_address: "5.6.7.8".into(),
        location: "Tel Aviv, Israel".into(),
        ..Default::default()
    }]);
    session.settings.sessions_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 22 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    // The old cache stays visible until the authoritative refetch
    // replaces it — no optimistic deletion.
    assert_eq!(session.settings.sessions.as_ref().unwrap().len(), 1);
    assert!(session.settings.sessions_stale);
    assert!(!session.settings.sessions_mutating);
    assert!(session.settings.sessions_error.is_none());
}

#[test]
fn terminate_session_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::TerminateSession { session_id: 22 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SESSION_REVOKED"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.sessions_mutating);
    assert_eq!(
        session.settings.sessions_error.as_deref(),
        Some("Could not terminate the session: Telegram refused the request")
    );
}

#[test]
fn sessions_fetch_error_clears_loading() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions_loading = true;
    // A failed stale-refetch (e.g. after a terminate-ok marked the
    // cache stale) must NOT leave the cache stale — otherwise the
    // next ingest retries the fetch and flood state worsens.
    session.settings.sessions_stale = true;
    let extra = session.request(RequestPurpose::GetActiveSessions, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"FLOOD_WAIT_3"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.sessions_loading);
    // The failed stale-refetch clears staleness: no auto-retry on
    // the next ingest; the user retries via the Refresh button.
    assert!(!session.settings.sessions_stale);
    assert_eq!(
        session.settings.sessions_error.as_deref(),
        Some("Could not load the sessions list: try again in 3 seconds")
    );
}

#[test]
fn account_ttl_answer_writes_cache_for_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetAccountTtl, None);
    session.settings.account_ttl_loading = true;
    session.settings.account_error = Some("stale".into());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"accountTtl","@extra":"{}","days":180}}"#,
            extra.0
        ),
    );
    assert_eq!(session.settings.account_ttl_days, Some(180));
    assert!(!session.settings.account_ttl_loading);
    assert!(session.settings.account_error.is_none());
}

#[test]
fn account_ttl_fetch_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.account_ttl_loading = true;
    let extra = session.request(RequestPurpose::GetAccountTtl, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD_REQUEST"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.account_ttl_loading);
    assert_eq!(
        session.settings.account_error.as_deref(),
        Some("Could not load the account inactivity timer: Telegram refused the request")
    );
}

#[test]
fn code_info_answer_ignores_unknown_extra() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.auth_state.change_number_loading = true;
    let extra = session.request(RequestPurpose::SendPhoneNumberCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"authenticationCodeInfo","@extra":"{}","phone_number":"+15550199","type":{{"@type":"authenticationCodeTypeSms","length":5}},"next_type":null,"timeout":60}}"#,
            extra.0 + 1000
        ),
    );
    assert!(session.auth_state.change_number_phone.is_none());
    assert!(session.auth_state.change_number_loading);
}

#[test]
fn check_code_ok_updates_own_phone_number() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.my_user_id = Some(31);
    session.users.insert(
        31,
        ParsedUser {
            id: 31,
            phone_number: "+15550131".into(),
            ..Default::default()
        },
    );
    session.auth_state.change_number_phone = Some("+15550199".into());
    session.auth_state.change_number_checking = true;
    session.auth_state.change_number_error = Some("stale".into());
    let resend_extra = session.request(RequestPurpose::ResendPhoneNumberCode, None);
    let extra = session.request(RequestPurpose::CheckPhoneNumberCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(session.users.get(&31).unwrap().phone_number, "+15550199");
    assert!(session.auth_state.change_number_phone.is_none());
    assert!(!session.auth_state.change_number_checking);
    assert!(session.auth_state.change_number_error.is_none());
    // The stale resend purpose is gone: its late answer writes nothing.
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"authenticationCodeInfo","@extra":"{}","phone_number":"+15550199","type":{{"@type":"authenticationCodeTypeSms","length":5}},"next_type":null,"timeout":60}}"#,
            resend_extra.0
        ),
    );
    assert!(session.auth_state.change_number_phone.is_none());
    assert!(session.auth_state.change_number_error.is_none());
}

#[test]
fn code_check_error_keeps_pending_number() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.auth_state.change_number_phone = Some("+15550199".into());
    session.auth_state.change_number_checking = true;
    let extra = session.request(RequestPurpose::CheckPhoneNumberCode, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PHONE_CODE_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(!session.auth_state.change_number_checking);
    assert_eq!(
        session.auth_state.change_number_phone.as_deref(),
        Some("+15550199")
    );
    assert_eq!(
        session.auth_state.change_number_error.as_deref(),
        Some("Could not check the verification code: Telegram refused the request")
    );
}

#[test]
fn websites_answer_replaces_cache_for_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let extra = session.request(RequestPurpose::GetConnectedWebsites, None);
    session.settings.connected_websites_loading = true;
    session.settings.websites_error = Some("stale".into());
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"connectedWebsites","@extra":"{}","websites":[{{"@type":"connectedWebsite","id":55,"domain_name":"example.com","bot_user_id":77,"browser":"Chrome","platform":"Web","log_in_date":1758900000,"last_active_date":1759000000,"ip_address":"9.9.9.9","location":"Boston, United States"}}]}}"#,
            extra.0,
        ),
    );
    let websites = session
        .settings
        .connected_websites
        .as_ref()
        .expect("cached");
    assert_eq!(websites.len(), 1);
    let site = &websites[0];
    assert_eq!(site.id, 55);
    assert_eq!(site.domain_name, "example.com");
    assert_eq!(site.bot_user_id, 77);
    assert_eq!(site.browser, "Chrome");
    assert_eq!(site.platform, "Web");
    assert_eq!(site.log_in_date, 1758900000);
    assert_eq!(site.ip_address, "9.9.9.9");
    assert_eq!(site.location, "Boston, United States");
    assert!(!session.settings.connected_websites_loading);
    assert!(session.settings.websites_error.is_none());
    assert!(!session.settings.websites_stale);
}

#[test]
fn websites_answer_ignored_without_matching_request() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.connected_websites = Some(Vec::new());
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"connectedWebsites","websites":[{"@type":"connectedWebsite","id":55,"domain_name":"example.com"}]}"#,
    );
    assert_eq!(
        session.settings.connected_websites.as_ref().unwrap().len(),
        0
    );
}

#[test]
fn disconnect_website_ok_keeps_cache_and_marks_stale() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.connected_websites = Some(vec![ParsedWebsite {
        id: 55,
        domain_name: "example.com".into(),
        bot_user_id: 77,
        browser: "Chrome".into(),
        platform: "Web".into(),
        log_in_date: 1758900000,
        last_active_date: 1759000000,
        ip_address: "9.9.9.9".into(),
        location: "Boston, United States".into(),
    }]);
    session.settings.websites_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::DisconnectWebsite { website_id: 55 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    // The old cache stays visible until the authoritative refetch
    // replaces it — no optimistic deletion.
    assert_eq!(
        session.settings.connected_websites.as_ref().unwrap().len(),
        1
    );
    assert!(session.settings.websites_stale);
    assert!(!session.settings.websites_mutating);
    assert!(session.settings.websites_error.is_none());
}

#[test]
fn disconnect_all_websites_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.websites_mutating = true;
    let extra = session.request(RequestPurpose::DisconnectAllWebsites, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"WEBSITE_NOT_FOUND"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.websites_mutating);
    assert_eq!(
        session.settings.websites_error.as_deref(),
        Some("Could not disconnect the website: Telegram refused the request")
    );
}

#[test]
fn websites_fetch_error_clears_loading() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.connected_websites_loading = true;
    // A failed stale-refetch (e.g. after a disconnect-ok marked the
    // cache stale) must NOT leave the cache stale — otherwise the
    // next ingest retries the fetch and flood state worsens.
    session.settings.websites_stale = true;
    let extra = session.request(RequestPurpose::GetConnectedWebsites, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"FLOOD_WAIT_3"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.connected_websites_loading);
    // The failed stale-refetch clears staleness: no auto-retry on
    // the next ingest; the user retries via the Refresh button.
    assert!(!session.settings.websites_stale);
    assert_eq!(
        session.settings.websites_error.as_deref(),
        Some("Could not load the websites list: try again in 3 seconds")
    );
}

#[test]
fn toggle_session_calls_ok_marks_sessions_stale() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions = Some(vec![ParsedSession {
        id: 22,
        is_current: false,
        is_password_pending: false,
        is_unconfirmed: false,
        can_accept_secret_chats: true,
        can_accept_calls: true,
        device_model: "iPhone".into(),
        application_name: "Telegram iOS".into(),
        application_version: "12.0".into(),
        platform: "iOS".into(),
        system_version: "18.0".into(),
        last_active_date: 1758900000,
        ip_address: "5.6.7.8".into(),
        location: "Tel Aviv, Israel".into(),
        ..Default::default()
    }]);
    session.settings.sessions_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::ToggleSessionCalls { session_id: 22 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    // No optimistic flip: the cached flag is untouched until the
    // authoritative refetch replaces the list.
    let sessions = session.settings.sessions.as_ref().unwrap();
    assert!(sessions[0].can_accept_calls);
    assert!(session.settings.sessions_stale);
    assert!(!session.settings.sessions_mutating);
    assert!(session.settings.sessions_error.is_none());
}

#[test]
fn toggle_session_secret_chats_error_surfaces_honestly() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    session.settings.sessions_mutating = true;
    let extra = session.request(
        RequestPurpose::Settings(SettingsPurpose::ToggleSessionSecretChats { session_id: 22 }),
        None,
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"SESSION_INVALID"}}"#,
            extra.0
        ),
    );
    assert!(!session.settings.sessions_mutating);
    assert_eq!(
        session.settings.sessions_error.as_deref(),
        Some("Could not change the session setting: Telegram refused the request")
    );
}

// Slice parity:platform-offline-errors — `is_offline` is false for
// Ready and Updating (live sync); true for WaitingForNetwork /
// Connecting / etc.
#[test]
fn is_offline_follows_connection_state() {
    let (mut session, _sink) = session();
    session.connection = ConnectionState::Ready;
    assert!(!session.is_offline());
    session.connection = ConnectionState::Updating;
    assert!(!session.is_offline());
    session.connection = ConnectionState::WaitingForNetwork;
    assert!(session.is_offline());
    session.connection = ConnectionState::Connecting;
    assert!(session.is_offline());
}

// Rich-text premium gate: `my_is_premium` reflects our own user's
// `user.is_premium` (schema 1.8.67 line 2403); false until our user
// object is cached.
#[test]
fn my_is_premium_follows_own_user_record() {
    let (mut session, _sink) = session();
    assert!(!session.my_is_premium());
    session.my_user_id = Some(31);
    assert!(!session.my_is_premium());
    session.users.insert(
        31,
        ParsedUser {
            id: 31,
            is_premium: true,
            verification: Default::default(),
            emoji_status_id: 0,
            ..Default::default()
        },
    );
    assert!(session.my_is_premium());
}
