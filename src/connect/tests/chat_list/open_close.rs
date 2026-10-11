//! Driver tests: opening and closing chats, the archive list, the action bar and folder sharing.
use super::*;

#[test]
fn select_chat_closes_previous_and_does_not_mark_unread_locally() {
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
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":3,"last_read_inbox_message_id":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":1}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.select_chat(ChatId(7)).unwrap();
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
    driver.select_chat(ChatId(8)).unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":7"))
    );
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":8"))
    );
    // Unread is TDLib-authoritative; opening must not zero it locally.
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 3);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":9,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.chats.get(&7).unwrap().unread_count, 0);
    assert!(!sink.rendered().contains("Alice"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_open_chat_is_closed_when_leaving_it() {
    // `openChat` / `closeChat` must pair (schema 1.8.67: "Informs TDLib
    // that the chat is opened/closed by the user"); Telegram X sends
    // `CloseChat` for every chat it opened (`Tdlib.openChat` /
    // `Tdlib.closeChatImpl` track `openedChats`). A chat whose type is not
    // known yet is still opened, so it must be closed.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    for json in [
        r#"{"@type":"updateChatPosition","chat_id":9,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"5","is_pinned":false}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":8,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":8},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    driver.select_chat(ChatId(9)).unwrap();
    driver.select_chat(ChatId(8)).unwrap();
    let opened_9 = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":9"))
        .count();
    let closed_9 = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("\"@type\":\"closeChat\"") && j.contains("\"chat_id\":9"))
        .count();
    assert_eq!(opened_9, 1);
    assert_eq!(closed_9, 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn archive_list_is_loaded_after_the_main_list() {
    // TDLib reports a list's chats only once the list is loaded: "The
    // loaded chats and their positions in the chat list will be sent
    // through updates" (`loadChats`, schema 1.8.67, line 11595). The
    // archive was never loaded, so it showed only chats that happened to
    // get a position. Telegram X loads every list it shows through
    // `loadChats` (`TdlibChatList.loadMore`).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let load_chats = |recorder: &RecordingSender| -> Vec<Value> {
        recorder
            .snapshot()
            .iter()
            .filter_map(|j| serde_json::from_str::<Value>(j).ok())
            .filter(|v| v["@type"] == "loadChats")
            .collect()
    };
    let answer = |driver: &mut ConnectDriver<Arc<RecordingSender>>, extra: &Value, ok: bool| {
        let extra = extra.as_str().unwrap();
        let json = if ok {
            format!(r#"{{"@type":"ok","@extra":"{extra}"}}"#)
        } else {
            format!(r#"{{"@type":"error","@extra":"{extra}","code":404,"message":"Not Found"}}"#)
        };
        driver
            .ingest(copy_and_parse(&json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_list"]["@type"], "chatListMain");
    // Main list exhausted → the archive starts paging.
    answer(&mut driver, &sent[0]["@extra"], false);
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1]["chat_list"]["@type"], "chatListArchive");
    assert_eq!(sent[1]["limit"], MAIN_CHAT_LOAD_LIMIT);
    // ok → next archive page; 404 → done.
    answer(&mut driver, &sent[1]["@extra"], true);
    let sent = load_chats(&recorder);
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[2]["chat_list"]["@type"], "chatListArchive");
    answer(&mut driver, &sent[2]["@extra"], false);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatTitle","chat_id":1,"title":"x"}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(load_chats(&recorder).len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_dismisses_and_shares_through_the_action_bar() {
    // Batch 8: close sends `removeChatActionBar` and drops the bar; Share
    // my phone number sends `sharePhoneNumber` for the peer user.
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":501,"title":"Stranger","type":{"@type":"chatTypePrivate","user_id":501},"action_bar":{"@type":"chatActionBarSharePhoneNumber"}}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    assert!(driver.session.chat_action_bar(ChatId(501)).is_some());
    driver.share_phone_number(ChatId(501), 501).unwrap();
    assert!(driver.session.chat_action_bar(ChatId(501)).is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"sharePhoneNumber\"") && j.contains("\"user_id\":501"))
    );

    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatActionBar","chat_id":501,"action_bar":{"@type":"chatActionBarAddContact"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    driver.dismiss_chat_action_bar(ChatId(501)).unwrap();
    assert!(driver.session.chat_action_bar(ChatId(501)).is_none());
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"removeChatActionBar\"") && j.contains("\"chat_id\":501"))
    );
    // Nothing to dismiss now: no second request.
    assert_eq!(driver.dismiss_chat_action_bar(ChatId(501)), Ok(None));
}

#[test]
fn driver_folder_share_requests_and_optimistic_delete() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    // Not ready: nothing is sent.
    assert_eq!(
        driver.fetch_recommended_chat_folders(),
        Err(ConnectSendError::InvalidRequest)
    );
    seed_ready_alice(&mut driver, &seq, &dyn_sink);

    driver.fetch_folder_share(5).expect("share fetch");
    let sent = recorder.snapshot();
    assert!(sent.iter().any(|j| j.contains("getChatFolderInviteLinks")));
    assert!(
        sent.iter()
            .any(|j| j.contains("getChatsForChatFolderInviteLink"))
    );
    // In-flight requests are not duplicated.
    driver.fetch_folder_share(5).expect("deduped");
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("getChatFolderInviteLinks"))
            .count(),
        1
    );

    driver.session.chat_list.folder_invite_links.insert(
        5,
        vec![crate::telegram::envelope::ChatFolderInviteLink {
            invite_link: "https://t.me/addlist/a".into(),
            name: String::new(),
            chat_ids: vec![7],
        }],
    );
    driver
        .delete_folder_invite_link(5, "https://t.me/addlist/a")
        .expect("delete");
    assert!(driver.session.chat_list.folder_invite_links[&5].is_empty());

    driver
        .check_folder_invite_link("https://t.me/addlist/b")
        .expect("check");
    assert_eq!(
        driver.session.chat_list.folder_invite_link.as_deref(),
        Some("https://t.me/addlist/b")
    );
    driver
        .add_folder_by_invite_link("https://t.me/addlist/b", &[7, 8])
        .expect("add");
    let last = recorder.snapshot().last().cloned().unwrap();
    let v: Value = serde_json::from_str(&last).unwrap();
    assert_eq!(v["@type"], "addChatFolderByInviteLink");
    assert_eq!(v["chat_ids"], serde_json::json!([7, 8]));
    let _ = std::fs::remove_dir_all(&dir);
}
