//! Connect-driver tests: stopping a live location.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, MessageId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn live_message_json(id: i64, outgoing: bool, expires_in: i32) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":7,"is_outgoing":{outgoing},"content":{{"@type":"messageLiveLocation","location":{{"@type":"liveLocation","location":{{"@type":"location","latitude":48.85,"longitude":2.35,"horizontal_accuracy":0}},"live_period":900,"heading":0,"proximity_alert_radius":0}},"expires_in":{expires_in}}}}}}}"#
    )
}

#[test]
fn stop_live_location_sends_null_location_only_for_a_running_own_share() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &sink);
    for json in [
        live_message_json(61, true, 600),
        live_message_json(62, false, 600),
        live_message_json(63, true, 0),
    ] {
        driver
            .ingest(copy_and_parse(&json, &seq, &sink).unwrap())
            .unwrap();
    }

    // Someone else's share, and one that already ended, are refused.
    assert!(driver.stop_live_location(ChatId(7), MessageId(62)).is_err());
    assert!(driver.stop_live_location(ChatId(7), MessageId(63)).is_err());
    assert!(driver.stop_live_location(ChatId(7), MessageId(99)).is_err());
    assert!(
        !recorder
            .snapshot()
            .iter()
            .any(|json| json.contains("editMessageLiveLocation"))
    );

    let extra = driver.stop_live_location(ChatId(7), MessageId(61)).unwrap();
    let json = recorder.snapshot().last().cloned().expect("request sent");
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "editMessageLiveLocation");
    assert_eq!(v["@extra"], extra.0.to_string());
    assert_eq!(v["message_id"], 61);
    assert!(v["location"].is_null());
}
