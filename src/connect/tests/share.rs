//! Connect-driver tests: B4 share box, forward options and send-as.
use super::super::*;
use super::*;
use crate::composer::{ComposerScheduling, ForwardDraft, SendOptions};
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{ChatId, MessageId, RequestId};
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::MessageSender;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn ingest(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    json: &str,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

fn chat_json(id: i64, title: &str, order: i64) -> [String; 2] {
    [
        format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{id},"title":"{title}","type":{{"@type":"chatTypePrivate","user_id":{id}}},"unread_count":0}}}}"#
        ),
        format!(
            r#"{{"@type":"updateChatPosition","chat_id":{id},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"{order}","is_pinned":false}}}}"#
        ),
    ]
}

fn forwarded_json(extra: RequestId, dest: i64, message_id: i64) -> String {
    format!(
        r#"{{"@type":"messages","@extra":"{}","total_count":1,"messages":[{{"id":{message_id},"chat_id":{dest},"is_outgoing":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hello already here","entities":[]}}}},"forward_info":{{"@type":"messageForwardInfo","origin":{{"@type":"messageOriginUser","sender_user_id":7}},"date":1}}}}]}}"#,
        extra.0
    )
}

const SOURCE_MESSAGE: &str = r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#;

#[test]
fn forwarding_to_two_chats_keeps_both_results_and_options() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for (id, title, order) in [(7, "Alice", 9), (8, "Bob", 8), (9, "Cy", 7)] {
        for json in chat_json(id, title, order) {
            ingest(&mut driver, &json, &seq, &sink);
        }
    }
    ingest(&mut driver, SOURCE_MESSAGE, &seq, &sink);
    driver.select_chat(ChatId(7)).unwrap();
    let draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
    let silent = SendOptions {
        disable_notification: true,
        ..SendOptions::default()
    };
    let to_bob = driver
        .forward_messages_with_options(ChatId(8), &draft, &silent)
        .unwrap();
    let to_cy = driver
        .forward_messages_with_options(ChatId(9), &draft, &silent)
        .unwrap();
    let sent: Vec<Value> = recorder
        .snapshot()
        .iter()
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .filter(|v| v["@type"] == "forwardMessages")
        .collect();
    assert_eq!(sent.len(), 2);
    assert!(
        sent.iter()
            .all(|v| v["options"]["disable_notification"] == true)
    );
    ingest(&mut driver, &forwarded_json(to_bob, 8, 80), &seq, &sink);
    let first = driver.session.last_forward.take().expect("bob result");
    assert_eq!(first.dest_title, "Bob");
    assert_eq!(first.from_chat_id, ChatId(7));
    ingest(&mut driver, &forwarded_json(to_cy, 9, 90), &seq, &sink);
    let second = driver.session.last_forward.take().expect("cy result");
    assert_eq!(second.dest_title, "Cy");
    assert_eq!(second.from_chat_id, ChatId(7));
    assert!(driver.session.queued_forward_flights.is_empty());
    assert!(driver.session.in_flight_forward.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn scheduled_forward_carries_the_send_date() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for (id, title, order) in [(7, "Alice", 9), (8, "Bob", 8)] {
        for json in chat_json(id, title, order) {
            ingest(&mut driver, &json, &seq, &sink);
        }
    }
    ingest(&mut driver, SOURCE_MESSAGE, &seq, &sink);
    driver.select_chat(ChatId(7)).unwrap();
    let draft = ForwardDraft::from_message(ChatId(7), MessageId(50), false).unwrap();
    let options = SendOptions {
        scheduling: ComposerScheduling::SendAtDate(1_900_000_000),
        ..SendOptions::default()
    };
    driver
        .forward_messages_with_options(ChatId(8), &draft, &options)
        .unwrap();
    let request = sent_request(&recorder, "forwardMessages");
    assert_eq!(
        request["options"]["scheduling_state"]["send_date"],
        1_900_000_000
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn share_search_sends_both_searches_and_ignores_stale_answers() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    for json in chat_json(7, "Alice", 9) {
        ingest(&mut driver, &json, &seq, &sink);
    }
    driver.search_share_chats("ad").unwrap();
    let local = sent_request(&recorder, "searchChats");
    let server = sent_request(&recorder, "searchChatsOnServer");
    assert_eq!(local["query"], "ad");
    assert_eq!(server["query"], "ad");
    // A chat only the server knows arrives before its answer.
    for json in chat_json(21, "Ada Lovelace", 0) {
        ingest(&mut driver, &json, &seq, &sink);
    }
    let server_extra = server["@extra"].as_str().unwrap().to_string();
    // A newer query supersedes the first one.
    driver.search_share_chats("zed").unwrap();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"chats","@extra":"{server_extra}","total_count":1,"chat_ids":[21]}}"#
        ),
        &seq,
        &sink,
    );
    assert!(driver.session.share_search.server.is_empty());
    assert!(
        driver
            .session
            .share_destinations("zed")
            .iter()
            .all(|chat| chat.id != ChatId(21))
    );
    // The current query's answer is merged below the loaded matches.
    let current = sent_request(&recorder, "searchChatsOnServer");
    let current_extra = current["@extra"].as_str().unwrap().to_string();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"chats","@extra":"{current_extra}","total_count":1,"chat_ids":[21]}}"#
        ),
        &seq,
        &sink,
    );
    let ids: Vec<i64> = driver
        .session
        .share_destinations("zed")
        .iter()
        .map(|chat| chat.id.0)
        .collect();
    assert_eq!(ids, vec![21]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn send_as_senders_load_and_set_only_listed_unlocked_identities() {
    let (dir, prepared) = prepared_tmp(&MemorySecretStore::new());
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    ingest(
        &mut driver,
        r#"{"@type":"updateNewChat","chat":{"id":-100,"title":"Group","type":{"@type":"chatTypeSupergroup","supergroup_id":100,"is_channel":false},"unread_count":0,"message_sender_id":{"@type":"messageSenderUser","user_id":1}}}"#,
        &seq,
        &sink,
    );
    assert!(driver.session.can_choose_message_sender(ChatId(-100)));
    assert_eq!(
        driver.session.selected_message_sender(ChatId(-100)),
        Some(MessageSender::User { user_id: 1 })
    );
    // Not allowed before the list is loaded.
    assert!(
        driver
            .set_chat_message_sender(ChatId(-100), MessageSender::Chat { chat_id: -200 })
            .is_err()
    );
    driver
        .get_chat_available_message_senders(ChatId(-100))
        .unwrap();
    // A second call while one is in flight sends nothing new.
    let before = recorder.snapshot().len();
    driver
        .get_chat_available_message_senders(ChatId(-100))
        .unwrap();
    assert_eq!(recorder.snapshot().len(), before);
    let request = sent_request(&recorder, "getChatAvailableMessageSenders");
    let extra = request["@extra"].as_str().unwrap().to_string();
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"chatMessageSenders","@extra":"{extra}","senders":[{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderUser","user_id":1}},"needs_premium":false}},{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderChat","chat_id":-200}},"needs_premium":false}},{{"@type":"chatMessageSender","sender":{{"@type":"messageSenderChat","chat_id":-300}},"needs_premium":true}}]}}"#
        ),
        &seq,
        &sink,
    );
    assert_eq!(
        driver.session.available_message_senders(ChatId(-100)).len(),
        3
    );
    driver
        .set_chat_message_sender(ChatId(-100), MessageSender::Chat { chat_id: -200 })
        .unwrap();
    let set = sent_request(&recorder, "setChatMessageSender");
    assert_eq!(set["message_sender_id"]["chat_id"], -200);
    // Premium-locked and unknown identities are refused.
    assert!(
        driver
            .set_chat_message_sender(ChatId(-100), MessageSender::Chat { chat_id: -300 })
            .is_err()
    );
    assert!(
        driver
            .set_chat_message_sender(ChatId(-100), MessageSender::Chat { chat_id: -999 })
            .is_err()
    );
    ingest(
        &mut driver,
        r#"{"@type":"updateChatMessageSender","chat_id":-100,"message_sender_id":{"@type":"messageSenderChat","chat_id":-200}}"#,
        &seq,
        &sink,
    );
    assert_eq!(
        driver.session.selected_message_sender(ChatId(-100)),
        Some(MessageSender::Chat { chat_id: -200 })
    );
    let _ = std::fs::remove_dir_all(dir);
}
