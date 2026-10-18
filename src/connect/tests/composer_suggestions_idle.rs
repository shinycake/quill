use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn plain_typing_leaves_nothing_for_the_suggestions_to_do() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    assert!(driver.composer_suggestions_idle("hello wor"));
    assert!(driver.composer_suggestions_idle(""));
    // A trailing emoji is work.
    assert!(!driver.composer_suggestions_idle("hello \u{1f600}"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn held_suggestions_are_cleared_by_the_next_plain_keystroke() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.update_sticker_suggestions("\u{1f600}").unwrap();
    driver
        .update_animated_emoji_suggestion("\u{1f600}")
        .unwrap();
    // The emoji is gone from the text, but a request and its marker are
    // still held: the suggestions are not idle until they are cleared.
    assert!(!driver.composer_suggestions_idle("hello"));
    driver.update_sticker_suggestions("hello").unwrap();
    driver.update_animated_emoji_suggestion("hello").unwrap();
    assert!(driver.composer_suggestions_idle("hello"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_path_that_is_not_live_is_never_idle() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    driver.session.auth = crate::telegram::envelope::AuthorizationState::WaitPhoneNumber;
    assert!(
        !driver.composer_suggestions_idle("hello"),
        "the failure note still shows while the path is down"
    );
    let _ = std::fs::remove_dir_all(dir);
}
