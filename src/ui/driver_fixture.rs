//! A real `ConnectDriver` over a recording sender for tests of the code
//! that sends through the live driver: the requests it sends are the real
//! ones, and what it sent can be read back.
use quill::connect::{ConnectDriver, RecordingSender, prepare_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::AccountKey;
use quill::platform::MemorySecretStore;
use quill::state::Session;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// A driver that has taken in `updates` (TDLib JSON, in order), and its
/// recorder. Only what is sent after `sent_so_far()` is the test's own.
pub(crate) fn driver_with(
    updates: &[String],
) -> (ConnectDriver<Arc<RecordingSender>>, Arc<RecordingSender>) {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let credentials = TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    };
    let dir = quill::scratch::unique_dir("quill-driver-fixture");
    let store = MemorySecretStore::new();
    let prepared =
        prepare_connect(&dir, AccountKey::primary(), &store, &credentials).expect("prepare");
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), credentials, prepared);
    let seq = AtomicU64::new(0);
    for json in updates {
        if let Some(parsed) = copy_and_parse(json, &seq, &sink) {
            let _ = driver.ingest(parsed);
        }
    }
    (driver, recorder)
}

/// The `@type` of each request sent after the first `before`.
pub(crate) fn sent_types(recorder: &RecordingSender, before: usize) -> Vec<String> {
    recorder
        .snapshot()
        .iter()
        .skip(before)
        .filter_map(|json| {
            let start = json.find(r#""@type":""#)? + 9;
            let rest = json.get(start..)?;
            Some(rest.get(..rest.find('"')?)?.to_string())
        })
        .collect()
}
