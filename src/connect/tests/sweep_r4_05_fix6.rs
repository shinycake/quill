//! Fix round 6 of sweep bundle r4-05: only the Privacy page's own block or
//! unblock starts the block-list note's wait.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::state::BlockOrigin;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (ConnectDriver<Recorder>, Arc<dyn DiagnosticSink>, AtomicU64) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, dyn_sink, seq)
}

fn refuse(
    driver: &mut ConnectDriver<Recorder>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    request: crate::ids::RequestId,
) {
    let json = format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"BROKEN"}}"#,
        request.0
    );
    driver
        .ingest(copy_and_parse(&json, seq, sink).unwrap())
        .unwrap();
}

#[test]
fn a_refused_chat_bar_block_shows_nothing_on_privacy() {
    let (mut driver, sink, seq) = driver_parts();
    let id = driver.block_sender(61, BlockOrigin::ChatBar).unwrap();
    refuse(&mut driver, &seq, &sink, id);
    assert!(driver.session.settings.block_note.line().is_none());
}

#[test]
fn a_chat_bar_block_does_not_drop_a_privacy_wait() {
    let (mut driver, sink, seq) = driver_parts();
    let unblock = driver.unblock_sender(62).unwrap();
    let bar = driver.block_sender(61, BlockOrigin::ChatBar).unwrap();
    refuse(&mut driver, &seq, &sink, bar);
    assert!(driver.session.settings.block_note.line().is_none());
    refuse(&mut driver, &seq, &sink, unblock);
    assert!(driver.session.settings.block_note.line().is_some());
}

#[test]
fn the_privacy_pages_own_block_shows_its_refusal() {
    let (mut driver, sink, seq) = driver_parts();
    let id = driver.block_sender(61, BlockOrigin::PrivacyPage).unwrap();
    refuse(&mut driver, &seq, &sink, id);
    assert!(driver.session.settings.block_note.line().is_some());
}

#[test]
fn two_unblocks_out_at_once_show_the_first_ones_refusal() {
    let (mut driver, sink, seq) = driver_parts();
    let first = driver.unblock_sender(61).unwrap();
    let _second = driver.unblock_sender(62).unwrap();
    refuse(&mut driver, &seq, &sink, first);
    assert!(driver.session.settings.block_note.line().is_some());
}
