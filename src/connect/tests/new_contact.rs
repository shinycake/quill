//! The New Contact box (Window > Add Contact): `importContacts` with one
//! phone contact, then the chat opens or the box says "not joined".
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::state::NewContactOutcome;
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

fn cleanup(f: Fixture) {
    let (dir, driver, ..) = f;
    drop(driver);
    std::fs::remove_dir_all(dir).unwrap();
}

fn ingest(f: &mut Fixture, json: &str) {
    let owned = copy_and_parse(json, &f.4, &f.3).unwrap();
    f.1.ingest(owned).unwrap();
}

fn last_request(f: &Fixture) -> Value {
    serde_json::from_str(f.2.snapshot().last().unwrap()).unwrap()
}

fn extra_of(request: &Value) -> String {
    request["@extra"].as_str().unwrap().to_string()
}

#[test]
fn a_phone_on_telegram_imports_the_contact_and_opens_the_chat() {
    let mut f = fixture();
    f.1.add_contact_by_phone("+1 555 010 0199", "Ada", "Lovelace")
        .unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "importContacts");
    let contact = &request["contacts"][0];
    assert_eq!(contact["phone_number"], "+1 555 010 0199");
    assert_eq!(contact["first_name"], "Ada");
    assert_eq!(contact["last_name"], "Lovelace");
    // A second Create while the first is in flight sends nothing.
    let sent = f.2.snapshot().len();
    assert_eq!(f.1.add_contact_by_phone("+1", "A", "").unwrap(), None);
    assert_eq!(f.2.snapshot().len(), sent);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"importedContacts","@extra":"{}","user_ids":[777],"importer_count":[0]}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session
            .users_state
            .new_contact_outcome
            .clone()
            .map(|(_, outcome)| outcome),
        Some(NewContactOutcome::Joined(777))
    );
    let open = last_request(&f);
    assert_eq!(open["@type"], "createPrivateChat");
    assert_eq!(open["user_id"], 777);
    cleanup(f);
}

#[test]
fn a_phone_not_on_telegram_reports_not_joined_and_opens_nothing() {
    let mut f = fixture();
    f.1.add_contact_by_phone("+15550100000", "Grace", "")
        .unwrap();
    let request = last_request(&f);
    let sent = f.2.snapshot().len();
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"importedContacts","@extra":"{}","user_ids":[0],"importer_count":[3]}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session
            .users_state
            .new_contact_outcome
            .clone()
            .map(|(_, outcome)| outcome),
        Some(NewContactOutcome::NotJoined)
    );
    assert!(f.1.session.users_state.user_chat_to_open.is_none());
    assert_eq!(f.2.snapshot().len(), sent);
    cleanup(f);
}

#[test]
fn a_failed_import_is_reported_to_the_box() {
    let mut f = fixture();
    f.1.add_contact_by_phone("+15550100000", "Grace", "")
        .unwrap();
    let request = last_request(&f);
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PHONE_NUMBER_INVALID"}}"#,
            extra_of(&request)
        ),
    );
    assert_eq!(
        f.1.session
            .users_state
            .new_contact_outcome
            .clone()
            .map(|(_, outcome)| outcome),
        Some(NewContactOutcome::Failed(400))
    );
    cleanup(f);
}

/// Esc before the answer, then a new box: its Create goes out, and the
/// first import's answer opens no chat behind it.
#[test]
fn a_closed_boxs_import_opens_nothing_and_the_next_box_sends() {
    let mut f = fixture();
    f.1.add_contact_by_phone("+15550100001", "Ada", "").unwrap();
    let first = last_request(&f);
    f.1.forget_new_contact();
    let second =
        f.1.add_contact_by_phone("+15550100002", "Grace", "")
            .unwrap();
    assert!(second.is_some(), "the next box's Create is sent");
    let sent = f.2.snapshot().len();
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"importedContacts","@extra":"{}","user_ids":[777],"importer_count":[0]}}"#,
            extra_of(&first)
        ),
    );
    assert!(f.1.session.users_state.user_chat_to_open.is_none());
    assert!(f.1.session.users_state.new_contact_outcome.is_none());
    assert_eq!(f.2.snapshot().len(), sent, "no createPrivateChat");
    assert_eq!(f.1.session.users_state.new_contact_request, second);
    cleanup(f);
}

/// The Contacts tab's list, invalidated by a contact change, is fetched
/// again; one that failed waits for Retry.
#[test]
fn an_invalidated_contact_list_is_fetched_again() {
    let mut f = fixture();
    f.1.session.users_state.contacts = None;
    f.1.session.users_state.contacts_error = false;
    f.1.refetch_invalidated_contacts();
    assert_eq!(last_request(&f)["@type"], "getContacts");
    let sent = f.2.snapshot().len();
    f.1.refetch_invalidated_contacts();
    assert_eq!(f.2.snapshot().len(), sent, "one fetch in flight");
    cleanup(f);
}
