//! Connect-driver tests: the recent actions admin filter is server side
//! (`getChatEventLog.user_ids`).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ChannelMemberStatus;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

#[test]
fn event_log_sends_the_selected_admins_as_user_ids() {
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
        r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    driver
        .session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Administrator, None);
    let log_sends = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|sent| sent.contains("getChatEventLog"))
            .map(|sent| serde_json::from_str::<Value>(&sent).unwrap())
            .collect::<Vec<_>>()
    };

    // No selection: every admin, an empty list.
    let first = driver.fetch_chat_event_log(ChatId(13)).unwrap().unwrap();
    assert_eq!(log_sends()[0]["user_ids"], serde_json::json!([]));

    // Picking two admins refetches with both ids, in pick order.
    driver.session.requests.take(first);
    driver.toggle_chat_event_log_user(ChatId(13), 7);
    driver.toggle_chat_event_log_user(ChatId(13), 3);
    let second = driver.refresh_chat_event_log(ChatId(13)).unwrap().unwrap();
    let sends = log_sends();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[1]["user_ids"], serde_json::json!([7, 3]));

    // Clearing the filter goes back to everyone.
    driver.session.requests.take(second);
    driver.clear_chat_event_log_users(ChatId(13));
    driver.refresh_chat_event_log(ChatId(13)).unwrap();
    assert_eq!(log_sends()[2]["user_ids"], serde_json::json!([]));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn toggling_the_same_admin_twice_clears_the_filter() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink);
    let mut driver = ConnectDriver::new(session, recorder, test_credentials(), prepared);
    driver.toggle_chat_event_log_user(ChatId(13), 7);
    assert_eq!(driver.session.event_log_users.get(&13), Some(&vec![7]));
    driver.toggle_chat_event_log_user(ChatId(13), 7);
    assert!(driver.session.event_log_users.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
