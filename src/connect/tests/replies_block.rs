//! Connect-driver test: confirming the Replies chat's "Block" box sends
//! `blockMessageSenderFromReplies` with the box's checkboxes.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, MessageId};
use crate::message_menu_origin::{RepliesBlock, RepliesBlockStage};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn confirming_the_block_box_sends_its_checkboxes() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    driver
        .ingest(copy_and_parse(ready, &seq, &dyn_sink).unwrap())
        .unwrap();

    let before = recorder.snapshot().len();
    driver.submit_replies_block().unwrap();
    assert_eq!(recorder.snapshot().len(), before, "no box, no request");

    let mut block = RepliesBlock::new(MessageId(42), "Ada".into(), "Ada L".into(), true);
    block.delete_all = false;
    driver.session.users_state.replies_block = Some(block);
    driver.submit_replies_block().unwrap();

    assert!(
        matches!(
            driver
                .session
                .users_state
                .replies_block
                .as_ref()
                .map(|b| b.stage),
            Some(RepliesBlockStage::Submitting(_))
        ),
        "the box waits for the answer"
    );
    let sent = recorder.snapshot();
    let request: Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(request["@type"], "blockMessageSenderFromReplies");
    assert_eq!(request["message_id"], 42);
    assert_eq!(request["report_spam"], true);
    assert_eq!(request["delete_message"], true);
    assert_eq!(request["delete_all_messages"], false);
}

#[test]
fn a_block_that_cannot_be_sent_keeps_its_box_and_choices() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink);
    // Never ready: the chats path is not active (locked, signed out).
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let block = RepliesBlock::new(MessageId(42), "Ada".into(), "Ada L".into(), true);
    driver.session.users_state.replies_block = Some(block);
    assert!(driver.submit_replies_block().is_err());
    assert!(
        driver.session.users_state.replies_block.is_some(),
        "the box and its checkboxes stay for another try"
    );
    assert!(recorder.snapshot().is_empty());
}

fn submitted_block_driver() -> (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    u64,
) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    driver
        .ingest(copy_and_parse(ready, &seq, &dyn_sink).unwrap())
        .unwrap();
    let block = RepliesBlock::new(MessageId(42), "Ada".into(), "Ada L".into(), true);
    driver.session.users_state.replies_block = Some(block);
    driver.submit_replies_block().unwrap();
    let extra = match driver
        .session
        .users_state
        .replies_block
        .as_ref()
        .map(|b| b.stage)
    {
        Some(RepliesBlockStage::Submitting(id)) => id.0,
        _ => 0,
    };
    (driver, recorder, extra)
}

#[test]
fn the_blocked_note_waits_for_the_ok_answer() {
    let (mut driver, recorder, extra) = submitted_block_driver();
    let seq = AtomicU64::new(1_000);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    assert!(driver.session.users_state.replies_blocked.is_empty());
    let sent = recorder.snapshot().len();
    driver.submit_replies_block().unwrap();
    assert_eq!(
        recorder.snapshot().len(),
        sent,
        "a second press sends nothing"
    );
    let ok = format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#);
    driver
        .ingest(copy_and_parse(&ok, &seq, &sink).unwrap())
        .unwrap();
    assert!(driver.session.users_state.replies_block.is_none());
    assert_eq!(driver.session.users_state.replies_blocked, vec!["Ada"]);
}

#[test]
fn a_rejected_block_gives_the_box_its_choices_back() {
    let (mut driver, _recorder, extra) = submitted_block_driver();
    let seq = AtomicU64::new(1_000);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let error = format!(r#"{{"@type":"error","@extra":"{extra}","code":400,"message":"X"}}"#);
    driver
        .ingest(copy_and_parse(&error, &seq, &sink).unwrap())
        .unwrap();
    let stage = driver
        .session
        .users_state
        .replies_block
        .as_ref()
        .map(|b| b.stage);
    assert_eq!(stage, Some(RepliesBlockStage::Editing));
    assert!(driver.session.users_state.replies_blocked.is_empty());
}
