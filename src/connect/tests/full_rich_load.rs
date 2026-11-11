//! "Load full message": the stage is held per message from the send until
//! the answer (gone) or the error (failed, tap again).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, ChatMessage, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::state::fetch::Fetch;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn asked() -> (ConnectDriver<Arc<RecordingSender>>, u64, ChatMessage) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    let seq = AtomicU64::new(0);
    driver
        .ingest(copy_and_parse(ready, &seq, &dyn_sink).unwrap())
        .unwrap();
    let key = ChatMessage::new(ChatId(7), MessageId(42));
    let extra = driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    (driver, extra.0, key)
}

#[test]
fn the_load_is_loading_until_the_answer_and_gone_after_it() {
    let (mut driver, extra, key) = asked();
    assert!(driver.session.messages.full_rich_loads[&key].is_loading());
    let answer = format!(
        r#"{{"@type":"richMessage","@extra":"{extra}","is_rtl":false,"is_full":true,"blocks":[]}}"#
    );
    let seq = AtomicU64::new(1_000);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    driver
        .ingest(copy_and_parse(&answer, &seq, &sink).unwrap())
        .unwrap();
    assert!(!driver.session.messages.full_rich_loads.contains_key(&key));
}

#[test]
fn a_failed_load_is_failed_and_can_be_asked_again() {
    let (mut driver, extra, key) = asked();
    let error = format!(r#"{{"@type":"error","@extra":"{extra}","code":500,"message":"X"}}"#);
    let seq = AtomicU64::new(1_000);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    driver
        .ingest(copy_and_parse(&error, &seq, &sink).unwrap())
        .unwrap();
    assert!(matches!(
        driver.session.messages.full_rich_loads.get(&key),
        Some(Fetch::Failed { .. })
    ));
    driver
        .fetch_full_rich_message(key.chat, key.message)
        .unwrap();
    assert!(driver.session.messages.full_rich_loads[&key].is_loading());
}
