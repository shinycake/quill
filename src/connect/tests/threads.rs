//! Connect-driver tests: channel comments and group reply threads
//! (recorded TDLib JSON in, recorded requests out).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::ThreadStatus;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn text_message(id: i64, chat: i64, thread: i64, text: &str) -> String {
    format!(
        r#"{{"@type":"message","id":{id},"chat_id":{chat},"is_outgoing":false,"date":1700000000,"topic_id":{{"@type":"messageTopicThread","message_thread_id":{thread}}},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"{text}","entities":[]}}}}}}"#
    )
}

fn requests_of(recorder: &RecordingSender, ty: &str) -> Vec<Value> {
    recorder
        .snapshot()
        .into_iter()
        .filter_map(|json| serde_json::from_str::<Value>(&json).ok())
        .filter(|value| value["@type"] == ty)
        .collect()
}

/// Channel 13 with discussion group 14: post 101 -> `getMessageThread` ->
/// chat switch -> `getMessageThreadHistory` -> sends, typing and reads go
/// into the thread -> back.
#[test]
fn comments_thread_open_load_send_and_close() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"News chat","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"permissions":{"@type":"chatPermissions","can_send_basic_messages":true},"unread_count":0}}"#,
    ] {
        ingest(&mut driver, json);
    }
    driver.select_chat(ChatId(13)).unwrap();

    // 1. The click sends getMessageThread for the post.
    let extra = driver
        .open_thread(ChatId(13), MessageId(101))
        .unwrap()
        .expect("getMessageThread sent");
    let sent = requests_of(&recorder, "getMessageThread");
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["chat_id"], 13);
    assert_eq!(sent[0]["message_id"], 101);
    assert_eq!(sent[0]["@extra"], extra.0.to_string());
    assert_eq!(
        driver.session.threads.thread.as_ref().unwrap().status,
        ThreadStatus::Resolving
    );
    // A second click on the same post does not resend.
    assert_eq!(
        driver.open_thread(ChatId(13), MessageId(101)).unwrap(),
        None
    );

    // 2. The answer lives in the discussion group.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":501,"reply_info":{{"@type":"messageReplyInfo","reply_count":3,"recent_replier_ids":[{{"@type":"messageSenderUser","user_id":7}}],"last_read_inbox_message_id":502,"last_read_outbox_message_id":0,"last_message_id":504}},"unread_message_count":2,"messages":[{}]}}"#,
            extra.0,
            text_message(501, 14, 501, "the post")
        ),
    );
    {
        let thread = driver.session.threads.thread.as_ref().unwrap();
        assert_eq!(thread.chat_id, ChatId(14));
        assert_eq!(thread.thread_id, 501);
        assert_eq!(thread.status, ThreadStatus::LoadingHistory);
        assert!(thread.needs_chat_switch);
        assert_eq!(thread.subtitle(), "3 comments");
        assert_eq!(thread.unread_anchor, Some(MessageId(502)));
        assert_eq!(thread.root_message().unwrap().id, MessageId(501));
    }
    assert!(driver.thread_needs_start());

    // 3. Switching opens the discussion group and requests the newest page.
    let page = driver
        .switch_to_thread_chat()
        .unwrap()
        .expect("history page requested");
    assert_eq!(driver.session.open_chat, Some(ChatId(14)));
    assert_eq!(requests_of(&recorder, "closeChat").len(), 1);
    assert_eq!(
        requests_of(&recorder, "openChat").last().unwrap()["chat_id"],
        14
    );
    let history = requests_of(&recorder, "getMessageThreadHistory");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["chat_id"], 13);
    assert_eq!(history[0]["message_id"], 101);
    assert_eq!(history[0]["from_message_id"], 0);
    assert!(!driver.thread_needs_start());
    // In flight: no duplicate page.
    assert_eq!(driver.fetch_thread_history().unwrap(), None);

    // 4. The page: newest first, ending at the root.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":4,"messages":[{},{},{},{}]}}"#,
            page.0,
            text_message(504, 14, 501, "third"),
            text_message(503, 14, 501, "second"),
            text_message(502, 14, 501, "first"),
            text_message(501, 14, 501, "the post"),
        ),
    );
    {
        let thread = driver.session.threads.thread.as_ref().unwrap();
        assert_eq!(thread.status, ThreadStatus::Ready);
        assert!(thread.history.loaded_complete, "the root ends the thread");
        let ids: Vec<i64> = thread.ordered().iter().map(|m| m.id.0).collect();
        assert_eq!(ids, vec![501, 502, 503, 504]);
    }
    assert_eq!(driver.fetch_thread_history().unwrap(), None);

    // 5. A live reply in the thread appears; an unrelated message does not.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            text_message(505, 14, 501, "live")
        ),
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewMessage","message":{}}}"#,
            text_message(600, 14, 600, "elsewhere")
        ),
    );
    {
        let thread = driver.session.threads.thread.as_ref().unwrap();
        assert!(thread.history.messages.contains_key(&505));
        assert!(!thread.history.messages.contains_key(&600));
    }
    // The root's counter follows `updateMessageInteractionInfo`.
    ingest(
        &mut driver,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":13,"message_id":101,"interaction_info":{"@type":"messageInteractionInfo","view_count":9,"forward_count":0,"reply_info":{"@type":"messageReplyInfo","reply_count":4,"recent_replier_ids":[],"last_read_inbox_message_id":502,"last_read_outbox_message_id":0,"last_message_id":505},"reactions":null}}"#,
    );
    assert_eq!(
        driver.session.threads.thread.as_ref().unwrap().reply_count,
        4
    );

    // 6. Sending goes into the thread, replying to the root by default.
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(14),
        driver.session.view_generation,
        "my comment",
    );
    driver.send_text_snapshot(&snap).unwrap();
    let send = requests_of(&recorder, "sendMessage");
    let send = send.last().unwrap();
    assert_eq!(send["chat_id"], 14);
    assert_eq!(send["topic_id"]["@type"], "messageTopicThread");
    assert_eq!(send["topic_id"]["message_thread_id"], 501);
    assert_eq!(send["reply_to"]["@type"], "inputMessageReplyToMessage");
    assert_eq!(send["reply_to"]["message_id"], 501);

    // 7. Typing goes into the thread.
    driver
        .sync_outgoing_typing("typing", false, 10_000)
        .unwrap();
    let action = requests_of(&recorder, "sendChatAction");
    let action = action.last().unwrap();
    assert_eq!(action["chat_id"], 14);
    assert_eq!(action["topic_id"]["message_thread_id"], 501);

    // 8. Rows on screen are read as thread history.
    driver
        .view_messages(ChatId(14), &[MessageId(503), MessageId(504)])
        .unwrap();
    let view = requests_of(&recorder, "viewMessages");
    let view = view.last().unwrap();
    assert_eq!(view["chat_id"], 14);
    assert_eq!(view["source"]["@type"], "messageSourceMessageThreadHistory");
    assert_eq!(view["force_read"], true);
    assert_eq!(view["message_ids"], serde_json::json!([503, 504]));

    // 9. Back returns to the channel and drops the thread.
    assert_eq!(driver.close_thread(), Some(ChatId(13)));
    assert!(driver.session.threads.thread.is_none());
    driver.select_chat(ChatId(13)).unwrap();
    assert_eq!(driver.session.open_chat, Some(ChatId(13)));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A group message's replies are a thread in the same chat: no chat switch,
/// older pages load until the root, and a failure is shown with a retry.
#[test]
fn group_reply_thread_pages_older_and_fails_with_retry() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":false},"unread_count":0}}"#,
    );
    driver.select_chat(ChatId(14)).unwrap();

    // First attempt fails.
    let extra = driver
        .open_thread(ChatId(14), MessageId(40))
        .unwrap()
        .unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":400,"message":"MESSAGE_THREAD_NOT_FOUND"}}"#,
            extra.0
        ),
    );
    assert!(matches!(
        driver.session.threads.thread.as_ref().unwrap().status,
        ThreadStatus::Failed(_)
    ));
    let extra = driver.retry_thread().unwrap().unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":14,"message_thread_id":40,"reply_info":{{"@type":"messageReplyInfo","reply_count":1,"recent_replier_ids":[],"last_read_inbox_message_id":0,"last_read_outbox_message_id":0,"last_message_id":90}},"unread_message_count":0,"messages":[{}]}}"#,
            extra.0,
            text_message(40, 14, 40, "root")
        ),
    );
    let thread = driver.session.threads.thread.as_ref().unwrap();
    assert!(!thread.needs_chat_switch, "same chat");
    assert!(!thread.is_comments());
    assert_eq!(thread.subtitle(), "1 reply");
    let page = driver
        .start_thread_in_open_chat()
        .unwrap()
        .expect("first page");
    // A page of replies that does not reach the root keeps paging.
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":2,"messages":[{},{}]}}"#,
            page.0,
            text_message(90, 14, 40, "newest"),
            text_message(80, 14, 40, "older"),
        ),
    );
    assert!(
        !driver
            .session
            .threads
            .thread
            .as_ref()
            .unwrap()
            .history
            .loaded_complete
    );
    let older = driver.fetch_thread_history().unwrap().expect("older page");
    let history = requests_of(&recorder, "getMessageThreadHistory");
    assert_eq!(history.last().unwrap()["from_message_id"], 80);
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messages","@extra":"{}","total_count":3,"messages":[{}]}}"#,
            older.0,
            text_message(80, 14, 40, "older"),
        ),
    );
    assert!(
        driver
            .session
            .threads
            .thread
            .as_ref()
            .unwrap()
            .history
            .loaded_complete
    );
    // Closing in the same chat needs no chat switch.
    assert_eq!(driver.close_thread(), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A thread opened from a forum topic stays in the topic: sends keep
/// `messageTopicForum` and reply to the thread root.
#[test]
fn forum_topic_thread_routes_sends_into_the_topic() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &dyn_sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
    );
    driver.select_chat(ChatId(16)).unwrap();
    driver.session.select_topic(ChatId(16), 7);
    let extra = driver
        .open_thread(ChatId(16), MessageId(120))
        .unwrap()
        .unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"messageThreadInfo","@extra":"{}","chat_id":16,"message_thread_id":120,"reply_info":null,"unread_message_count":0,"messages":[{}]}}"#,
            extra.0,
            text_message(120, 16, 120, "root")
        ),
    );
    driver.start_thread_in_open_chat().unwrap();
    assert_eq!(
        driver
            .session
            .threads
            .thread
            .as_ref()
            .unwrap()
            .forum_topic_id,
        Some(7)
    );
    let send = crate::telegram::requests::send_text(
        crate::ids::RequestId(9),
        ChatId(16),
        Some(7),
        "hi",
        None,
        &crate::composer::SendOptions::default(),
    );
    let routed: Value = serde_json::from_str(&driver.thread_routed(ChatId(16), send)).unwrap();
    assert_eq!(routed["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(routed["topic_id"]["forum_topic_id"], 7);
    assert_eq!(routed["reply_to"]["message_id"], 120);
    let _ = std::fs::remove_dir_all(&dir);
}
