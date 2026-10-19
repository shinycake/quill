//! Connect-driver test: confirming the Replies chat's "Block" box sends
//! `blockMessageSenderFromReplies` with the box's checkboxes.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, MessageId};
use crate::message_menu_origin::RepliesBlock;
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

    assert!(driver.session.users_state.replies_block.is_none());
    let sent = recorder.snapshot();
    let request: Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(request["@type"], "blockMessageSenderFromReplies");
    assert_eq!(request["message_id"], 42);
    assert_eq!(request["report_spam"], true);
    assert_eq!(request["delete_message"], true);
    assert_eq!(request["delete_all_messages"], false);
}
