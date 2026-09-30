//! Connect-driver tests: selection, forums, read/pin/archive state.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn forum_flow_topics_then_topic_history() {
    // Phase 5.1: selecting a forum supergroup resolves `is_forum` via
    // `getSupergroup`, loads `getForumTopics`, and selecting a topic
    // fetches its history with `searchChatMessages` + `messageTopicForum`.
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
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Forum status unknown: selecting the chat fires `getSupergroup`.
    driver.select_chat(ChatId(16)).unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.iter().any(
            |j| j.contains("\"@type\":\"getSupergroup\"") && j.contains("\"supergroup_id\":16")
        )
    );
    assert!(
        !sent
            .iter()
            .any(|j| j.contains("\"@type\":\"getForumTopics\""))
    );
    // The response resolves `is_forum`; re-selecting loads the topics.
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetSupergroup, Some(ChatId(16)))
        .expect("getSupergroup in flight");
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"supergroup","@extra":"{}","id":16,"is_forum":true}}"#,
                    extra.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert!(driver.session.chats.get(&16).unwrap().is_forum_chat());
    driver.select_chat(ChatId(16)).unwrap();
    let sent = recorder.snapshot();
    let topics_extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetForumTopics, Some(ChatId(16)))
        .expect("getForumTopics in flight");
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"getForumTopics\"") && j.contains("\"chat_id\":16"))
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"forumTopics","@extra":"{}","total_count":1,"topics":[{{"info":{{"@type":"forumTopicInfo","chat_id":16,"forum_topic_id":2,"name":"Random","icon":{{"@type":"forumTopicIcon","color":0,"custom_emoji_id":"0"}},"creation_date":1,"creator_id":{{"@type":"messageSenderUser","user_id":6}},"is_general":false,"is_outgoing":false,"is_closed":false,"is_hidden":false,"is_name_implicit":false}},"last_message":null,"order":"100","is_pinned":false,"unread_count":0,"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"unread_mention_count":0,"unread_reaction_count":0,"unread_poll_vote_count":0,"notification_settings":{{"@type":"chatNotificationSettings"}},"draft_message":null}}],"next_offset_date":0,"next_offset_message_id":0,"next_offset_forum_topic_id":0}}"#,
                        topics_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(driver.session.forum_topics.get(&16).unwrap().len(), 1);
    // Selecting the topic fetches per-topic history.
    driver.select_topic(2).unwrap();
    assert_eq!(driver.session.open_topic, Some(2));
    let sent = recorder.snapshot();
    let search_json = sent
        .iter()
        .find(|j| {
            j.contains("\"@type\":\"searchChatMessages\"")
                && j.contains("\"query\":\"\"")
                && j.contains("\"messageTopicForum\"")
        })
        .expect("topic searchChatMessages sent");
    assert!(search_json.contains("\"forum_topic_id\":2"));
    // The response populates the topic history.
    let extra = driver
        .session
        .requests
        .pending_extra_for(RequestPurpose::GetTopicHistory, Some(ChatId(16)))
        .expect("GetTopicHistory in flight");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"foundChatMessages","@extra":"{}","total_count":1,"next_from_message_id":0,"messages":[{{"id":50,"chat_id":16,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"CANARY_DRV_topic","entities":[]}}}}}}]}}"#,
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    let history = driver.session.topic_histories.get(&(16, 2)).unwrap();
    assert!(history.loaded_complete);
    assert!(history.messages.contains_key(&50));
    // Back to the topic list.
    driver.deselect_topic();
    assert_eq!(driver.session.open_topic, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn send_text_rejected_when_empty_or_no_open_chat() {
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
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(1),
        driver.session.view_generation,
        "   ",
    );
    assert_eq!(
        driver.send_text_snapshot(&snap),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

impl JsonSender for Arc<ViewCtlSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if request.contains("viewMessages") && *self.fail_view.lock().expect("view ctl sender") {
            return Err(ConnectSendError::Native);
        }
        self.sent
            .lock()
            .expect("view ctl sender")
            .push(request.to_string());
        Ok(())
    }
}

#[test]
fn view_messages_send_failure_unsticks_gate_and_retries() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let sender = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);

    sender.set_fail_view(true);
    assert_eq!(driver.select_chat(ChatId(7)), Err(ConnectSendError::Native));
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::ViewMessages)
    );
    assert_eq!(
        driver.session.message_ids_to_view(ChatId(7)),
        vec![MessageId(11)]
    );
    assert_eq!(sender.view_count(), 0);

    sender.set_fail_view(false);
    let extra = driver
        .maybe_view_open_messages()
        .unwrap()
        .expect("retry viewMessages");
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
    );
    assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
    assert_eq!(sender.view_count(), 1);
    assert!(
        sender
            .snapshot()
            .last()
            .unwrap()
            .contains(&format!("\"@extra\":\"{}\"", extra.0))
    );
    assert!(!sink.rendered().contains("hi"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn view_messages_tdlib_error_unsticks_gate_and_retries() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let sender = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);

    let _history = driver
        .select_chat(ChatId(7))
        .unwrap()
        .expect("history after view");
    assert_eq!(sender.view_count(), 1);
    let view_extra = sender
        .snapshot()
        .iter()
        .find(|j| j.contains("viewMessages"))
        .and_then(|j| serde_json::from_str::<Value>(j).ok())
        .and_then(|v| v["@extra"].as_str().map(str::to_string))
        .expect("view extra");
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
    );
    assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());

    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"error","code":400,"message":"CANARY_VIEW_TD","@extra":"{view_extra}"}}"#
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        sender.view_count(),
        2,
        "TDLib error must retry viewMessages"
    );
    let retry_extra = sender
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("viewMessages"))
        .and_then(|j| serde_json::from_str::<Value>(&j).ok())
        .and_then(|v| v["@extra"].as_str().map(str::to_string))
        .expect("retry extra");
    assert_ne!(
        retry_extra, view_extra,
        "retry must not reuse the failed extra"
    );
    assert!(driver.session.message_ids_to_view(ChatId(7)).is_empty());
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ViewMessages, ChatId(7))
    );
    assert!(!sink.rendered().contains("CANARY_VIEW_TD"));
    let _ = std::fs::remove_dir_all(&dir);
}

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
        driver.session.chat_action_error.as_deref(),
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
    driver.session.search.recents = true;
    driver.session.search.chat_ids = vec![ChatId(7)];

    let sent = driver.clear_recently_found_chats().expect("send");
    assert!(sent.is_some(), "recents were non-empty");
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|j| j.contains("\"clearRecentlyFoundChats\"")),
        "clearRecentlyFoundChats sent"
    );
    assert!(driver.session.search.chat_ids.is_empty());
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
        driver.session.chat_action_error.as_deref(),
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
    assert!(driver.session.archive_chat_list_settings.is_some());
    assert!(!driver.session.archive_settings_loading);

    let mut next = driver.session.archive_chat_list_settings.expect("fetched");
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
            .archive_chat_list_settings
            .expect("cached")
            .keep_unmuted_chats_archived,
        "refusal restored the old settings"
    );
    assert_eq!(
        driver.session.chat_action_error.as_deref(),
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
        driver.session.chat_action_error.as_deref(),
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

#[test]
fn chat_export_pages_history_until_short_page() {
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
        .start_chat_export(ChatId(16), "Export Chat".into())
        .unwrap();
    // A second export while one is running is refused.
    assert!(
        driver
            .start_chat_export(ChatId(16), "Export Chat".into())
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
    // inclusive from_message_id, then 900) → boundary deduped, paging done.
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
    assert!(export.done_paging);
    let _ = std::fs::remove_dir_all(&dir);
}
