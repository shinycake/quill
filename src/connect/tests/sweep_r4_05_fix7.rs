//! Fix round 7 of sweep bundle r4-05: a block or unblock that never left
//! shows its line and waits for nothing.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::platform::MemorySecretStore;
use crate::state::BlockOrigin;
use crate::state::fetch::ChangeNote;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type Recorder = Arc<RecordingSender>;

fn driver_parts() -> (ConnectDriver<Recorder>, Recorder) {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink;
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    (driver, recorder)
}

fn shows_line_waiting_for_nothing(note: &ChangeNote) -> bool {
    matches!(note, ChangeNote::Refused { waiting, .. } if waiting.is_empty())
}

#[test]
fn a_failed_unblock_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(driver.unblock_sender(62).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.block_note
    ));
}

#[test]
fn a_failed_block_send_shows_the_line_with_nothing_waiting() {
    let (mut driver, recorder) = driver_parts();
    recorder.refuse_sends(true);
    assert!(driver.block_sender(61, BlockOrigin::PrivacyPage).is_err());
    assert!(shows_line_waiting_for_nothing(
        &driver.session.settings.block_note
    ));
}
