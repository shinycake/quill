//! Connect-driver tests: bots inline mode.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::state::InlineBotResolve;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// Bots slice: `resolve_inline_bot` sets the resolve slot with a
/// fresh generation and sends `searchPublicChat`; a second resolve
/// bumps the generation so the first answer goes stale.
#[test]
fn resolve_inline_bot_sends_search_public_chat_with_generation() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();

    let gen1 = driver.session.bots.inline_bot_resolve_seq + 1;
    driver
        .resolve_inline_bot("gif")
        .expect("resolve")
        .expect("request id");
    assert_eq!(
        driver.session.bots.inline_bot_resolve,
        Some(InlineBotResolve::Resolving {
            username: "gif".into(),
            generation: gen1,
        })
    );
    driver
        .resolve_inline_bot("gifs")
        .expect("resolve")
        .expect("request id");
    let gen2 = gen1 + 1;
    assert_eq!(
        driver.session.bots.inline_bot_resolve,
        Some(InlineBotResolve::Resolving {
            username: "gifs".into(),
            generation: gen2,
        })
    );
    let sends: Vec<Value> = recorder
        .snapshot()
        .iter()
        .filter(|json| json.contains("searchPublicChat"))
        .map(|json| serde_json::from_str::<Value>(json).unwrap())
        .collect();
    assert_eq!(sends.len(), 2);
    assert_eq!(sends[0]["username"], "gif");
    assert_eq!(sends[1]["username"], "gifs");
    let _ = std::fs::remove_dir_all(&dir);
}
