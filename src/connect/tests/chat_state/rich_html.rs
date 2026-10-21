//! Driver tests: "Save as HTML" request pump (`parity:menu-save-selection-html`).
use super::*;
use crate::rich_html::RichHtmlOutcome;

fn driver_with(
    recorder: &Arc<RecordingSender>,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    prepared: PreparedConnect,
    updates: &[&str],
) -> ConnectDriver<Arc<RecordingSender>> {
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in std::iter::once(
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    )
    .chain(updates.iter().copied())
    {
        driver
            .ingest(copy_and_parse(json, &seq, dyn_sink).unwrap())
            .unwrap();
    }
    driver
}

#[test]
fn rich_html_export_requests_flow_through_the_driver() {
    // The worker's `getMessage` goes out tagged for the export, the answer
    // reaches it, and the page lands in its own folder under the base.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = driver_with(&recorder, &dyn_sink, prepared, &[]);
    let seq = AtomicU64::new(100);
    let base = dir.join("Downloads");

    driver
        .start_rich_html_export(ChatId(16), MessageId(5), Some(base.clone()))
        .unwrap();
    // Saving the same message again while it runs starts nothing new.
    driver
        .start_rich_html_export(ChatId(16), MessageId(5), Some(base.clone()))
        .unwrap();
    assert_eq!(driver.session.settings.rich_html_exports.len(), 1);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut answered = false;
    let outcome = loop {
        assert!(std::time::Instant::now() < deadline, "export never settled");
        driver.pump_rich_html_exports();
        if let Some(outcome) = driver.take_finished_rich_html_exports().pop() {
            break outcome;
        }
        assert_eq!(driver.session.settings.rich_html_exports.len(), 1);
        if !answered
            && let Some(request) = recorder
                .snapshot()
                .into_iter()
                .find(|j| j.contains("\"@type\":\"getMessage\""))
        {
            let value: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(value["chat_id"], 16);
            assert_eq!(value["message_id"], 5);
            let extra = value["@extra"]["quill_account_export"].clone();
            let reply = format!(
                r#"{{"@type":"message","@extra":{{"quill_account_export":{extra}}},"id":5,"chat_id":16,"can_be_saved":true,"content":{{"@type":"messageRichMessage","message":{{"@type":"richMessage","is_rtl":false,"is_full":true,"blocks":[{{"@type":"pageBlockTitle","title":{{"@type":"richTextPlain","text":"Notes"}}}}]}}}}}}"#
            );
            driver
                .ingest(copy_and_parse(&reply, &seq, &dyn_sink).unwrap())
                .unwrap();
            answered = true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let html = base.join("Notes").join("Notes.html");
    assert_eq!(outcome, RichHtmlOutcome::Saved(html.clone()));
    assert!(
        std::fs::read_to_string(&html)
            .unwrap()
            .contains("<h1>Notes</h1>")
    );
    assert!(driver.session.settings.rich_html_exports.is_empty());
    // A finished export no longer blocks saving the same message again.
    driver
        .start_rich_html_export(ChatId(16), MessageId(5), Some(base.clone()))
        .unwrap();
    assert_eq!(driver.session.settings.rich_html_exports.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rich_html_export_refuses_protected_chats() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = driver_with(
        &recorder,
        &dyn_sink,
        prepared,
        &[r#"{"@type":"updateChatHasProtectedContent","chat_id":16,"has_protected_content":true}"#],
    );
    assert!(
        driver
            .start_rich_html_export(ChatId(16), MessageId(5), Some(dir.join("out")))
            .is_err()
    );
    assert!(driver.session.settings.rich_html_exports.is_empty());
    driver.pump_rich_html_exports();
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"getMessage\""))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn logging_out_stops_rich_html_exports() {
    // Dropping the export cancels its worker: nothing is sent after logout
    // and no "could not save" toast follows it.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = driver_with(&recorder, &dyn_sink, prepared, &[]);
    driver
        .start_rich_html_export(ChatId(16), MessageId(5), Some(dir.join("out")))
        .unwrap();
    assert_eq!(driver.session.settings.rich_html_exports.len(), 1);
    let seq = AtomicU64::new(100);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateLoggingOut"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.settings.rich_html_exports.is_empty());
    assert!(driver.take_finished_rich_html_exports().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn starting_a_logout_stops_rich_html_exports() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = driver_with(&recorder, &dyn_sink, prepared, &[]);
    driver
        .start_rich_html_export(ChatId(16), MessageId(5), Some(dir.join("out")))
        .unwrap();
    driver.session.begin_logout();
    assert!(driver.session.settings.rich_html_exports.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
