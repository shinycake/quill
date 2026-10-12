//! Connect-driver tests: the business bot manage bar (recorded JSON,
//! nothing live).
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::ChatId;
use crate::platform::MemorySecretStore;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: &str = r#"{"@type":"updateNewChat","chat":{"id":501,"title":"Shopper","type":{"@type":"chatTypePrivate","user_id":501},"business_bot_manage_bar":{"@type":"businessBotManageBar","bot_user_id":900,"manage_url":"https://t.me/helper_bot?start=bizChat501","is_bot_paused":false,"can_bot_reply":true}}}"#;

#[test]
fn bar_loads_updates_toggles_and_removes() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let mut driver = ready_driver(&recorder, prepared, &sink, &seq);
    let feed = |driver: &mut ConnectDriver<_>, json: &str| {
        driver
            .ingest(copy_and_parse(json, &seq, &sink).unwrap())
            .unwrap();
    };
    feed(&mut driver, CHAT);
    let bar = driver.session.business_bot_bar(ChatId(501)).unwrap();
    assert_eq!(bar.bot_user_id, 900);
    assert!(bar.can_reply && !bar.is_paused);

    // Pause: request goes out and the bar flips at once.
    driver
        .toggle_business_bot_paused(ChatId(501), true)
        .unwrap();
    let sent = recorder.snapshot();
    assert!(sent.iter().any(|j| {
        j.contains("\"toggleBusinessConnectedBotChatIsPaused\"")
            && j.contains("\"chat_id\":501")
            && j.contains("\"is_paused\":true")
    }));
    assert!(
        driver
            .session
            .business_bot_bar(ChatId(501))
            .unwrap()
            .is_paused
    );

    // The server's update wins.
    feed(
        &mut driver,
        r#"{"@type":"updateChatBusinessBotManageBar","chat_id":501,"business_bot_manage_bar":{"@type":"businessBotManageBar","bot_user_id":900,"manage_url":"u","is_bot_paused":false,"can_bot_reply":false}}"#,
    );
    let bar = driver.session.business_bot_bar(ChatId(501)).unwrap();
    assert!(!bar.is_paused && !bar.can_reply);

    // Remove: request goes out, bar disappears, no second request.
    driver.remove_business_bot_from_chat(ChatId(501)).unwrap();
    assert!(driver.session.business_bot_bar(ChatId(501)).is_none());
    assert!(recorder.snapshot().iter().any(|j| {
        j.contains("\"removeBusinessConnectedBotFromChat\"") && j.contains("\"chat_id\":501")
    }));
    assert_eq!(driver.remove_business_bot_from_chat(ChatId(501)), Ok(None));

    // A null update clears a stored bar too.
    feed(&mut driver, CHAT);
    feed(
        &mut driver,
        r#"{"@type":"updateChatBusinessBotManageBar","chat_id":501,"business_bot_manage_bar":null}"#,
    );
    assert!(driver.session.business_bot_bar(ChatId(501)).is_none());
}
