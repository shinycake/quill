//! Connect-driver tests: sponsored messages in live channel history.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::{RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// getChatSponsoredMessages -> store -> view: `viewMessages` carries the ad id
/// once, `clickChatSponsoredMessage` is sent per click, and a second fetch
/// inside five minutes is skipped.
#[test]
fn sponsored_fetch_view_once_and_click() {
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
    let chat = ChatId(13);
    driver.session.open_chat(chat);
    let extra = driver
        .fetch_sponsored_messages(chat)
        .unwrap()
        .expect("fetch sent");
    let response = format!(
        r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":0,"messages":[{{"@type":"sponsoredMessage","message_id":9001,"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"ad","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":""}},"title":"T","button_text":"Go","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}]}}"#,
        extra.0
    );
    driver
        .ingest(copy_and_parse(&response, &seq, &dyn_sink).unwrap())
        .unwrap();
    assert_eq!(
        driver.session.open_sponsored_tail().map(|m| m.message_id),
        Some(9001)
    );
    // Fresh list: no refetch.
    assert!(driver.fetch_sponsored_messages(chat).unwrap().is_none());

    // The ad is shown on several frames: one viewMessages.
    assert!(
        driver
            .view_sponsored_messages(chat, &[9001])
            .unwrap()
            .is_some()
    );
    assert!(
        driver
            .view_sponsored_messages(chat, &[9001])
            .unwrap()
            .is_none()
    );
    let views: Vec<Value> = recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("viewMessages"))
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
    assert_eq!(views.len(), 1);
    assert_eq!(views[0]["chat_id"], 13);
    assert_eq!(views[0]["message_ids"], serde_json::json!([9001]));
    assert_eq!(views[0]["force_read"], false);

    driver
        .click_chat_sponsored_message(chat, 9001, false, false)
        .unwrap();
    let clicks = recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("clickChatSponsoredMessage"))
        .count();
    assert_eq!(clicks, 1);
    assert!(
        driver
            .session
            .requests
            .has_purpose_for_chat(RequestPurpose::ClickChatSponsoredMessage, chat)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
