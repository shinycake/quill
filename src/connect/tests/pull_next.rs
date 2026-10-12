//! Connect-driver tests: preloading the next unread channel while the user
//! pulls toward it (`pull_next`).
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// `getChatHistory` around the read position (50 messages, 25 newer) and
/// the pinned messages; the answers mark the chat ready without opening
/// it, and a second pull while the page is in flight sends nothing new.
#[test]
fn preload_loads_history_around_the_read_position_and_pins() {
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
        r#"{"@type":"updateNewChat","chat":{"id":14,"title":"Next","type":{"@type":"chatTypeSupergroup","supergroup_id":14,"is_channel":true},"unread_count":3,"last_read_inbox_message_id":500}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":15,"title":"Read","type":{"@type":"chatTypeSupergroup","supergroup_id":15,"is_channel":true},"unread_count":0,"last_read_inbox_message_id":700}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    let next = ChatId(14);
    driver.preload_next_channel(next).unwrap();
    driver.preload_next_channel(next).unwrap();
    let sent: Vec<Value> = recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
    let history: Vec<&Value> = sent
        .iter()
        .filter(|json| json["@type"] == "getChatHistory")
        .collect();
    assert_eq!(history.len(), 1, "deduped while in flight");
    assert_eq!(history[0]["chat_id"], 14);
    assert_eq!(history[0]["from_message_id"], 500);
    assert_eq!(history[0]["offset"], -25);
    assert_eq!(history[0]["limit"], 50);
    assert_eq!(history[0]["only_local"], false);
    let pinned: Vec<&Value> = sent
        .iter()
        .filter(|json| {
            json["@type"] == "searchChatMessages"
                && json["filter"]["@type"] == "searchMessagesFilterPinned"
        })
        .collect();
    assert_eq!(pinned.len(), 1);
    assert!(!sent.iter().any(|json| json["@type"] == "openChat"));
    assert!(!driver.next_channel_history_ready(next));
    assert!(!driver.next_channel_pinned_ready(next));

    let answer = |extra: &Value, body: &str| {
        format!(
            r#"{{"@type":"{body}","@extra":"{}","total_count":1,"messages":[{{"id":501,"chat_id":14,"is_outgoing":false,"date":1,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"post","entities":[]}}}}}}],"next_from_message_id":0}}"#,
            extra.as_str().unwrap()
        )
    };
    let history_answer = answer(&history[0]["@extra"], "messages");
    driver
        .ingest(copy_and_parse(&history_answer, &seq, &dyn_sink).unwrap())
        .unwrap();
    let pinned_answer = answer(&pinned[0]["@extra"], "foundChatMessages");
    driver
        .ingest(copy_and_parse(&pinned_answer, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert!(driver.next_channel_history_ready(next));
    assert!(driver.next_channel_pinned_ready(next));
    // The chat was not opened, and its page is not shown anywhere.
    assert_eq!(driver.session.open_chat, None);
    assert!(
        driver
            .session
            .histories
            .get(&14)
            .is_none_or(|h| h.messages.is_empty())
    );

    // Everything read: the newest page, no offset.
    driver.preload_next_channel(ChatId(15)).unwrap();
    let last: Value = serde_json::from_str(
        recorder
            .snapshot()
            .iter()
            .rev()
            .find(|json| json.contains("getChatHistory"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(last["chat_id"], 15);
    assert_eq!(last["from_message_id"], 0);
    assert_eq!(last["offset"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}
