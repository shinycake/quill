//! A `logOut` that cannot go out changes nothing: the account keeps its
//! requests, its generation and its shutdown phase. One that goes out
//! settles the rest and keeps its own request for TDLib's answer.
use super::super::*;
use super::flood_retry_lost::SwitchSender;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

fn ready_driver(down: Arc<AtomicBool>) -> ConnectDriver<SwitchSender> {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let sender = SwitchSender {
        inner: Arc::new(RecordingSender::new()),
        down,
    };
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver
}

#[test]
fn a_logout_that_cannot_go_out_leaves_the_account_as_it_was() {
    let down = Arc::new(AtomicBool::new(false));
    let mut driver = ready_driver(down.clone());
    let send = driver
        .session
        .request(RequestPurpose::SendMessage, Some(ChatId(5)));
    let chats = driver.session.request(RequestPurpose::LoadChats, None);
    let generation = driver.session.account_generation;
    let shutdown = driver.session.shutdown;

    down.store(true, Ordering::SeqCst);
    assert!(driver.request_logout().is_err());

    assert!(driver.session.account_generation == generation);
    assert!(driver.session.shutdown == shutdown);
    assert!(driver.session.requests.get(send).is_some());
    assert!(driver.session.requests.get(chats).is_some());
    assert!(
        !driver
            .session
            .requests
            .pending
            .values()
            .any(|p| p.purpose == RequestPurpose::LogOut)
    );
}

#[test]
fn a_logout_that_went_out_settles_the_rest_and_waits_for_its_answer() {
    let mut driver = ready_driver(Arc::new(AtomicBool::new(false)));
    let chats = driver.session.request(RequestPurpose::LoadChats, None);
    let generation = driver.session.account_generation;

    let extra = driver.request_logout().unwrap();

    assert!(driver.session.account_generation != generation);
    assert!(driver.session.requests.get(chats).is_none());
    let kept = driver
        .session
        .requests
        .get(extra)
        .expect("the logOut request");
    assert_eq!(kept.purpose, RequestPurpose::LogOut);
    assert!(
        kept.account_generation == driver.session.account_generation,
        "its answer is not dropped as stale"
    );
}
