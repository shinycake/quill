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

mod export;
mod list_ops;

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
    assert_eq!(
        driver.session.threads.forum_topics.get(&16).unwrap().len(),
        1
    );
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
    let history = driver
        .session
        .threads
        .topic_histories
        .get(&(16, 2))
        .unwrap();
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
fn view_messages_reports_only_what_is_shown() {
    // Opening a chat views only its newest message (Telegram X opens at the
    // bottom and its viewport reports what is on screen); older loaded rows
    // are viewed only when the UI reports them through `view_messages`.
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    let views = |recorder: &RecordingSender| -> Vec<(String, Vec<i64>)> {
        recorder
            .snapshot()
            .iter()
            .filter_map(|j| serde_json::from_str::<Value>(j).ok())
            .filter(|v| v["@type"] == "viewMessages")
            .map(|v| {
                assert_eq!(v["force_read"], true);
                let ids = v["message_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_i64().unwrap())
                    .collect();
                (v["@extra"].as_str().unwrap().to_string(), ids)
            })
            .collect()
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":3}}"#,
    );
    let history = driver
        .select_chat(ChatId(7))
        .unwrap()
        .expect("history request");
    let row = |id: i64| {
        format!(
            r#"{{"id":{id},"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"m","entities":[]}}}}}}"#
        )
    };
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{},{},{}]}}"#,
            history.0,
            row(12),
            row(11),
            row(10)
        ),
    );
    let sent = views(&recorder);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1, vec![12], "open views the newest message only");

    // The UI reports rows on screen while the first view is in flight:
    // they wait for it, then go out together.
    assert_eq!(
        driver
            .view_messages(ChatId(7), &[MessageId(10), MessageId(11)])
            .unwrap(),
        None
    );
    assert_eq!(views(&recorder).len(), 1);
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, sent[0].0),
    );
    let sent = views(&recorder);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].1, vec![10, 11]);
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"ok","@extra":"{}"}}"#, sent[1].0),
    );

    // Already viewed rows are not re-sent; once the UI reports, a new
    // incoming message waits until the UI shows it.
    assert_eq!(
        driver
            .view_messages(ChatId(7), &[MessageId(11), MessageId(12)])
            .unwrap(),
        None
    );
    ingest(
        &mut driver,
        &format!(r#"{{"@type":"updateNewMessage","message":{}}}"#, row(13)),
    );
    assert_eq!(views(&recorder).len(), 2);
    assert!(
        driver
            .view_messages(ChatId(7), &[MessageId(13)])
            .unwrap()
            .is_some()
    );
    assert_eq!(views(&recorder)[2].1, vec![13]);
    // Reports for a chat that is not open are ignored.
    assert_eq!(
        driver.view_messages(ChatId(8), &[MessageId(1)]).unwrap(),
        None
    );
    let _ = std::fs::remove_dir_all(&dir);
}
