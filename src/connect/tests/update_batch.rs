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

/// An account not on screen leaves its open chat's custom emoji, views and
/// media for later; the first batch once it is shown sends them all, even
/// with nothing new in it.
#[test]
fn an_offscreen_batch_runs_the_shown_chat_passes_once_shown() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":4,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":4},"unread_count":0}}"#,
    );
    driver.select_chat(ChatId(4)).unwrap();
    driver.begin_batch();
    ingest(&mut driver, &text_with_emoji(9, 12345).to_string());
    ingest(&mut driver, &photo_message(10, 4, 1, 2));
    driver.end_offscreen_batch();
    assert!(sent_of_type(&recorder, "getCustomEmojiStickers").is_empty());
    assert!(sent_of_type(&recorder, "downloadFile").is_empty());
    assert!(sent_of_type(&recorder, "viewMessages").is_empty());
    driver.begin_batch();
    driver.end_batch().unwrap();
    assert_eq!(
        sent_request(&recorder, "viewMessages")["message_ids"],
        json!([10])
    );
    assert_eq!(
        sent_request(&recorder, "getCustomEmojiStickers")["custom_emoji_ids"],
        json!(["12345"])
    );
    let mut downloads: Vec<i64> = sent_of_type(&recorder, "downloadFile")
        .iter()
        .filter_map(|request| request["file_id"].as_i64())
        .collect();
    downloads.sort_unstable();
    assert_eq!(downloads, vec![1, 2]);
    let _ = std::fs::remove_dir_all(dir);
}

fn photo_file(id: i32, size: i32) -> String {
    format!(
        r#"{{"@type":"file","id":{id},"size":{size},"expected_size":{size},"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":{size}}}}}"#
    )
}

fn photo_message(id: i64, chat_id: i64, thumb: i32, full: i32) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}}}"#,
        photo_file(thumb, 10),
        photo_file(full, 80),
    )
}

/// A storm of file progress looks at the open chat's media once per batch,
/// and still sends the downloads a photo in that batch needs. A photo in a
/// chat nobody shows needs no look at all.
#[test]
fn a_batch_of_file_updates_looks_at_shown_media_once() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    let downloads = || {
        let mut files: Vec<i64> = recorder
            .snapshot()
            .iter()
            .filter(|request| request.contains("\"downloadFile\""))
            .filter_map(|request| serde_json::from_str::<serde_json::Value>(request).ok())
            .filter_map(|request| request["file_id"].as_i64())
            .collect();
        files.sort_unstable();
        files
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    driver.select_chat(ChatId(7)).unwrap();
    let before = driver.batch.media_passes;
    driver.begin_batch();
    ingest(&mut driver, &photo_message(20, 7, 1, 2));
    for size in 0..100 {
        ingest(
            &mut driver,
            &format!(
                r#"{{"@type":"updateFile","file":{}}}"#,
                photo_file(50, size)
            ),
        );
    }
    driver.end_batch().unwrap();
    assert_eq!(driver.batch.media_passes, before + 1);
    assert_eq!(downloads(), vec![1, 2]);
    driver.begin_batch();
    ingest(&mut driver, &photo_message(30, 8, 3, 4));
    driver.end_batch().unwrap();
    assert_eq!(driver.batch.media_passes, before + 1);
    assert_eq!(downloads(), vec![1, 2]);
    let _ = std::fs::remove_dir_all(dir);
}

fn emoji_file(id: i32, active: bool) -> String {
    format!(
        r#"{{"@type":"updateFile","file":{{"@type":"file","id":{id},"size":100,"expected_size":100,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":10}},"remote":{{"@type":"remoteFile","id":"r{id}","unique_id":"u{id}","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":100}}}}}}"#
    )
}

/// A reaction's custom emoji whose file TDLib was already fetching (a
/// resume this session never asked for) is downloaded once that transfer
/// stops without completing, with nothing else arriving after it.
#[test]
fn a_refused_custom_emoji_file_downloads_once_its_transfer_stops() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    let emoji_downloads = || {
        recorder
            .snapshot()
            .iter()
            .filter_map(|request| serde_json::from_str::<serde_json::Value>(request).ok())
            .filter(|request| request["@type"] == "downloadFile" && request["file_id"] == 88)
            .count()
    };
    driver.session.stickers.emoji.custom_emoji_stickers.push(
        crate::telegram::envelope::StickerItem {
            custom_emoji_id: Some(777),
            id: 88,
            set_id: 1,
            emoji: String::new(),
            width: 100,
            height: 100,
            format: crate::telegram::envelope::StickerFormat::Webp,
            file_id: crate::ids::FileId(88),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: false,
            premium_animation: None,
        },
    );
    driver.session.open_chat = Some(ChatId(4));
    ingest(&mut driver, &emoji_file(88, true));
    driver.begin_batch();
    ingest(&mut driver, &text_with_emoji(9, 12345).to_string());
    ingest(
        &mut driver,
        r#"{"@type":"updateMessageInteractionInfo","chat_id":4,"message_id":9,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reply_info":null,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeCustomEmoji","custom_emoji_id":"777"},"total_count":2,"is_chosen":false,"used_sender_id":null,"recent_sender_ids":[]}],"are_tags":false,"paid_reactors":[],"can_get_added_reactions":false}}}"#,
    );
    driver.end_batch().unwrap();
    assert_eq!(emoji_downloads(), 0);
    driver.begin_batch();
    ingest(&mut driver, &emoji_file(88, false));
    driver.end_batch().unwrap();
    assert_eq!(emoji_downloads(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

fn text_message(id: i64, chat_id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":{chat_id},"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}}}"#
    )
}

fn sent_of_type(recorder: &RecordingSender, type_name: &str) -> Vec<serde_json::Value> {
    recorder
        .snapshot()
        .iter()
        .filter_map(|request| serde_json::from_str::<serde_json::Value>(request).ok())
        .filter(|request| request["@type"] == type_name)
        .collect()
}

/// New messages in the open chat are marked viewed once per batch, at its
/// end, up to the newest; a message in a chat nobody shows marks nothing.
#[test]
fn a_batch_views_the_open_chat_new_messages_once() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    for chat in [7, 8] {
        ingest(
            &mut driver,
            &format!(
                r#"{{"@type":"updateNewChat","chat":{{"id":{chat},"title":"Alice","type":{{"@type":"chatTypePrivate","user_id":{chat}}},"unread_count":0}}}}"#
            ),
        );
    }
    driver.select_chat(ChatId(7)).unwrap();
    let before = sent_of_type(&recorder, "viewMessages").len();
    driver.begin_batch();
    ingest(&mut driver, &text_message(20, 7));
    ingest(&mut driver, &text_message(21, 7));
    assert_eq!(sent_of_type(&recorder, "viewMessages").len(), before);
    driver.end_batch().unwrap();
    let views = sent_of_type(&recorder, "viewMessages");
    assert_eq!(views.len(), before + 1);
    assert_eq!(views[before]["chat_id"], 7);
    assert_eq!(views[before]["message_ids"], json!([21]));
    driver.begin_batch();
    ingest(&mut driver, &text_message(30, 8));
    driver.end_batch().unwrap();
    assert_eq!(sent_of_type(&recorder, "viewMessages").len(), before + 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// A shown message whose content turns into a photo gets the photo's
/// downloads; the same change in a chat nobody shows looks at no media.
#[test]
fn a_content_change_in_the_open_chat_downloads_its_new_photo() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    let downloads = || {
        let mut files: Vec<i64> = sent_of_type(&recorder, "downloadFile")
            .iter()
            .filter_map(|request| request["file_id"].as_i64())
            .collect();
        files.sort_unstable();
        files
    };
    let photo_content = |thumb: i32, full: i32| {
        format!(
            r#"{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{},"width":320,"height":240,"progressive_sizes":[]}},{{"@type":"photoSize","type":"x","photo":{},"width":800,"height":600,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}"#,
            photo_file(thumb, 10),
            photo_file(full, 80),
        )
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    driver.select_chat(ChatId(7)).unwrap();
    ingest(&mut driver, &text_message(20, 7));
    ingest(&mut driver, &text_message(30, 8));
    assert_eq!(downloads(), Vec::<i64>::new());
    driver.begin_batch();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateMessageContent","chat_id":7,"message_id":20,"new_content":{}}}"#,
            photo_content(1, 2)
        ),
    );
    driver.end_batch().unwrap();
    assert_eq!(downloads(), vec![1, 2]);
    let before = driver.batch.media_passes;
    driver.begin_batch();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateMessageContent","chat_id":8,"message_id":30,"new_content":{}}}"#,
            photo_content(3, 4)
        ),
    );
    driver.end_batch().unwrap();
    assert_eq!(driver.batch.media_passes, before);
    assert_eq!(downloads(), vec![1, 2]);
    let _ = std::fs::remove_dir_all(dir);
}

/// A reply strip in the open chat shows a message from another chat: when
/// that message turns into a photo, the strip's thumbnail is downloaded
/// without waiting for an update in the open chat.
#[test]
fn a_cross_chat_reply_target_that_turns_into_a_photo_gets_its_thumbnail() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, payload: &str| {
        driver
            .ingest(copy_and_parse(payload, &seq, &sink).unwrap())
            .unwrap();
    };
    let downloads = || {
        sent_of_type(&recorder, "downloadFile")
            .iter()
            .filter_map(|request| request["file_id"].as_i64())
            .collect::<Vec<i64>>()
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    );
    ingest(&mut driver, &text_message(30, 8));
    driver.select_chat(ChatId(7)).unwrap();
    ingest(
        &mut driver,
        r#"{"@type":"updateNewMessage","message":{"id":40,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"look","entities":[]}},"reply_to":{"@type":"messageReplyToMessage","chat_id":8,"message_id":30}}}"#,
    );
    assert_eq!(downloads(), Vec::<i64>::new());
    driver.begin_batch();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateMessageContent","chat_id":8,"message_id":30,"new_content":{{"@type":"messagePhoto","photo":{{"@type":"photo","has_stickers":false,"sizes":[{{"@type":"photoSize","type":"m","photo":{},"width":320,"height":240,"progressive_sizes":[]}}]}},"caption":{{"@type":"formattedText","text":"","entities":[]}},"has_spoiler":false,"is_secret":false}}}}"#,
            photo_file(5, 10),
        ),
    );
    driver.end_batch().unwrap();
    assert_eq!(downloads(), vec![5]);
    let _ = std::fs::remove_dir_all(dir);
}
