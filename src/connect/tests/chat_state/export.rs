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
        ONLINE,
    );

    let sent = recorder.snapshot();
    assert!(sent.iter().any(|json| json.contains("getChatHistory")));
    assert!(sent.iter().any(|json| json.contains("getSessions")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cancelled_exports_last_request_passes_the_download_it_stops() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);
    let (chat, worker) = crate::export::ChatExport::scripted();
    driver.session.settings.chat_export = Some(chat);
    let stop_sent = |recorder: &RecordingSender| {
        let mut sent = recorder.snapshot().into_iter();
        sent.any(|json| json.contains("cancelDownloadFile"))
    };

    worker
        .send(serde_json::json!({"@type": "downloadFile"}))
        .unwrap();
    driver.pump_chat_export();
    worker
        .send(serde_json::json!({"@type": "cancelDownloadFile"}))
        .unwrap();
    driver.pump_chat_export();
    assert!(
        !stop_sent(&recorder),
        "one request at a time until cancelled"
    );

    let export = driver.session.settings.chat_export.as_ref().unwrap();
    export.cancel();
    driver.pump_chat_export();
    assert!(stop_sent(&recorder));
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

#[test]
fn an_export_leaves_a_file_the_view_asked_for_in_the_cache() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);
    let (chat, worker) = crate::export::ChatExport::scripted();
    driver.session.settings.chat_export = Some(chat);
    let deletes = |recorder: &RecordingSender| {
        let sent = recorder.snapshot().into_iter();
        sent.filter(|json| json.contains("deleteFile")).count()
    };

    let _ = driver.download_file(crate::ids::FileId(10), 1);
    worker
        .send(serde_json::json!({"@type": "deleteFile", "file_id": 10}))
        .unwrap();
    driver.pump_chat_export();
    assert_eq!(deletes(&recorder), 0, "the view's photo stays");

    worker
        .send(serde_json::json!({"@type": "deleteFile", "file_id": 11}))
        .unwrap();
    driver.pump_chat_export();
    assert_eq!(deletes(&recorder), 1, "a file only the export wanted goes");
    let _ = std::fs::remove_dir_all(&dir);
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sent {
    deletes: usize,
    downloads: usize,
}

/// When the chat's page arrives, against the export's own steps.
#[derive(Clone, Copy)]
enum PageArrives {
    WhileDownloading,
    BeforeTheDelete,
    AfterTheDelete,
}

/// Chat 7 is open while its export downloads, copies and deletes file 1,
/// and a page showing the file arrives. Returns what went to TDLib while
/// the chat was open, and the deletes after it closed.
fn open_chat_shows_a_file_the_export_holds(arrives: PageArrives, photo: &str) -> (Sent, usize) {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let chat = r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#;
    let mut driver = ready_driver(&recorder, &sink, prepared, &[chat]);
    let history = driver.select_chat(ChatId(7)).unwrap().expect("history");
    let (export, worker) = crate::export::ChatExport::scripted();
    driver.session.settings.chat_export = Some(export);
    worker
        .send(serde_json::json!({"@type": "downloadFile", "file_id": 1}))
        .unwrap();
    driver.pump_chat_export();

    let downloading = matches!(arrives, PageArrives::WhileDownloading);
    let (active, done, path) = if downloading {
        (true, false, "")
    } else {
        (false, true, photo)
    };
    let file = format!(
        r#"{{"@type":"file","id":1,"size":4,"expected_size":4,"local":{{"@type":"localFile","path":"{path}","can_be_downloaded":true,"can_be_deleted":true,"is_downloading_active":{active},"is_downloading_completed":{done},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":4}},"remote":{{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":4}}}}"#
    );
    let page = format!(
        r#"{{"@type":"messages","@extra":"{extra}","messages":[{{"id":20,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{file},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}]}}"#,
        extra = history.0,
    );
    let seq = AtomicU64::new(100);
    let mut page = Some(page);
    let mut page_arrives = |driver: &mut ConnectDriver<Arc<RecordingSender>>| {
        if let Some(page) = page.take() {
            driver
                .ingest(copy_and_parse(&page, &seq, &sink).unwrap())
                .unwrap();
        }
    };
    if !matches!(arrives, PageArrives::AfterTheDelete) {
        page_arrives(&mut driver);
    }

    let link = &mut driver.session.settings.chat_export.as_mut().unwrap().link;
    let download = link.pending.expect("the export's download went out");
    link.reply(download, serde_json::json!({"@type": "ok"}));
    worker
        .send(serde_json::json!({"@type": "deleteFile", "file_id": 1, "chat_id": 7}))
        .unwrap();
    driver.pump_chat_export();
    page_arrives(&mut driver);
    driver.pump_exports();
    let count = |method: &str| {
        let sent = recorder.snapshot().into_iter();
        sent.filter(|json| json.contains(method)).count()
    };
    let open = Sent {
        deletes: count("deleteFile"),
        downloads: count("downloadFile"),
    };

    driver.session.leave_open_chat();
    driver.pump_exports();
    let closed = count("deleteFile");
    drop(worker);
    let _ = std::fs::remove_dir_all(&dir);
    (open, closed)
}

#[test]
fn an_export_leaves_a_file_the_open_chat_shows_until_the_chat_closes() {
    let dir = crate::scratch::unique_dir("quill-export-finished");
    let photo = dir.join("photo.jpg");
    std::fs::write(&photo, b"jpeg").unwrap();
    let photo = photo.to_string_lossy();
    let one_download = Sent {
        deletes: 0,
        downloads: 1,
    };
    for (arrives, case) in [
        (
            PageArrives::WhileDownloading,
            "the row loads while the export downloads",
        ),
        (
            PageArrives::BeforeTheDelete,
            "the row shows the finished file",
        ),
        (
            PageArrives::AfterTheDelete,
            "the row loads after the export let go",
        ),
    ] {
        assert_eq!(
            open_chat_shows_a_file_the_export_holds(arrives, &photo),
            (one_download, 1),
            "{case}: no delete and only the export's own download while the chat \
             is open, the delete once it closes"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_held_delete_goes_when_the_export_stops_and_stays_for_a_file_the_view_asked_for() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let chat = r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Ada","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#;
    let mut driver = ready_driver(&recorder, &sink, prepared, &[chat]);
    let _ = driver.select_chat(ChatId(7)).unwrap();
    let (export, worker) = crate::export::ChatExport::scripted();
    driver.session.settings.chat_export = Some(export);
    let deleted = |recorder: &RecordingSender| -> Vec<i64> {
        let sent = recorder.snapshot().into_iter();
        sent.map(|json| serde_json::from_str::<Value>(&json).unwrap())
            .filter(|request| request["@type"] == "deleteFile")
            .map(|request| request["file_id"].as_i64().unwrap())
            .collect()
    };

    for file_id in [1, 2] {
        worker
            .send(serde_json::json!({"@type": "deleteFile", "file_id": file_id, "chat_id": 7}))
            .unwrap();
        driver.pump_chat_export();
    }
    let _ = driver.download_file(crate::ids::FileId(2), 1);
    driver.pump_exports();
    assert!(
        deleted(&recorder).is_empty(),
        "both held while the chat is open"
    );

    driver
        .session
        .settings
        .chat_export
        .as_ref()
        .unwrap()
        .cancel();
    driver.pump_exports();
    assert_eq!(deleted(&recorder), [1], "the view asked for file 2");
    drop(worker);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Answer the sent request of type `method` (the last one) with `body`.
fn answer_export_request(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    recorder: &RecordingSender,
    sink: &Arc<dyn DiagnosticSink>,
    method: &str,
    body: Value,
) -> Value {
    let sent = recorder.snapshot().into_iter().rev();
    let request: Value = sent
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .find(|request| request["@type"] == method)
        .unwrap_or_else(|| panic!("{method} sent"));
    let mut answer = body;
    answer["@extra"] = request["@extra"].clone();
    static SEQ: AtomicU64 = AtomicU64::new(500);
    driver
        .ingest(copy_and_parse(&answer.to_string(), &SEQ, sink).unwrap())
        .unwrap();
    request
}

fn cached_file(id: i64, path: &std::path::Path) -> Value {
    serde_json::json!({"@type": "file", "id": id, "size": 4, "expected_size": 4,
        "local": {"@type": "localFile", "path": path.to_string_lossy(),
            "can_be_downloaded": true, "can_be_deleted": true,
            "is_downloading_active": false, "is_downloading_completed": true,
            "download_offset": 0, "downloaded_prefix_size": 4, "downloaded_size": 4},
        "remote": {"@type": "remoteFile", "id": "AgAD", "unique_id": "u",
            "is_uploading_active": false, "is_uploading_completed": true, "uploaded_size": 4}})
}

#[test]
fn a_file_an_earlier_run_left_in_the_cache_goes_on_the_next_start() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let journal = prepared.paths.root.join("export-leftovers");
    let photo = dir.join("photo.jpg");
    std::fs::write(&photo, b"jpeg").unwrap();
    let earlier = crate::export::leftovers::Leftovers::new(journal.clone(), 1);
    earlier.record(
        5,
        &crate::export::leftovers::Entry {
            remote_id: "AgAD".into(),
            path: Some(photo.canonicalize().unwrap()),
        },
    );
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);

    let asked = answer_export_request(
        &mut driver,
        &recorder,
        &sink,
        "getRemoteFile",
        cached_file(42, &photo),
    );
    assert_eq!(asked["remote_file_id"], "AgAD");
    driver.pump_exports();
    let deleted = answer_export_request(
        &mut driver,
        &recorder,
        &sink,
        "deleteFile",
        serde_json::json!({"@type": "ok"}),
    );

    assert_eq!(deleted["file_id"], 42, "this run's id for the file");
    assert_eq!(std::fs::read_dir(&journal).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_used_since_an_earlier_run_left_it_stays_in_the_cache() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let journal = prepared.paths.root.join("export-leftovers");
    let photo = dir.join("photo.jpg");
    let other = dir.join("again.jpg");
    std::fs::write(&photo, b"jpeg").unwrap();
    std::fs::write(&other, b"jpeg").unwrap();
    let earlier = crate::export::leftovers::Leftovers::new(journal.clone(), 1);
    earlier.record(
        5,
        &crate::export::leftovers::Entry {
            remote_id: "AgAD".into(),
            path: Some(photo.canonicalize().unwrap()),
        },
    );
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);

    answer_export_request(
        &mut driver,
        &recorder,
        &sink,
        "getRemoteFile",
        cached_file(42, &other),
    );
    driver.pump_exports();

    let sent = recorder.snapshot();
    assert!(!sent.iter().any(|json| json.contains("deleteFile")));
    assert_eq!(std::fs::read_dir(&journal).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cache_copy_leaves_the_list_once_its_delete_is_answered_or_the_view_keeps_it() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let journal = prepared.paths.root.join("export-leftovers");
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let mut driver = ready_driver(&recorder, &sink, prepared, &[]);
    let leftovers = driver.session.settings.export_files.leftovers().unwrap();
    let entry = crate::export::leftovers::Entry {
        remote_id: "AgAD".into(),
        path: None,
    };
    leftovers.record(10, &entry);
    leftovers.record(11, &entry);
    let listed = || std::fs::read_dir(&journal).unwrap().count();
    let (chat, worker) = crate::export::ChatExport::scripted();
    driver.session.settings.chat_export = Some(chat);

    let _ = driver.download_file(crate::ids::FileId(10), 1);
    worker
        .send(serde_json::json!({"@type": "deleteFile", "file_id": 10}))
        .unwrap();
    driver.pump_chat_export();
    assert_eq!(listed(), 1, "the view keeps file 10");

    worker
        .send(serde_json::json!({"@type": "deleteFile", "file_id": 11}))
        .unwrap();
    driver.pump_chat_export();
    assert_eq!(listed(), 1, "listed until TDLib answers");
    answer_export_request(
        &mut driver,
        &recorder,
        &sink,
        "deleteFile",
        serde_json::json!({"@type": "ok"}),
    );
    assert_eq!(listed(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
