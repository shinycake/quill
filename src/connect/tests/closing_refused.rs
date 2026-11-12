//! A `close` the sender refuses is held and sent again on the next drain
//! (sweep r4-02, 03-006), instead of the client waiting for a Closed nothing
//! asked for.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::ShutdownPhase;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

fn close_requests(recorder: &RecordingSender) -> usize {
    recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("\"@type\":\"close\""))
        .count()
}

#[test]
fn a_refused_close_is_held_and_sent_again_on_the_next_drain() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    recorder.refuse(Some("close"));
    crate::connect::closing::begin_close(&mut driver);
    assert_eq!(close_requests(&recorder), 0, "the sender refused it");
    let ShutdownPhase::CloseRefused { retry_at } = driver.session.shutdown else {
        panic!("the close waits for another try: {:?}", driver.session.shutdown);
    };
    // A drain before the backoff has passed does not ask again.
    recorder.refuse(None);
    crate::connect::closing::release_close(&mut driver, retry_at - Duration::from_millis(1));
    assert_eq!(close_requests(&recorder), 0, "the backoff holds");
    assert!(matches!(
        driver.session.shutdown,
        ShutdownPhase::CloseRefused { .. }
    ));
    recorder.refuse(Some("close"));
    crate::connect::closing::release_close(&mut driver, retry_at);
    assert_eq!(close_requests(&recorder), 0, "still refused");
    let ShutdownPhase::CloseRefused { retry_at: later } = driver.session.shutdown else {
        panic!("a second refusal backs off again: {:?}", driver.session.shutdown);
    };
    recorder.refuse(None);
    crate::connect::closing::release_close(&mut driver, later);
    assert_eq!(close_requests(&recorder), 1, "sent once the backoff passed");
    assert!(!matches!(
        driver.session.shutdown,
        ShutdownPhase::CloseRefused { .. }
    ));
    let _ = std::fs::remove_dir_all(dir);
}
