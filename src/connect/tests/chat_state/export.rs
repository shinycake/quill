//! Driver tests: chat export paging and call-history clearing.
use super::*;

#[test]
fn chat_export_pages_history_until_a_page_adds_nothing() {
    // `parity:platform-chat-export`: starting an export sends
    // `getChatHistory` (newest first, limit 100); a full page triggers the
    // next page from the oldest id, a short page ends paging.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    driver
        .start_chat_export(ChatId(16), "Export Chat".into(), Default::default())
        .unwrap();
    // A second export while one is running is refused.
    assert!(
        driver
            .start_chat_export(ChatId(16), "Export Chat".into(), Default::default())
            .is_err()
    );
    let first = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("\"@type\":\"getChatHistory\""))
        .expect("export history JSON sent");
    let value: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(value["chat_id"], 16);
    assert_eq!(value["from_message_id"], 0);
    assert_eq!(value["limit"], 100);

    // Full page (100 messages, ids 1000..901) → not done, next page due.
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::ExportChatHistory, Some(ChatId(16)))
        .expect("export page in flight");
    let msgs: Vec<String> = (0..100)
        .map(|i| {
            let id = 1000 - i;
            format!(
                r#"{{"id":{id},"chat_id":16,"is_outgoing":false,"date":1700000000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m{id}","entities":[]}}}}}}"#
            )
        })
        .collect();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"messages","@extra":"{}","messages":[{}]}}"#,
                    extra.0,
                    msgs.join(",")
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let export = driver.session.chat_export.as_ref().expect("export active");
    assert_eq!(export.messages.len(), 100);
    assert_eq!(export.messages[0].text.as_deref(), Some("m1000"));
    assert!(!export.done_paging);
    assert!(!export.in_flight);

    // Pump sends the next page from the oldest fetched id (901).
    driver.pump_chat_export();
    let second = recorder
        .snapshot()
        .into_iter()
        .filter(|j| j.contains("\"@type\":\"getChatHistory\""))
        .nth(1)
        .expect("second export page sent");
    let value: Value = serde_json::from_str(&second).unwrap();
    assert_eq!(value["from_message_id"], 901);

    // Short page (2 messages: the boundary id 901 re-included by TDLib's
    // inclusive from_message_id, then 900) → boundary deduped. Short is
    // not the end: TDLib picks the page size (schema 1.8.67, line 11822).
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::ExportChatHistory, Some(ChatId(16)))
        .expect("second export page in flight");
    let msg = |id: i64, text: &str| {
        format!(
            r#"{{"id":{id},"chat_id":16,"is_outgoing":true,"date":1699999999,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
        )
    };
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"messages","@extra":"{}","messages":[{},{}]}}"#,
                    extra.0,
                    msg(901, "m901"),
                    msg(900, "last"),
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let export = driver.session.chat_export.as_ref().expect("export active");
    // 100 from page 1 + 1 new (901 was already there — no duplicate).
    assert_eq!(export.messages.len(), 101);
    assert_eq!(export.messages.iter().filter(|m| m.id == 901).count(), 1);
    assert!(export.messages.last().unwrap().outgoing);
    assert!(!export.done_paging);

    // A page with nothing new (only the boundary) ends paging.
    driver.pump_chat_export();
    let third = recorder
        .snapshot()
        .into_iter()
        .filter(|j| j.contains("\"@type\":\"getChatHistory\""))
        .nth(2)
        .expect("third export page sent");
    let value: Value = serde_json::from_str(&third).unwrap();
    assert_eq!(value["from_message_id"], 900);
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::ExportChatHistory, Some(ChatId(16)))
        .expect("third export page in flight");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"messages","@extra":"{}","messages":[{}]}}"#,
                    extra.0,
                    msg(900, "last"),
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let export = driver.session.chat_export.as_ref().expect("export active");
    assert_eq!(export.messages.len(), 101);
    assert!(export.done_paging);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn chat_export_refuses_protected_chats() {
    // A chat with `has_protected_content` can't be saved or forwarded, so
    // the export never starts and no `getChatHistory` page goes out.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateChatHasProtectedContent","chat_id":16,"has_protected_content":true}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }

    assert!(
        driver
            .start_chat_export(ChatId(16), "Protected".into(), Default::default())
            .is_err()
    );
    assert!(driver.session.chat_export.is_none());
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
        driver.session.chat_action_error.as_deref(),
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
