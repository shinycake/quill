//! Connect-driver tests: dice, contact and location sends, map tiles.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::requests::ContactShare;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn ready_driver() -> (
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    std::path::PathBuf,
) {
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
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    driver.select_chat(ChatId(7)).unwrap();
    (driver, recorder, dir)
}

fn last_send(recorder: &RecordingSender) -> Value {
    let json = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|json| json.contains("\"sendMessage\""))
        .expect("a sendMessage");
    serde_json::from_str(&json).unwrap()
}

#[test]
fn a_lone_dice_emoji_sends_as_a_dice() {
    let (mut driver, recorder, dir) = ready_driver();
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "\u{1F3B2}",
    );
    driver.send_text_snapshot(&snap).unwrap();
    let sent = last_send(&recorder);
    assert_eq!(sent["input_message_content"]["@type"], "inputMessageDice");
    assert_eq!(sent["input_message_content"]["emoji"], "\u{1F3B2}");

    let text = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "roll \u{1F3B2}",
    );
    driver.send_text_snapshot(&text).unwrap();
    let sent = last_send(&recorder);
    assert_eq!(sent["input_message_content"]["@type"], "inputMessageText");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn contact_and_location_shares_go_out_as_input_messages() {
    let (mut driver, recorder, dir) = ready_driver();
    let options = crate::composer::SendOptions::default();
    let contact = ContactShare {
        phone_number: "+14155550123".into(),
        first_name: "Ada".into(),
        last_name: "Lovelace".into(),
        user_id: 42,
    };
    driver
        .share_contact_to_chat(ChatId(7), &contact, None, &options)
        .unwrap();
    let sent = last_send(&recorder);
    assert_eq!(
        sent["input_message_content"]["@type"],
        "inputMessageContact"
    );
    assert_eq!(sent["input_message_content"]["contact"]["user_id"], 42);

    // A card with neither a phone number nor a user id is refused.
    let empty = ContactShare {
        phone_number: " ".into(),
        first_name: "Nobody".into(),
        last_name: String::new(),
        user_id: 0,
    };
    assert!(
        driver
            .share_contact_to_chat(ChatId(7), &empty, None, &options)
            .is_err()
    );

    driver
        .share_location_to_chat(ChatId(7), 37.7749, -122.4194, None, &options)
        .unwrap();
    let sent = last_send(&recorder);
    assert_eq!(
        sent["input_message_content"]["@type"],
        "inputMessageLocation"
    );
    assert_eq!(
        sent["input_message_content"]["location"]["latitude"],
        37.7749
    );

    // Unknown chat: refused, nothing sent.
    assert!(
        driver
            .share_location_to_chat(ChatId(99), 1.0, 2.0, None, &options)
            .is_err()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_location_message_asks_for_its_map_tile_once_and_downloads_it() {
    let (mut driver, recorder, dir) = ready_driver();
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let seq = AtomicU64::new(100);
    let message = r#"{"@type":"updateNewMessage","message":{"id":501,"chat_id":7,"is_outgoing":false,"date":1700000000,"content":{"@type":"messageLocation","location":{"@type":"location","latitude":37.7749,"longitude":-122.4194,"horizontal_accuracy":15}}}}"#;
    driver
        .ingest(copy_and_parse(message, &seq, &dyn_sink).unwrap())
        .unwrap();
    driver.maybe_download_open_thumbs().unwrap();
    let thumbs = |recorder: &RecordingSender| -> Vec<Value> {
        recorder
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("getMapThumbnailFile"))
            .map(|json| serde_json::from_str(&json).unwrap())
            .collect()
    };
    let asked = thumbs(&recorder);
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0]["location"]["latitude"], 37.7749);
    assert_eq!(asked[0]["chat_id"], 7);
    assert_eq!(asked[0]["zoom"], 15);

    // A second pass does not ask again while the answer is awaited.
    driver.maybe_download_open_thumbs().unwrap();
    assert_eq!(thumbs(&recorder).len(), 1);

    let extra = asked[0]["@extra"].as_str().unwrap();
    let file = format!(
        r#"{{"@type":"file","@extra":"{extra}","id":88,"size":0,"expected_size":9000,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"m","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":0}}}}"#
    );
    driver
        .ingest(copy_and_parse(&file, &seq, &dyn_sink).unwrap())
        .unwrap();
    driver.maybe_download_open_thumbs().unwrap();
    assert!(
        recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("downloadFile") && json.contains("\"file_id\":88"))
    );
    assert_eq!(thumbs(&recorder).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
