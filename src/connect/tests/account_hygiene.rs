//! Batch 4 / 6 driver tests: presence, new-login alert, terms of
//! service, storage clearing and limits, two-step recovery. TDLib JSON
//! is recorded from the 1.8.67 schema shapes.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::state::{LoginReview, PasswordOp, RequestPurpose, TwofaNotice};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Fixture = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

fn fixture() -> Fixture {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    (dir, driver, recorder, sink, seq)
}

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn last_request(recorder: &RecordingSender) -> Value {
    serde_json::from_str(recorder.snapshot().last().unwrap()).unwrap()
}

fn requests_of(recorder: &RecordingSender, ty: &str) -> Vec<Value> {
    recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|value| value["@type"] == ty)
        .collect()
}

const PASSWORD_STATE_ON: &str = r#"{"@type":"passwordState","has_password":true,"password_hint":"street","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"i***@example.com","pending_reset_date":0,"@extra":"EXTRA"}"#;

fn seed_password_state(f: &mut Fixture, json: &str) {
    let extra = f.1.fetch_password_state().unwrap().unwrap();
    let json = json.replace("EXTRA", &extra.0.to_string());
    let parsed = copy_and_parse(&json, &f.4, &f.3).unwrap();
    f.1.ingest(parsed).unwrap();
}

#[test]
fn online_option_is_set_with_a_boolean_value() {
    let mut f = fixture();
    let extra = f.1.set_online(true).unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "setOption");
    assert_eq!(request["name"], "online");
    assert_eq!(request["value"]["@type"], "optionValueBoolean");
    assert_eq!(request["value"]["value"], true);
    assert_eq!(request["@extra"], extra.0.to_string());
    // The `ok` is consumed without touching anything else.
    let ok = format!(r#"{{"@type":"ok","@extra":"{}"}}"#, extra.0);
    let parsed = copy_and_parse(&ok, &f.4, &f.3).unwrap();
    f.1.ingest(parsed).unwrap();
    assert!(f.1.session.requests.purpose(extra).is_none());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn online_is_not_sent_before_authorization() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = crate::state::Session::new(crate::ids::AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
    assert_invalid(driver.set_online(true));
    std::fs::remove_dir_all(dir).unwrap();
}

const UNCONFIRMED_UPDATE: &str = r#"{"@type":"updateUnconfirmedSession","session":{"@type":"unconfirmedSession","type":{"@type":"sessionTypeAndroid"},"date":1760000000,"device_model":"Pixel 9","location":"Berlin, Germany"},"unconfirmed_session_count":1}"#;

fn sessions_answer(extra: u64) -> String {
    format!(
        r#"{{"@type":"sessions","sessions":[{{"@type":"session","id":"1","is_current":true,"is_password_pending":false,"is_unconfirmed":false,"can_accept_secret_chats":true,"can_accept_calls":true,"api_id":1,"application_name":"Quill","application_version":"1","is_official_application":false,"device_model":"Mac","platform":"macOS","system_version":"15","log_in_date":1,"last_active_date":2,"ip_address":"1.1.1.1","location":"Home"}},{{"@type":"session","id":"77","is_current":false,"is_password_pending":false,"is_unconfirmed":true,"can_accept_secret_chats":true,"can_accept_calls":true,"api_id":2,"application_name":"Telegram Android","application_version":"11","is_official_application":true,"device_model":"Pixel 9","platform":"Android","system_version":"16","log_in_date":1760000000,"last_active_date":1760000000,"ip_address":"2.2.2.2","location":"Berlin, Germany"}}],"inactive_session_ttl_days":180,"@extra":"{extra}"}}"#
    )
}

/// An unconfirmed login resolves its session id through the sessions
/// list, and the user's answer confirms or terminates exactly that session.
fn unconfirmed_alert(f: &mut Fixture) {
    ingest(&mut f.1, &f.4, &f.3, UNCONFIRMED_UPDATE);
    assert_eq!(f.1.session.notices.unconfirmed_count, 1);
    // The update marked the list stale: the same ingest refetched it.
    let fetch = requests_of(&f.2, "getActiveSessions");
    assert_eq!(fetch.len(), 1);
    let extra: u64 = fetch[0]["@extra"].as_str().unwrap().parse().unwrap();
    ingest(&mut f.1, &f.4, &f.3, &sessions_answer(extra));
    let entries = &f.1.session.notices.unconfirmed_entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, 77);
    assert_eq!(entries[0].device, "Pixel 9");
}

#[test]
fn new_login_yes_confirms_the_unconfirmed_session() {
    let mut f = fixture();
    unconfirmed_alert(&mut f);
    f.1.review_unconfirmed_sessions(true).unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "confirmSession");
    assert_eq!(request["session_id"], 77);
    // Not twice while the answer is pending.
    assert_invalid(f.1.review_unconfirmed_sessions(true));
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    assert_eq!(
        f.1.session.notices.review_outcome,
        Some(LoginReview::Allowed)
    );
    assert_eq!(f.1.session.notices.unconfirmed_count, 0);
    assert!(f.1.session.notices.unconfirmed_entries.is_empty());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn new_login_no_terminates_and_names_the_attempt() {
    let mut f = fixture();
    unconfirmed_alert(&mut f);
    f.1.review_unconfirmed_sessions(false).unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "terminateSession");
    assert_eq!(request["session_id"], 77);
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    assert_eq!(
        f.1.session.notices.review_outcome,
        Some(LoginReview::Prevented {
            places: vec!["Berlin, Germany (Pixel 9)".into()]
        })
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn new_login_review_error_keeps_the_alert() {
    let mut f = fixture();
    unconfirmed_alert(&mut f);
    f.1.review_unconfirmed_sessions(true).unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(r#"{{"@type":"error","code":400,"message":"SESSION_NOT_FOUND","@extra":"{extra}"}}"#),
    );
    assert!(f.1.session.notices.review_error.is_some());
    assert!(f.1.session.notices.review_outcome.is_none());
    assert_eq!(f.1.session.notices.unconfirmed_count, 1);
    assert_eq!(f.1.session.notices.review_pending, 0);
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn unconfirmed_update_with_none_left_clears_the_alert() {
    let mut f = fixture();
    unconfirmed_alert(&mut f);
    ingest(&mut f.1, &f.4, &f.3,
        r#"{"@type":"updateUnconfirmedSession","session":null,"unconfirmed_session_count":0}"#,
    );
    assert_eq!(f.1.session.notices.unconfirmed_count, 0);
    assert!(f.1.session.notices.unconfirmed.is_none());
    assert!(f.1.session.notices.unconfirmed_entries.is_empty());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn service_notifications_queue_in_order_and_skip_withdrawal_ones() {
    let mut f = fixture();
    for (kind, text) in [
        ("", "First"),
        ("API_WITHDRAWAL_FEATURE_DISABLED_X", "Hidden"),
        ("", "Second"),
    ] {
        ingest(&mut f.1, &f.4, &f.3,
            &format!(
                r#"{{"@type":"updateServiceNotification","type":"{kind}","content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
            ),
        );
    }
    let queue: Vec<&str> = f
        .1
        .session
        .notices
        .service
        .iter()
        .map(|n| n.text.as_str())
        .collect();
    assert_eq!(queue, ["First", "Second"]);
    f.1.session.dismiss_service_notice();
    assert_eq!(f.1.session.notices.service.front().unwrap().text, "Second");
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn terms_of_service_accept_round_trip() {
    let mut f = fixture();
    assert_invalid(f.1.accept_terms());
    ingest(&mut f.1, &f.4, &f.3,
        r#"{"@type":"updateTermsOfService","terms_of_service_id":"tos-1","terms_of_service":{"@type":"termsOfService","text":{"@type":"formattedText","text":"Be nice.","entities":[]},"min_user_age":0,"show_popup":true}}"#,
    );
    assert_eq!(f.1.session.notices.terms.as_ref().unwrap().id, "tos-1");
    f.1.accept_terms().unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "acceptTermsOfService");
    assert_eq!(request["terms_of_service_id"], "tos-1");
    assert!(f.1.session.notices.terms_in_flight);
    assert_invalid(f.1.accept_terms());
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    assert!(f.1.session.notices.terms.is_none());
    assert!(!f.1.session.notices.terms_in_flight);
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn terms_accept_failure_keeps_the_prompt_with_an_error() {
    let mut f = fixture();
    ingest(&mut f.1, &f.4, &f.3,
        r#"{"@type":"updateTermsOfService","terms_of_service_id":"tos-1","terms_of_service":{"@type":"termsOfService","text":{"@type":"formattedText","text":"Be nice.","entities":[]},"min_user_age":18,"show_popup":true}}"#,
    );
    f.1.accept_terms().unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(r#"{{"@type":"error","code":500,"message":"x","@extra":"{extra}"}}"#),
    );
    assert!(f.1.session.notices.terms.is_some());
    assert!(!f.1.session.notices.terms_in_flight);
    assert!(f.1.session.notices.terms_error.is_some());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn clear_storage_sends_optimize_storage_and_reports_freed_bytes() {
    let mut f = fixture();
    f.1.clear_storage(&["fileTypePhoto", "fileTypeVideo"], &[])
        .unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "optimizeStorage");
    assert_eq!(request["size"], 0);
    assert_eq!(request["ttl"], 0);
    assert_eq!(request["count"], 0);
    assert_eq!(request["immunity_delay"], 0);
    assert_eq!(request["file_types"][0]["@type"], "fileTypePhoto");
    assert_eq!(request["file_types"][1]["@type"], "fileTypeVideo");
    assert_eq!(request["chat_ids"], serde_json::json!([]));
    assert_eq!(request["return_deleted_file_statistics"], true);
    assert!(f.1.session.storage_clearing);
    // One clear at a time.
    assert_invalid(f.1.clear_storage(&[], &[]));
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"storageStatistics","size":"5242880","count":3,"by_chat":[],"@extra":"{extra}"}}"#
        ),
    );
    assert_eq!(f.1.session.storage_freed, Some(5_242_880));
    assert!(!f.1.session.storage_clearing);
    // The usage numbers are refetched on the same ingest.
    assert!(f.1.session.storage_stats_loading);
    assert_eq!(requests_of(&f.2, "getStorageStatistics").len(), 1);
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn clear_storage_for_one_chat_names_the_chat() {
    let mut f = fixture();
    f.1.clear_storage(&[], &[4242]).unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["chat_ids"], serde_json::json!([4242]));
    assert_eq!(request["file_types"], serde_json::json!([]));
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn clear_storage_failure_reports_and_frees_the_gate() {
    let mut f = fixture();
    f.1.clear_storage(&[], &[]).unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(r#"{{"@type":"error","code":500,"message":"x","@extra":"{extra}"}}"#),
    );
    assert!(!f.1.session.storage_clearing);
    assert!(f.1.session.data_storage_error.is_some());
    assert!(f.1.session.storage_freed.is_none());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn storage_limits_send_the_four_options_and_read_them_back() {
    let mut f = fixture();
    f.1.set_storage_limits(Some(2 * 1024 * 1024 * 1024), Some(31 * 86_400))
        .unwrap();
    let sent: Vec<Value> = requests_of(&f.2, "setOption");
    let names: Vec<&str> = sent.iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "storage_max_files_size",
            "storage_max_time_from_last_access",
            "storage_max_file_count",
            "use_storage_optimizer"
        ]
    );
    // 2 GiB in KiB, as an int64 string.
    assert_eq!(sent[0]["value"]["@type"], "optionValueInteger");
    assert_eq!(sent[0]["value"]["value"], "2097152");
    assert_eq!(sent[1]["value"]["value"], (31 * 86_400).to_string());
    assert_eq!(sent[3]["value"]["@type"], "optionValueBoolean");
    assert_eq!(sent[3]["value"]["value"], true);
    // TDLib echoes each option as `updateOption`.
    for (name, value) in [
        (
            "storage_max_files_size",
            r#"{"@type":"optionValueInteger","value":"2097152"}"#,
        ),
        (
            "storage_max_time_from_last_access",
            r#"{"@type":"optionValueInteger","value":"2678400"}"#,
        ),
        (
            "use_storage_optimizer",
            r#"{"@type":"optionValueBoolean","value":true}"#,
        ),
    ] {
        ingest(&mut f.1, &f.4, &f.3,
            &format!(r#"{{"@type":"updateOption","name":"{name}","value":{value}}}"#),
        );
    }
    let limits = f.1.session.storage_limits;
    assert_eq!(limits.size_limit(), Some(2 * 1024 * 1024 * 1024));
    assert_eq!(limits.keep_for(), Some(31 * 86_400));
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn recovery_email_code_confirms_the_pending_address() {
    let mut f = fixture();
    // Nothing pending: the code cannot be sent.
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    assert_invalid(f.1.check_recovery_email_code("123456"));
    let extra = f.1.set_recovery_email("pw", "new@example.com").unwrap();
    let pending = format!(
        r#"{{"@type":"passwordState","has_password":true,"password_hint":"","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
        extra.0
    );
    ingest(&mut f.1, &f.4, &f.3, &pending);
    assert_invalid(f.1.check_recovery_email_code(""));
    f.1.check_recovery_email_code("123456").unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "checkRecoveryEmailAddressCode");
    assert_eq!(request["code"], "123456");
    let extra = request["@extra"].as_str().unwrap().to_string();
    let done = PASSWORD_STATE_ON.replace("EXTRA", &extra);
    ingest(&mut f.1, &f.4, &f.3, &done);
    assert_eq!(f.1.session.password_state.as_ref().unwrap().pending_email_pattern, None);
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::RecoveryEmailConfirmed)
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn wrong_recovery_email_code_reports_without_changing_state() {
    let mut f = fixture();
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    let extra = f.1.set_recovery_email("pw", "new@example.com").unwrap();
    let pending = format!(
        r#"{{"@type":"passwordState","has_password":true,"password_hint":"","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
        extra.0
    );
    ingest(&mut f.1, &f.4, &f.3, &pending);
    f.1.check_recovery_email_code("000000").unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(r#"{{"@type":"error","code":400,"message":"CODE_INVALID","@extra":"{extra}"}}"#),
    );
    let line = f.1.session.password_op_error.clone().unwrap();
    assert!(line.contains("confirm the recovery email"), "{line}");
    assert!(line.contains("wrong or has expired"), "{line}");
    assert!(f.1.session.password_state.as_ref().unwrap().pending_email_pattern.is_some());
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn forgot_password_requests_a_code_then_recovers_with_a_new_password() {
    let mut f = fixture();
    // Needs the password state with a recovery email.
    assert_invalid(f.1.request_twofa_recovery_code());
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    f.1.request_twofa_recovery_code().unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "requestPasswordRecovery");
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"i***@example.com","length":6,"@extra":"{extra}"}}"#
        ),
    );
    assert_eq!(
        f.1.session.twofa_flow.recovery_code_sent_to.as_deref(),
        Some("i***@example.com")
    );
    assert!(!f.1.session.password_state_loading);

    assert_invalid(f.1.recover_twofa_password("", "new", "hint"));
    f.1.recover_twofa_password("654321", "newpw", "new hint")
        .unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "recoverPassword");
    assert_eq!(request["recovery_code"], "654321");
    assert_eq!(request["new_password"], "newpw");
    assert_eq!(request["new_hint"], "new hint");
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &PASSWORD_STATE_ON.replace("EXTRA", &extra));
    assert!(f.1.session.twofa_flow.recovery_code_sent_to.is_none());
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::PasswordRecovered)
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn recovering_with_an_empty_password_reports_removal() {
    let mut f = fixture();
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    f.1.recover_twofa_password("654321", "", "").unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    let removed = format!(
        r#"{{"@type":"passwordState","has_password":false,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{extra}"}}"#
    );
    ingest(&mut f.1, &f.4, &f.3, &removed);
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::PasswordRemoved)
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn reset_password_pending_refetches_the_state_for_the_date() {
    let mut f = fixture();
    assert_invalid(f.1.reset_twofa_password());
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    f.1.reset_twofa_password().unwrap();
    assert_eq!(last_request(&f.2)["@type"], "resetPassword");
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"resetPasswordResultPending","pending_reset_date":1760604800,"@extra":"{extra}"}}"#
        ),
    );
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::ResetPending {
            reset_date: 1_760_604_800
        })
    );
    // The new date lives in the password state: refetched on the same ingest.
    assert_eq!(requests_of(&f.2, "getPasswordState").len(), 2);
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn reset_password_declined_reports_the_retry_date() {
    let mut f = fixture();
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    f.1.reset_twofa_password().unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"resetPasswordResultDeclined","retry_date":1760700000,"@extra":"{extra}"}}"#
        ),
    );
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::ResetDeclined {
            retry_date: 1_760_700_000
        })
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn cancel_password_reset_needs_a_pending_reset() {
    let mut f = fixture();
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    assert_invalid(f.1.cancel_twofa_password_reset());
    let pending_json = PASSWORD_STATE_ON.replace(r#""pending_reset_date":0"#, r#""pending_reset_date":1760604800"#);
    f.1.refresh_password_state().unwrap();
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &pending_json.replace("EXTRA", &extra));
    f.1.cancel_twofa_password_reset().unwrap();
    assert_eq!(last_request(&f.2)["@type"], "cancelPasswordReset");
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::ResetCancelled)
    );
    assert_eq!(requests_of(&f.2, "getPasswordState").len(), 3);
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn login_email_change_is_code_confirmed() {
    let mut f = fixture();
    seed_password_state(&mut f, PASSWORD_STATE_ON);
    assert_invalid(f.1.set_login_email(" "));
    assert_invalid(f.1.check_login_email_code("123456"));
    f.1.set_login_email(" me@example.com ").unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "setLoginEmailAddress");
    assert_eq!(request["new_login_email_address"], "me@example.com");
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"m***@example.com","length":5,"@extra":"{extra}"}}"#
        ),
    );
    assert_eq!(
        f.1.session.twofa_flow.login_email_code_sent_to.as_deref(),
        Some("m***@example.com")
    );
    f.1.resend_login_email_code().unwrap();
    assert_eq!(last_request(&f.2)["@type"], "resendLoginEmailAddressCode");
    let extra = last_request(&f.2)["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3,
        &format!(
            r#"{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"m***@example.com","length":5,"@extra":"{extra}"}}"#
        ),
    );
    f.1.check_login_email_code("12345").unwrap();
    let request = last_request(&f.2);
    assert_eq!(request["@type"], "checkLoginEmailAddressCode");
    assert_eq!(request["code"]["@type"], "emailAddressAuthenticationCode");
    assert_eq!(request["code"]["code"], "12345");
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(&mut f.1, &f.4, &f.3, &format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#));
    assert!(f.1.session.twofa_flow.login_email_code_sent_to.is_none());
    assert_eq!(
        f.1.session.twofa_flow.notice,
        Some(TwofaNotice::LoginEmailChanged)
    );
    std::fs::remove_dir_all(&f.0).unwrap();
}

#[test]
fn password_ops_carry_distinct_error_lines() {
    for (op, expect) in [
        (PasswordOp::RequestRecoveryCode, "send the recovery code"),
        (PasswordOp::ResetPassword, "reset the password"),
        (PasswordOp::SetLoginEmail, "change the login email"),
    ] {
        assert!(op.action_label().contains(expect));
    }
    let _ = RequestPurpose::PasswordStateOp {
        op: PasswordOp::ResetPassword,
    };
}
