//! Driver tests: the chat-row "Mark as read" reads the whole chat.
use super::*;

fn sent_of(snapshot: &[String], method: &str) -> Vec<Value> {
    let tag = format!("\"{method}\"");
    snapshot
        .iter()
        .filter(|j| j.contains(&tag))
        .filter_map(|j| serde_json::from_str(j).ok())
        .collect()
}

#[test]
fn mark_as_read_reads_till_the_last_message_not_the_loaded_page() {
    // A chat opened at its first unread holds an older page (here only
    // message 11) while the chat's last message is 40. Reading till the
    // page's end left everything after it unread.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    for update in [
        r#"{"@type":"updateChatLastMessage","chat_id":7,"last_message":{"id":40,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"late","entities":[]}}},"positions":[]}"#,
        r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":5,"unread_count":30}"#,
        r#"{"@type":"updateChatUnreadMentionCount","chat_id":7,"unread_mention_count":2}"#,
        r#"{"@type":"updateChatUnreadReactionCount","chat_id":7,"unread_reaction_count":1}"#,
    ] {
        driver
            .ingest(copy_and_parse(update, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // The open chat's own on-screen view is in flight: it must not
    // swallow the whole-chat read.
    driver
        .session
        .request(RequestPurpose::ViewMessages, Some(ChatId(7)));
    let before = recorder.snapshot().len();

    let sent = driver.mark_chat_as_read(ChatId(7)).expect("send");
    assert!(sent.is_some());
    let snapshot = recorder.snapshot().split_off(before);
    let views = sent_of(&snapshot, "viewMessages");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["message_ids"], serde_json::json!([40]));
    assert_eq!(views[0]["source"]["@type"], "messageSourceChatList");
    assert_eq!(views[0]["force_read"], true);
    assert_eq!(sent_of(&snapshot, "readAllChatMentions").len(), 1);
    assert_eq!(sent_of(&snapshot, "readAllChatReactions").len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mark_as_read_clears_mentions_of_a_read_chat() {
    // Nothing unread but a mention badge left: the badge still clears,
    // and no read is sent for messages already read.
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = ViewCtlSender::new();
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    ready_private_chat(&mut driver, &seq, &dyn_sink);
    for update in [
        r#"{"@type":"updateChatReadInbox","chat_id":7,"last_read_inbox_message_id":11,"unread_count":0}"#,
        r#"{"@type":"updateChatUnreadMentionCount","chat_id":7,"unread_mention_count":1}"#,
    ] {
        driver
            .ingest(copy_and_parse(update, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    let before = recorder.snapshot().len();

    driver.mark_chat_as_read(ChatId(7)).expect("send");
    let snapshot = recorder.snapshot().split_off(before);
    assert!(sent_of(&snapshot, "viewMessages").is_empty());
    assert_eq!(sent_of(&snapshot, "readAllChatMentions").len(), 1);
    assert!(sent_of(&snapshot, "readAllChatReactions").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
