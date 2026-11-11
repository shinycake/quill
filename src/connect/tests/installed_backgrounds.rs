//! Connect-driver tests: the installed wallpapers list asked for from a
//! page that opens (`getInstalledBackgrounds`).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Records requests; refuses them while `refuse` is set.
struct SwitchedSender {
    recorder: RecordingSender,
    refuse: AtomicBool,
}

impl JsonSender for Arc<SwitchedSender> {
    fn send_json(&self, json: &str) -> Result<(), ConnectSendError> {
        if self.refuse.load(Ordering::SeqCst) {
            return Err(ConnectSendError::Native);
        }
        self.recorder.send_json(json)
    }
}

fn asks(sender: &SwitchedSender) -> usize {
    sender
        .recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("getInstalledBackgrounds"))
        .count()
}

#[test]
fn a_list_request_that_cannot_leave_fails_once_and_waits_for_retry() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let session = Session::new(crate::ids::AccountKey::primary(), sink.clone());
    let sender = Arc::new(SwitchedSender {
        recorder: RecordingSender::new(),
        refuse: AtomicBool::new(false),
    });
    let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    let ready = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;
    driver
        .ingest(copy_and_parse(ready, &seq, &sink).unwrap())
        .unwrap();

    sender.refuse.store(true, Ordering::SeqCst);
    assert!(driver.fetch_installed_backgrounds(false).is_err());
    let list = &driver.session.chats_state.installed_backgrounds;
    assert!(list.error().is_some(), "the refused send is a failed list");
    assert!(!list.shows_loading());

    sender.refuse.store(false, Ordering::SeqCst);
    assert!(
        matches!(driver.fetch_installed_backgrounds(false), Ok(None)),
        "a failed list is not asked again until Retry"
    );
    assert_eq!(asks(&sender), 0);

    assert!(matches!(
        driver.retry_installed_backgrounds(false),
        Ok(Some(_))
    ));
    assert_eq!(asks(&sender), 1);
    assert!(
        driver
            .session
            .chats_state
            .installed_backgrounds
            .is_loading()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
