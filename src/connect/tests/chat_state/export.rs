//! Driver tests: chat export request pump and call-history clearing.
use super::*;

fn ready_driver(
    recorder: &Arc<RecordingSender>,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    prepared: PreparedConnect,
    extra_updates: &[&str],
) -> ConnectDriver<Arc<RecordingSender>> {
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in std::iter::once(
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    )
    .chain(extra_updates.iter().copied())
    {
        driver
            .ingest(copy_and_parse(json, &seq, dyn_sink).unwrap())
            .unwrap();
    }
    driver
}

fn export_settings(dir: &std::path::Path) -> crate::export::ExportSettings {
    crate::export::ExportSettings::new(16, "Export Chat".into(), dir.to_path_buf())
}

#[test]
fn an_account_off_screen_still_forwards_its_exports_requests() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);
    let (chat, chat_worker) = crate::export::ChatExport::scripted();
    let (account, account_worker) = crate::export::AccountExport::scripted();
    driver.session.settings.chat_export = Some(chat);
    driver.session.settings.account_export = Some(account);

    chat_worker
        .send(serde_json::json!({"@type": "getChatHistory"}))
        .unwrap();
    account_worker
        .send(serde_json::json!({"@type": "getSessions"}))
        .unwrap();
    driver.poll_ticks(
        crate::connect::TickScope::Background,
        std::time::Instant::now(),
    );

    let sent = recorder.snapshot();
    assert!(sent.iter().any(|json| json.contains("getChatHistory")));
    assert!(sent.iter().any(|json| json.contains("getSessions")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_export_worker_requests_flow_through_the_driver() {
    // `parity:data-chat-export-html`: the worker's TDLib requests go out
    // tagged for the export, the answer is routed back to the worker, and an
    // empty history ends the export with an (empty) HTML page.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &dyn_sink, prepared, &[]);
    let seq = AtomicU64::new(100);

    driver.start_chat_export(export_settings(&dir)).unwrap();
    // A second export while one is on screen is refused.
    assert!(driver.start_chat_export(export_settings(&dir)).is_err());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut answered = false;
    loop {
        assert!(std::time::Instant::now() < deadline, "export never settled");
        driver.pump_chat_export();
        let export = driver
            .session
            .settings
            .chat_export
            .as_ref()
            .expect("export active");
        if export.progress().is_settled() {
            break;
        }
        if !answered
            && let Some(request) = recorder
                .snapshot()
                .into_iter()
                .find(|j| j.contains("\"@type\":\"getChatHistory\""))
        {
            let value: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(value["chat_id"], 16);
            assert_eq!(value["from_message_id"], 0);
            assert_eq!(value["limit"], 100);
            let extra = value["@extra"]["quill_account_export"].clone();
            let reply = format!(
                r#"{{"@type":"messages","@extra":{{"quill_account_export":{extra}}},"total_count":0,"messages":[]}}"#
            );
            driver
                .ingest(copy_and_parse(&reply, &seq, &dyn_sink).unwrap())
                .unwrap();
            answered = true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let export = driver.session.settings.chat_export.as_ref().unwrap();
    let crate::export::Stage::Finished(summary) = export.progress().stage else {
        panic!("export should finish: {:?}", export.progress());
    };
    assert_eq!(summary.messages, 0);
    assert!(summary.folder.join("messages.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn account_export_worker_requests_flow_through_the_driver() {
    // `parity:data-export-full`: the account export's requests go out tagged
    // for it, one at a time, and answers reach the worker. With only
    // sessions chosen it finishes after a handful of fixture answers.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &dyn_sink, prepared, &[]);
    let seq = AtomicU64::new(100);

    let mut settings = crate::export::AccountSettings::new(dir.clone());
    settings.sections = crate::export::account::Sections::none();
    assert!(driver.start_account_export(settings.clone()).is_err());
    settings.sections =
        crate::export::account::Sections::of(&[crate::export::account::Section::Sessions]);
    driver.start_account_export(settings.clone()).unwrap();
    assert!(driver.start_account_export(settings).is_err());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut answered = 0;
    loop {
        assert!(std::time::Instant::now() < deadline, "export never settled");
        driver.pump_account_export();
        let export = driver.session.settings.account_export.as_ref().unwrap();
        if export.progress().is_settled() {
            break;
        }
        let sent: Vec<String> = recorder
            .snapshot()
            .into_iter()
            .filter(|j| j.contains("quill_account_export"))
            .collect();
        if sent.len() > answered {
            assert_eq!(sent.len(), answered + 1, "one request at a time");
            let value: Value = serde_json::from_str(&sent[answered]).unwrap();
            let extra = value["@extra"]["quill_account_export"].clone();
            let body = match value["@type"].as_str().unwrap() {
                "getMe" => r#""@type":"user","id":1,"first_name":"Fixture""#.to_string(),
                "getActiveSessions" => r#""@type":"sessions","sessions":[]"#.to_string(),
                "getConnectedWebsites" => {
                    r#""@type":"connectedWebsites","websites":[]"#.to_string()
                }
                other => panic!("unexpected request {other}"),
            };
            let reply = format!(r#"{{{body},"@extra":{{"quill_account_export":{extra}}}}}"#);
            driver
                .ingest(copy_and_parse(&reply, &seq, &dyn_sink).unwrap())
                .unwrap();
            answered += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(answered, 3);
    let export = driver.session.settings.account_export.as_ref().unwrap();
    let crate::export::account::AccountStage::Finished(summary) = export.progress().stage else {
        panic!("export should finish: {:?}", export.progress());
    };
    assert!(summary.folder.join("export_results.html").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_export_refuses_protected_chats() {
    // A chat with `has_protected_content` can't be saved or forwarded, so
    // the export never starts and no TDLib request goes out.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(
        &recorder,
        &dyn_sink,
        prepared,
        &[r#"{"@type":"updateChatHasProtectedContent","chat_id":16,"has_protected_content":true}"#],
    );

    assert!(driver.start_chat_export(export_settings(&dir)).is_err());
    assert!(driver.session.settings.chat_export.is_none());
    driver.pump_chat_export();
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"@type\":\"getChatHistory\""))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn clear_call_history_empties_the_list_only_on_ok() {
    // "Clear all" on the Calls list: `deleteAllCallMessages` goes out with
    // the revoke flag, the cached list stays until TDLib answers `ok`,
    // and a refusal keeps it and reports the error.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    let ingest = |driver: &mut ConnectDriver<Arc<ViewCtlSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };

    // Nothing to clear: nothing is sent.
    assert!(driver.clear_call_history(false).expect("idle").is_none());

    let page = driver.fetch_call_history().expect("history page");
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"foundMessages","@extra":"{}","total_count":1,"messages":[{{"id":901,"chat_id":7,"is_outgoing":false,"date":1790000000,"content":{{"@type":"messageCall","unique_id":901,"is_video":false,"discard_reason":{{"@type":"callDiscardReasonMissed"}},"duration":0}}}}],"next_offset":""}}"#,
            page.0
        ),
    );
    assert_eq!(driver.session.calls.recent_calls.len(), 1);

    let sent = driver.clear_call_history(true).expect("send").expect("id");
    let request = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("\"deleteAllCallMessages\""))
        .expect("deleteAllCallMessages sent");
    assert!(request.contains("\"revoke\":true"));
    assert_eq!(driver.session.calls.recent_calls.len(), 1, "kept until ok");
    assert!(driver.session.calls.recent_calls_clearing);
    assert!(
        driver.clear_call_history(true).expect("again").is_none(),
        "one clear in flight at a time"
    );

    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":500,"message":"FAILED"}}"#,
            sent.0
        ),
    );
    assert_eq!(driver.session.calls.recent_calls.len(), 1);
    assert!(!driver.session.calls.recent_calls_clearing);
    assert_eq!(
        driver.session.chats_state.chat_action_error.as_deref(),
        Some("could not clear the call history (error 500)")
    );

    let sent = driver.clear_call_history(false).expect("send").expect("id");
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, sent.0),
    );
    assert!(driver.session.calls.recent_calls.is_empty());
    assert!(!driver.session.calls.recent_calls_clearing);
    let _ = std::fs::remove_dir_all(&dir);
}
