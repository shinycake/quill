use super::super::{ConnectDriver, RecordingSender};
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn text_with_emoji(id: i64, custom_emoji_id: i64) -> serde_json::Value {
    json!({"@type":"updateNewMessage","message":{"id":id,"chat_id":4,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi 😀","entities":[{"@type":"textEntity","offset":3,"length":2,"type":{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":custom_emoji_id.to_string()}}]}}}})
}

/// Inside a poll batch the open chat's custom emoji are looked at once, at
/// the end; the request names every id the batch brought.
#[test]
fn a_poll_batch_resolves_open_chat_custom_emoji_once() {
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
    let resolve_requests = || {
        recorder
            .snapshot()
            .iter()
            .filter(|request| request.contains("getCustomEmojiStickers"))
            .count()
    };
    driver.session.open_chat = Some(ChatId(4));
    driver.begin_batch();
    ingest(&mut driver, text_with_emoji(9, 12345));
    ingest(&mut driver, text_with_emoji(10, 12346));
    assert_eq!(resolve_requests(), 0);
    driver.end_batch().unwrap();
    assert_eq!(resolve_requests(), 1);
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["12345", "12346"])
    );
    // An empty batch asks for nothing more.
    driver.begin_batch();
    driver.end_batch().unwrap();
    assert_eq!(resolve_requests(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// File progress, presence, typing and read receipts cannot show a new
/// custom emoji, so a batch of them leaves the open chat's alone; the next
/// message still resolves every id the chat shows.
#[test]
fn a_batch_of_file_and_presence_updates_leaves_custom_emoji_alone() {
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
    let resolve_requests = || {
        recorder
            .snapshot()
            .iter()
            .filter(|request| request.contains("getCustomEmojiStickers"))
            .count()
    };
    // The message lands while its chat is closed; the chat opens after.
    ingest(&mut driver, text_with_emoji(9, 12345));
    driver.session.open_chat = Some(ChatId(4));
    driver.begin_batch();
    ingest(
        &mut driver,
        json!({"@type":"updateFile","file":{"@type":"file","id":77,"size":100,"expected_size":100,"local":{"@type":"localFile","path":"","is_downloading_active":true,"is_downloading_completed":false,"downloaded_size":10},"remote":{"@type":"remoteFile","id":"r","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}}),
    );
    ingest(
        &mut driver,
        json!({"@type":"updateUserStatus","user_id":5,"status":{"@type":"userStatusOnline","expires":100}}),
    );
    ingest(
        &mut driver,
        json!({"@type":"updateChatAction","chat_id":4,"message_thread_id":0,"sender_id":{"@type":"messageSenderUser","user_id":5},"action":{"@type":"chatActionTyping"}}),
    );
    ingest(
        &mut driver,
        json!({"@type":"updateChatReadOutbox","chat_id":4,"last_read_outbox_message_id":9}),
    );
    driver.end_batch().unwrap();
    assert_eq!(resolve_requests(), 0);
    driver.begin_batch();
    ingest(&mut driver, text_with_emoji(10, 12346));
    driver.end_batch().unwrap();
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["12345", "12346"])
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// An account not on screen leaves its custom emoji for later; the first
/// batch once it is shown resolves them, even with nothing new in it.
#[test]
fn an_offscreen_batch_resolves_custom_emoji_once_shown() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let resolve_requests = || {
        recorder
            .snapshot()
            .iter()
            .filter(|request| request.contains("getCustomEmojiStickers"))
            .count()
    };
    driver.session.open_chat = Some(ChatId(4));
    driver.begin_batch();
    driver
        .ingest(copy_and_parse(&text_with_emoji(9, 12345).to_string(), &seq, &sink).unwrap())
        .unwrap();
    driver.end_offscreen_batch();
    assert_eq!(resolve_requests(), 0);
    driver.begin_batch();
    driver.end_batch().unwrap();
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["12345"])
    );
    let _ = std::fs::remove_dir_all(dir);
}
