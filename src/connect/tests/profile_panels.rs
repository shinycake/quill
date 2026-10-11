//! Connect-driver tests: B10 profile and contact panels.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::{ProfileChatsFetch, ProfileChatsKind};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    json: &str,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

#[test]
fn share_contact_sends_input_message_contact_for_a_known_phone() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Target","type":{"@type":"chatTypePrivate","user_id":7}}}"#,
        &seq,
        &sink,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","last_name":"Lovelace","phone_number":"15550131","type":{"@type":"userTypeRegular"}}}"#,
        &seq,
        &sink,
    );
    driver
        .send_contact_message(ChatId(7), 31)
        .expect("contact sent");
    let sent = recorder.snapshot();
    let send = sent
        .iter()
        .find(|j| j.contains(r#""@type":"inputMessageContact""#))
        .expect("inputMessageContact in outbox");
    assert!(send.contains(r#""phone_number":"15550131""#));
    assert!(send.contains(r#""user_id":31"#));
    // A user whose phone is hidden has nothing to share.
    ingest(
        &mut driver,
        r#"{"@type":"updateUser","user":{"id":32,"first_name":"Hidden","last_name":"","phone_number":"","type":{"@type":"userTypeRegular"}}}"#,
        &seq,
        &sink,
    );
    assert!(driver.send_contact_message(ChatId(7), 32).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn profile_lists_are_deduped_and_failures_retry() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let extra = driver
        .fetch_groups_in_common(31)
        .unwrap()
        .expect("first fetch sends");
    // In flight: no second request.
    assert!(driver.fetch_groups_in_common(31).unwrap().is_none());
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"X"}}"#,
            extra.0
        ),
        &seq,
        &sink,
    );
    assert!(matches!(
        driver
            .session
            .users_state
            .profile_chat_lists
            .get(&(ProfileChatsKind::GroupsInCommon, 31)),
        Some(ProfileChatsFetch::Failed(_))
    ));
    // Failed lists are retried.
    assert!(driver.fetch_groups_in_common(31).unwrap().is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn main_profile_tab_request_goes_out() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    assert!(
        driver
            .set_main_profile_tab(crate::profile_tab::ProfileTab::Files)
            .is_ok()
    );
    let sent = recorder.snapshot();
    assert_eq!(
        sent.iter()
            .filter(
                |j| j.contains(r#""@type":"setMainProfileTab""#) && j.contains("profileTabFiles")
            )
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn birthdate_is_validated_before_sending() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    assert!(driver.set_birthdate(Some((32, 1, None))).is_err());
    assert!(driver.set_birthdate(Some((1, 13, None))).is_err());
    assert!(driver.set_birthdate(Some((9, 10, Some(1990)))).is_ok());
    assert!(driver.set_birthdate(None).is_ok());
    let sent = recorder.snapshot();
    assert_eq!(
        sent.iter()
            .filter(|j| j.contains(r#""@type":"setBirthdate""#))
            .count(),
        2
    );
    let _ = std::fs::remove_dir_all(dir);
}
