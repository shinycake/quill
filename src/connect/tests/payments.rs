//! Slice payments: "clear saved payment/shipping info" dispatch.
use super::super::*;
use super::support::{prepared_tmp, ready_driver, sent_request, test_credentials};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::Session;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn clear_saved_payment_info_dispatches_both_deletes() {
    // The clear action must fire BOTH parameterless `= Ok` constructors
    // (`deleteSavedOrderInfo` + `deleteSavedCredentials`) with their own
    // `@extra` ids so the answers correlate.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);

    assert_eq!(driver.clear_saved_payment_info().unwrap(), 2);

    let order = sent_request(&recorder, "deleteSavedOrderInfo");
    let creds = sent_request(&recorder, "deleteSavedCredentials");
    assert_ne!(order["@extra"], creds["@extra"]);
}

#[test]
fn clear_saved_payment_info_refuses_without_ready_path() {
    // The driver gate refuses before anything is registered or sent.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let session = Session::new(AccountKey::primary(), sink);
    let mut driver = ConnectDriver::new(
        session,
        Arc::new(RecordingSender::new()),
        test_credentials(),
        prepared,
    );

    assert_eq!(
        driver.clear_saved_payment_info(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(driver.session.requests.pending.is_empty());
}
