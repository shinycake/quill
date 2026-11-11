//! Sweep s4-accounts-2: answers that move a stage, matched to what asked.
use super::common::*;
use super::*;

const WAIT_PASSWORD: &str = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"","has_recovery_email_address":true}}"#;

fn ask_recovery(session: &mut Session) -> RequestId {
    let extra = session.request(RequestPurpose::RequestAuthenticationPasswordRecovery, None);
    session.auth_state.password_recovery = PasswordRecovery::Requested(extra);
    extra
}

#[test]
fn the_recovery_code_step_opens_only_when_the_server_accepts() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, WAIT_PASSWORD);
    let extra = ask_recovery(&mut session);
    assert_eq!(
        session.auth_state.password_recovery,
        PasswordRecovery::Requested(extra)
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0),
    );
    assert_eq!(
        session.auth_state.password_recovery,
        PasswordRecovery::CodeSent
    );
}

#[test]
fn a_refused_recovery_request_stays_on_the_password_step() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, WAIT_PASSWORD);
    let extra = ask_recovery(&mut session);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"Too Many Requests: retry after 5"}}"#,
            extra.0
        ),
    );
    assert_eq!(session.auth_state.password_recovery, PasswordRecovery::Idle);
    assert!(session.auth_state.last_auth_error.is_some());
}

#[test]
fn leaving_the_password_step_drops_the_recovery_stage() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    apply_json(&mut session, &seq, &sink, WAIT_PASSWORD);
    session.auth_state.password_recovery = PasswordRecovery::CodeSent;
    apply_json(
        &mut session,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
    );
    assert_eq!(session.auth_state.password_recovery, PasswordRecovery::Idle);
}

#[test]
fn an_auto_download_save_is_answered_by_the_request_that_asked() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    let network = crate::data_settings::NetworkKind::WiFi;
    let settings = *session.settings.data_storage.for_network(network);
    let purpose =
        RequestPurpose::Settings(SettingsPurpose::SetAutoDownloadSettings { network, settings });
    let first = session.request(purpose, None);
    let second = session.request(purpose, None);
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"BAD"}}"#,
            first.0
        ),
    );
    assert_eq!(
        session.settings.auto_download_answer,
        Some(AutoDownloadAnswer::Refused(first))
    );
    apply_json(
        &mut session,
        &seq,
        &sink,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, second.0),
    );
    assert_eq!(
        session.settings.auto_download_answer,
        Some(AutoDownloadAnswer::Saved(second))
    );
}
