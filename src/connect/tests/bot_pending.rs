use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, unix_ms_now};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn bot_stream_stop_races_retention_topics_and_expiry() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: serde_json::Value| {
        driver
            .ingest(copy_and_parse(&payload.to_string(), &seq, &sink).unwrap())
            .unwrap();
    };
    let draft = |id: i64, topic: i32, keep: bool| {
        json!({
            "@type":"updatePendingMessage", "chat_id":7, "forum_topic_id":topic,
            "draft_id":id.to_string(), "can_stop":true, "keep_on_stop":keep,
            "content":{"@type":"messageText", "text":{"text":"Thinking about this…", "entities":[]}}
        })
    };
    ingest(
        &mut driver,
        json!({"@type":"updateOption", "name":"pending_text_message_period",
        "value":{"@type":"optionValueInteger", "value":"2"}}),
    );
    ingest(&mut driver, draft(i64::MAX, 42, false));
    assert_eq!(driver.session.pending_bot_period_secs, 2);
    assert!(driver.session.pending_bot_messages[&(7, 42)].expires_at_ms <= unix_ms_now() + 2000);
    let request = driver
        .stop_pending_bot_message(ChatId(7), 42, i64::MAX)
        .unwrap()
        .unwrap();
    let sent = sent_request(&recorder, "stopPendingMessage");
    assert_eq!(sent["draft_id"], i64::MAX.to_string());
    assert_eq!(sent["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(sent["topic_id"]["forum_topic_id"], 42);
    assert!(
        driver
            .stop_pending_bot_message(ChatId(7), 42, i64::MAX)
            .unwrap()
            .is_none()
    );
    assert!(driver.session.pending_bot_messages.contains_key(&(7, 42))); // No optimistic deletion.
    ingest(
        &mut driver,
        json!({"@type":"error", "@extra":request.as_extra(), "code":500, "message":"test"}),
    );
    assert!(driver.session.pending_bot_messages[&(7, 42)].stop_failed);
    let old_stop = driver
        .stop_pending_bot_message(ChatId(7), 42, i64::MAX)
        .unwrap()
        .unwrap();
    ingest(&mut driver, draft(12, 42, true));
    ingest(
        &mut driver,
        json!({"@type":"ok", "@extra":old_stop.as_extra()}),
    );
    assert!(driver.session.pending_bot_messages[&(7, 42)].can_stop); // Older draft ACK cannot stop this one.
    let stop = driver
        .stop_pending_bot_message(ChatId(7), 42, 12)
        .unwrap()
        .unwrap();
    ingest(&mut driver, json!({"@type":"ok", "@extra":stop.as_extra()}));
    assert!(!driver.session.pending_bot_messages[&(7, 42)].can_stop);
    assert!(driver.session.pending_bot_messages[&(7, 42)].stopped);
    ingest(&mut driver, draft(12, 42, true));
    assert!(!driver.session.pending_bot_messages[&(7, 42)].can_stop);
    assert!(
        driver
            .stop_pending_bot_message(ChatId(7), 42, 12)
            .unwrap()
            .is_none()
    );
    ingest(&mut driver, draft(13, 0, false));
    let stop = driver
        .stop_pending_bot_message(ChatId(7), 0, 13)
        .unwrap()
        .unwrap();
    assert!(sent_request(&recorder, "stopPendingMessage")["topic_id"].is_null());
    ingest(&mut driver, json!({"@type":"ok", "@extra":stop.as_extra()}));
    assert!(!driver.session.pending_bot_messages.contains_key(&(7, 0)));
    assert!(driver.session.pending_bot_messages.contains_key(&(7, 42)));
    // A final incoming message clears only the matching thread's ephemeral draft.
    ingest(
        &mut driver,
        json!({"@type":"updateNewMessage", "message":{"id":99, "chat_id":7,
        "topic_id":{"@type":"messageTopicForum","forum_topic_id":42}, "is_outgoing":false,
        "content":{"@type":"messageText", "text":{"text":"Done", "entities":[]}}}}),
    );
    assert!(!driver.session.pending_bot_messages.contains_key(&(7, 42)));
    ingest(&mut driver, draft(14, 42, false));
    ingest(
        &mut driver,
        json!({"@type":"updateStopMessageDraft", "chat_id":7, "forum_topic_id":42, "draft_id":"13"}),
    );
    assert!(driver.session.pending_bot_messages.contains_key(&(7, 42)));
    ingest(
        &mut driver,
        json!({"@type":"updateStopMessageDraft", "chat_id":7, "forum_topic_id":42, "draft_id":"14"}),
    );
    assert!(!driver.session.pending_bot_messages.contains_key(&(7, 42)));
    ingest(&mut driver, draft(15, 0, true));
    assert!(
        driver
            .session
            .expire_pending_bot_messages(unix_ms_now() + 2001)
    );
    assert!(driver.session.pending_bot_messages.is_empty());
    assert!(
        driver
            .stop_pending_bot_message(ChatId(7), 0, 15)
            .unwrap()
            .is_none()
    );
    let mut no_stop = draft(16, 0, false);
    no_stop["can_stop"] = json!(false);
    ingest(&mut driver, no_stop);
    assert!(!driver.session.pending_bot_messages[&(7, 0)].stopped);
    assert!(
        driver
            .stop_pending_bot_message(ChatId(7), 0, 16)
            .unwrap()
            .is_none()
    );
    let mut malformed = draft(17, 0, false);
    malformed["forum_topic_id"] = json!(i64::MAX);
    assert!(copy_and_parse(&malformed.to_string(), &seq, &sink).is_none());
    driver.session.auth = AuthorizationState::WaitPhoneNumber;
    assert!(driver.stop_pending_bot_message(ChatId(7), 0, 15).is_err());
    assert!(
        !driver
            .session
            .requests
            .has_purpose(RequestPurpose::StopPendingMessage {
                topic_id: 0,
                draft_id: 15
            })
    );
    let _ = std::fs::remove_dir_all(dir);
}
