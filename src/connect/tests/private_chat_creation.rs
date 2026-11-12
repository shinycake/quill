//! `createPrivateChat` is guarded per user, not for everyone at once.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn two_users_chats_are_created_side_by_side_and_one_user_once() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let created = |recorder: &Arc<RecordingSender>| {
        recorder
            .snapshot()
            .iter()
            .filter(|json| json.contains("\"@type\":\"createPrivateChat\""))
            .count()
    };
    assert!(driver.create_private_chat_for(5).unwrap().is_some());
    assert!(driver.create_private_chat_for(6).unwrap().is_some());
    assert!(driver.create_private_chat_for(5).unwrap().is_none());
    assert_eq!(created(&recorder), 2);
    drop(dir);
}
