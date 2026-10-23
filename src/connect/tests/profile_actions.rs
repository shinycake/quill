//! What the profile's buttons and More menu need from the driver: Disable
//! Sharing (`toggleChatHasProtectedContent` in a private chat, applied at
//! once and rolled back on an error), `createPrivateChat` for a person
//! whose profile opened without a chat, and a group's location and gift
//! flag from `supergroupFullInfo`.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
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

const ADA: &str = r#"{"@type":"updateUser","user":{"id":31,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#;
const ADA_CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":31},"unread_count":0}}"#;

#[test]
fn disabling_sharing_in_a_private_chat_applies_at_once_and_rolls_back() {
    let mut f = fixture();
    ingest(&mut f, ADA);
    ingest(&mut f, ADA_CHAT);
    assert_eq!(
        f.1.session.private_sharing_disabled(ChatId(31)),
        Some(false)
    );
    f.1.set_chat_protected_content(ChatId(31), true)
        .unwrap()
        .unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "toggleChatHasProtectedContent");
    assert_eq!(request["chat_id"], 31);
    assert_eq!(request["has_protected_content"], true);
    assert_eq!(f.1.session.private_sharing_disabled(ChatId(31)), Some(true));
    ingest(
        &mut f,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"PREMIUM_ACCOUNT_REQUIRED"}}"#,
            request["@extra"].as_str().unwrap()
        ),
    );
    assert_eq!(
        f.1.session.private_sharing_disabled(ChatId(31)),
        Some(false)
    );
    cleanup(f);
}

#[test]
fn bots_cannot_have_sharing_disabled() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateUser","user":{"id":77,"first_name":"Go","type":{"@type":"userTypeBot","is_inline":false}}}"#,
    );
    ingest(
        &mut f,
        r#"{"@type":"updateNewChat","chat":{"id":77,"title":"Go","type":{"@type":"chatTypePrivate","user_id":77},"unread_count":0}}"#,
    );
    assert_eq!(f.1.session.private_sharing_disabled(ChatId(77)), None);
    let sent = f.2.snapshot().len();
    assert_eq!(
        f.1.set_chat_protected_content(ChatId(77), true).unwrap(),
        None
    );
    assert_eq!(f.2.snapshot().len(), sent);
    cleanup(f);
}

#[test]
fn a_person_without_a_chat_gets_one_created_once() {
    let mut f = fixture();
    ingest(&mut f, ADA);
    f.1.ensure_private_chat(31).unwrap().unwrap();
    let request = last_request(&f);
    assert_eq!(request["@type"], "createPrivateChat");
    assert_eq!(request["user_id"], 31);
    assert_eq!(f.1.ensure_private_chat(31).unwrap(), None);
    ingest(&mut f, ADA_CHAT);
    assert_eq!(f.1.ensure_private_chat(31).unwrap(), None);
    cleanup(f);
}

#[test]
fn a_location_based_group_keeps_its_address_and_gift_flag() {
    let mut f = fixture();
    ingest(
        &mut f,
        r#"{"@type":"updateSupergroupFullInfo","supergroup_id":2,"supergroup_full_info":{"description":"","can_send_gift":true,"location":{"@type":"chatLocation","location":{"latitude":52.52,"longitude":13.405},"address":"Alexanderplatz, Berlin"}}}"#,
    );
    let profile = &f.1.session.supergroup_full_info(2).unwrap().profile;
    assert!(profile.can_send_gift);
    assert_eq!(
        profile.location.as_ref().map(|l| l.address.as_str()),
        Some("Alexanderplatz, Berlin")
    );
    cleanup(f);
}
