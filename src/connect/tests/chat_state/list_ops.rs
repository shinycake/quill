//! Driver tests: mark read, pinned order, recent chats, archive settings and private chats.
use super::*;

#[test]
fn cl1_mark_chat_as_read_sends_view_plus_untoggle() {
    // Slice CL1: "Mark as read" follows Telegram X
    // (`Tdlib.markChatAsRead` with `MessageSourceChatList`) —
    // `viewMessages` over the newest known message reads real
    // unread history, and the manual marked-unread flag is cleared.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateChatIsMarkedAsUnread","chat_id":7,"is_marked_as_unread":true}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    let sent = driver.mark_chat_as_read(ChatId(7)).expect("send");
    assert!(sent.is_some(), "something to read");
    let snapshot = recorder.snapshot();
    let view = snapshot
        .iter()
        .find(|j| j.contains("\"viewMessages\""))
        .expect("viewMessages sent");
    let view_value: Value = serde_json::from_str(view).unwrap();
    assert_eq!(view_value["source"]["@type"], "messageSourceChatList");
    assert_eq!(view_value["message_ids"], serde_json::json!([11]));
    assert_eq!(view_value["force_read"], true);
    let untoggle = snapshot
        .iter()
        .find(|j| j.contains("\"toggleChatIsMarkedAsUnread\""))
        .expect("untoggle sent");
    let untoggle_value: Value = serde_json::from_str(untoggle).unwrap();
    assert_eq!(untoggle_value["is_marked_as_unread"], false);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl1_mark_chat_as_read_noop_when_nothing_unread() {
    // Slice CL1: "Mark as read" on a fully-read chat sends nothing
    // (honest noop, like TGX skipping when there is nothing to do).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    // ready_private_chat seeds unread_count=1; mark it read first.
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":11,"unread_count":0}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    assert_eq!(driver.mark_chat_as_read(ChatId(7)).expect("send"), None);
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("viewMessages") && !j.contains("toggleChatIsMarkedAsUnread")),
        "noop must not send read requests"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_set_pinned_chat_order_sends_full_list_and_rolls_back() {
    // Slice CL2: the pin drag reorder sends the full reordered id
    // list via `setPinnedChats` (TGX `ChatsAdapter.movePinnedChat`
    // semantics), applies optimistically, and restores the old
    // order on refusal.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Bob","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    for (id, order) in [(7i64, 300i64), (9, 200)] {
        let chat = driver.session.chats.get_mut(&id).expect("chat");
        chat.in_main_list = true;
        chat.is_pinned = true;
        chat.order = order;
    }

    let sent = driver
        .set_pinned_chat_order(false, vec![9, 7])
        .expect("send");
    assert!(sent.is_some(), "order actually changed");
    let snapshot = recorder.snapshot();
    let set_pinned = snapshot
        .iter()
        .find(|j| j.contains("\"setPinnedChats\""))
        .expect("setPinnedChats sent");
    let set_pinned_value: Value = serde_json::from_str(set_pinned).unwrap();
    assert_eq!(set_pinned_value["chat_list"]["@type"], "chatListMain");
    assert_eq!(set_pinned_value["chat_ids"], serde_json::json!([9, 7]));
    // Optimistic: the model order flipped before the answer.
    assert_eq!(driver.session.chats.get(&9).expect("chat").order, 300);
    assert_eq!(driver.session.chats.get(&7).expect("chat").order, 200);

    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_NOT_MODIFIED"}}"#,
                    sent.unwrap().0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.session.chats.get(&7).expect("chat").order, 300);
    assert_eq!(driver.session.chats.get(&9).expect("chat").order, 200);
    assert_eq!(
        driver.session.chats_state.chat_action_error.as_deref(),
        Some("could not reorder pinned chats (error 400)")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_set_pinned_chat_order_id_mismatch_is_noop() {
    // Slice CL2: a drag order whose id set doesn't match the
    // current pins sends nothing — `setPinnedChats` requires the
    // complete list.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    let chat = driver.session.chats.get_mut(&7).expect("chat");
    chat.in_main_list = true;
    chat.is_pinned = true;

    assert_eq!(
        driver
            .set_pinned_chat_order(false, vec![7, 999])
            .expect("send"),
        None
    );
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("setPinnedChats")),
        "mismatched order must not send"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_mark_all_chats_as_read_sends_read_chat_list() {
    // Slice CL2: \"Mark all as read\" sends `readChatList`; badges
    // clear via the server updates, nothing is faked locally.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver.session.chats.get_mut(&7).expect("chat").in_main_list = true;

    let sent = driver.mark_all_chats_as_read(false).expect("send");
    assert!(sent.is_some(), "chat 7 is unread");
    let snapshot = recorder.snapshot();
    let read_list = snapshot
        .iter()
        .find(|j| j.contains("\"readChatList\""))
        .expect("readChatList sent");
    let read_list_value: Value = serde_json::from_str(read_list).unwrap();
    assert_eq!(read_list_value["chat_list"]["@type"], "chatListMain");
    // Second call while the first is in flight is a no-op.
    assert_eq!(driver.mark_all_chats_as_read(false).expect("send"), None);
    assert_eq!(
        snapshot
            .iter()
            .filter(|j| j.contains("\"readChatList\""))
            .count(),
        1
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_mark_all_chats_as_read_noop_when_all_read() {
    // Slice CL2: no unread chats means no `readChatList` — honest
    // no-op.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    let chat = driver.session.chats.get_mut(&7).expect("chat");
    chat.in_main_list = true;
    chat.unread_count = 0;

    assert_eq!(driver.mark_all_chats_as_read(false).expect("send"), None);
    assert!(
        recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("readChatList")),
        "all-read must not send"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_clear_recently_found_chats_optimistic_clear() {
    // Slice CL2: \"Clear recents\" sends `clearRecentlyFoundChats`
    // and clears the local empty-search recents immediately (TGX
    // `SearchManager` clears locally too).
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver.session.search.search.recents = true;
    driver.session.search.search.chat_ids = vec![ChatId(7)];

    let sent = driver.clear_recently_found_chats().expect("send");
    assert!(sent.is_some(), "recents were non-empty");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"clearRecentlyFoundChats\"")),
        "clearRecentlyFoundChats sent"
    );
    assert!(driver.session.search.search.chat_ids.is_empty());
    // A refusal surfaces — the next recents fetch restores truth.
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"error","@extra":"{}","code":500,"message":"CLEAR_FAILED"}}"#,
                    sent.unwrap().0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        driver.session.chats_state.chat_action_error.as_deref(),
        Some("could not clear recent searches (error 500)")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_archive_chat_list_settings_fetch_and_set() {
    // Slice CL2: `getArchiveChatListSettings` fills the session
    // cache; a toggle sends `setArchiveChatListSettings` with the
    // three schema fields and flips optimistically.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);

    let sent = driver.fetch_archive_chat_list_settings().expect("send");
    assert!(sent.is_some(), "settings not fetched yet");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"getArchiveChatListSettings\"")),
        "getArchiveChatListSettings sent"
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"archiveChatListSettings","@extra":"{}","archive_and_mute_new_chats_from_unknown_users":true,"keep_unmuted_chats_archived":false,"keep_chats_from_folders_archived":true}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        driver
            .session
            .chat_list
            .archive_chat_list_settings
            .is_some()
    );
    assert!(!driver.session.chat_list.archive_settings_loading);

    let mut next = driver
        .session
        .chat_list
        .archive_chat_list_settings
        .expect("fetched");
    next.keep_unmuted_chats_archived = true;
    let sent = driver.set_archive_chat_list_settings(next).expect("send");
    assert!(sent.is_some(), "a field actually changed");
    let snapshot = recorder.snapshot();
    let set = snapshot
        .iter()
        .find(|j| j.contains("\"setArchiveChatListSettings\""))
        .expect("setArchiveChatListSettings sent");
    let set_value: Value = serde_json::from_str(set).unwrap();
    assert_eq!(set_value["settings"]["keep_unmuted_chats_archived"], true);
    assert!(
        driver
            .session
            .chat_list
            .archive_chat_list_settings
            .expect("cached")
            .keep_unmuted_chats_archived,
        "optimistic flip applied"
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"ARCHIVE_SETTINGS_INVALID"}}"#,
                        sent.unwrap().0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        !driver
            .session
            .chat_list
            .archive_chat_list_settings
            .expect("cached")
            .keep_unmuted_chats_archived,
        "refusal restored the old settings"
    );
    assert_eq!(
        driver.session.chats_state.chat_action_error.as_deref(),
        Some("could not save archive settings (error 400)")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_create_private_chat_answer_opens_through_select_chat() {
    // Slice CL2: the `createPrivateChat` answer is a bare `chat`
    // object (schema 1.8.67, line 13312), parsed as `UpdateNewChat`.
    // The driver opens it through the normal `select_chat` flow —
    // `openChat` + history — never by poking `session.open_chat`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver.session.my_user_id = Some(777);

    let extra = driver
        .create_private_chat_with_self()
        .expect("send")
        .expect("request sent");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"createPrivateChat\"")),
        "createPrivateChat sent"
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"chat","@extra":"{}","id":777001,"title":"Saved Messages","type":{{"@type":"chatTypePrivate","user_id":777}},"unread_count":0}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.open_chat, Some(ChatId(777001)));
    assert!(
        driver.session.chats.contains_key(&777001),
        "created chat inserted into the model"
    );
    let snapshot = recorder.snapshot();
    assert!(
        snapshot.iter().any(|j| {
            let v: Value = serde_json::from_str(j).unwrap();
            v["@type"] == "openChat" && v["chat_id"] == 777001
        }),
        "openChat sent for the created chat"
    );
    assert!(
        snapshot.iter().any(|j| {
            let v: Value = serde_json::from_str(j).unwrap();
            v["@type"] == "getChatHistory" && v["chat_id"] == 777001
        }),
        "history requested for the created chat"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl2_create_private_chat_refusal_opens_nothing() {
    // Slice CL2: a refused `createPrivateChat` must not open
    // anything — the refusal surfaces on the chat-action error
    // line instead of silently doing nothing.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    driver.session.my_user_id = Some(777);

    let extra = driver
        .create_private_chat_with_self()
        .expect("send")
        .expect("request sent");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CHAT_CREATE_FAILED"}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.open_chat, None, "nothing opened on refusal");
    assert_eq!(
        driver.session.chats_state.chat_action_error.as_deref(),
        Some("could not open Saved Messages (error 400)")
    );
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"openChat\"")),
        "no openChat sent on refusal"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cl1_remove_chat_from_list_sends_delete_history() {
    // Slice CL1: chat-list "Delete chat" is `deleteChatHistory`
    // with `remove_from_chat_list: true` (Telegram X
    // `Tdlib.deleteChat`), never the destructive `deleteChat`.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
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
                    r#"{"@type":"updateNewChat","chat":{"id":9,"title":"Old","type":{"@type":"chatTypePrivate","user_id":9},"unread_count":0,"can_be_deleted_only_for_self":true,"can_be_deleted_for_all_users":false}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver
        .remove_chat_from_list(ChatId(9))
        .expect("send")
        .expect("removable");
    let delete = recorder
        .snapshot()
        .into_iter()
        .find(|j| j.contains("deleteChatHistory"))
        .expect("deleteChatHistory sent");
    let value: Value = serde_json::from_str(&delete).unwrap();
    assert_eq!(value["@type"], "deleteChatHistory");
    assert_eq!(value["chat_id"], 9);
    assert_eq!(value["remove_from_chat_list"], true);
    assert_eq!(value["revoke"], false);
    assert!(
        !delete.contains("\"deleteChat\""),
        "must not use the destructive deleteChat constructor"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
