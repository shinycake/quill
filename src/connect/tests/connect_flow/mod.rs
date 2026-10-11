//! Connect-driver tests: gate, auth, parameters, shutdown, chat loading.
use super::super::*;
use super::*;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{InlineQueryFetch, RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::envelope::ChannelMemberStatus;
use crate::telegram::requests::{
    close_secret_chat as close_secret_chat_request, create_new_secret_chat, get_secret_chat,
};
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

mod auth;
mod sending;

#[test]
fn gate_blocks_without_credentials() {
    assert_eq!(
        evaluate_gate(false),
        ConnectGate::Blocked(ConnectBlocker::MissingCredentials)
    );
}

#[test]
fn gate_blocks_without_tdjson_when_credentials_present() {
    let _lock = TDJSON_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = std::env::var("QUILL_TDJSON_PATH").ok();
    // SAFETY: exclusive lock; restored below.
    unsafe { std::env::remove_var("QUILL_TDJSON_PATH") };
    let gate = evaluate_gate(true);
    if let Some(v) = previous {
        unsafe { std::env::set_var("QUILL_TDJSON_PATH", v) };
    }
    assert_eq!(gate, ConnectGate::Blocked(ConnectBlocker::MissingTdjson));
    assert!(
        ConnectBlocker::MissingTdjson
            .user_message()
            .contains("QUILL_TDJSON_PATH")
    );
    assert!(
        ConnectBlocker::MissingTdjson
            .user_message()
            .contains("native-bundle")
    );
}

#[test]
fn set_tdlib_parameters_shape_includes_hash_but_debug_redacts_credentials() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let creds = test_credentials();
    // Slice parity:settings-language: the params carry the persisted
    // language pref instead of a hardcoded "en".
    let params = build_set_tdlib_parameters(&creds, &prepared.paths, &prepared.database_key, "de");
    let json = params.to_json(RequestId(7));
    let v: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["@type"], "setTdlibParameters");
    assert_eq!(v["@extra"], "7");
    assert_eq!(v["api_id"], 99);
    assert_eq!(v["api_hash"], "unit-test-hash-not-for-network");
    assert_eq!(v["system_language_code"], "de");
    assert_eq!(v["use_secret_chats"], true);
    assert_eq!(v["use_file_database"], true);
    assert!(v["database_directory"].as_str().unwrap().contains("tdlib"));
    assert!(!v["database_encryption_key"].as_str().unwrap().is_empty());
    let debug = format!("{creds:?}");
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("unit-test-hash"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Bots slice: `ConnectDriver::inline_query` — a first page sets the
/// slot to `Loading` and sends one `getInlineQueryResults`, a second
/// call while in flight is a no-op that leaves the slot alone, and a
/// first-page send failure rolls the slot back to `None`.
#[test]
fn driver_inline_query_first_page_sends_dedupes_and_rolls_back() {
    type InlineQueryDriverHarness = (
        std::path::PathBuf,
        ConnectDriver<Arc<RecordingSender>>,
        Arc<RecordingSender>,
    );

    fn inline_query_driver() -> InlineQueryDriverHarness {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
                .ingest(
                    copy_and_parse(
                        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                        &seq,
                        &sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        (dir, driver, sender)
    }

    fn failing_inline_query_driver() -> (
        std::path::PathBuf,
        ConnectDriver<Arc<FailFirstInlineQuerySender>>,
    ) {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FailFirstInlineQuerySender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
        let seq = AtomicU64::new(0);
        driver
                .ingest(
                    copy_and_parse(
                        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                        &seq,
                        &sink,
                    )
                    .unwrap(),
                )
                .unwrap();
        (dir, driver)
    }

    let (dir, mut driver, sender) = inline_query_driver();

    // (a) first page: slot → Loading, one getInlineQueryResults sent.
    let extra = driver
        .inline_query(77, ChatId(1), "@gif cats", "")
        .expect("first page")
        .expect("request id");
    match &driver.session.bots.inline_query {
        Some(slot) => {
            assert_eq!(slot.chat_id, ChatId(1));
            assert_eq!(slot.bot_user_id, 77);
            assert_eq!(slot.query, "@gif cats");
            assert!(matches!(slot.fetch, InlineQueryFetch::Loading));
        }
        None => panic!("inline query slot missing"),
    }
    let inline_query_sends = || {
        sender
            .snapshot()
            .into_iter()
            .filter(|json| json.contains("getInlineQueryResults"))
            .map(|json| serde_json::from_str::<Value>(&json).unwrap())
            .collect::<Vec<_>>()
    };
    let first_page = inline_query_sends();
    assert_eq!(first_page.len(), 1);
    assert_eq!(first_page[0]["@type"], "getInlineQueryResults");
    assert_eq!(first_page[0]["@extra"], extra.0.to_string());
    assert_eq!(first_page[0]["bot_user_id"], 77);
    assert_eq!(first_page[0]["chat_id"], 1);
    assert_eq!(first_page[0]["query"], "@gif cats");
    assert_eq!(first_page[0]["offset"], "");

    // (b) in flight: the second call no-ops and the slot is untouched.
    assert_eq!(
        driver.inline_query(77, ChatId(1), "@gif cats", ""),
        Ok(None)
    );
    assert!(matches!(
        driver
            .session
            .bots
            .inline_query
            .as_ref()
            .map(|slot| &slot.fetch),
        Some(InlineQueryFetch::Loading)
    ));
    assert_eq!(inline_query_sends().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);

    // (c) a first-page send failure rolls the slot back to None.
    let (dir2, mut driver2) = failing_inline_query_driver();
    let failed = driver2.inline_query(77, ChatId(1), "@gif cats", "");
    assert!(matches!(failed, Err(ConnectSendError::Native)));
    assert!(driver2.session.bots.inline_query.is_none());
    let _ = std::fs::remove_dir_all(&dir2);
}

/// Phase B1: the three secret-chat request shapes
/// (`createNewSecretChat`, `getSecretChat`, `closeSecretChat`;
/// schema 1.8.67 lines 13340, 11516, 15242).
#[test]
fn secret_chat_request_shapes() {
    let create = create_new_secret_chat(RequestId(11), 41);
    let v: Value = serde_json::from_str(&create).unwrap();
    assert_eq!(v["@type"], "createNewSecretChat");
    assert_eq!(v["@extra"], "11");
    assert_eq!(v["user_id"], 41);

    let get = get_secret_chat(RequestId(12), 7);
    let v: Value = serde_json::from_str(&get).unwrap();
    assert_eq!(v["@type"], "getSecretChat");
    assert_eq!(v["@extra"], "12");
    assert_eq!(v["secret_chat_id"], 7);

    let close = close_secret_chat_request(RequestId(13), 7);
    let v: Value = serde_json::from_str(&close).unwrap();
    assert_eq!(v["@type"], "closeSecretChat");
    assert_eq!(v["@extra"], "13");
    assert_eq!(v["secret_chat_id"], 7);
}

/// Phase B4: `setChatMessageAutoDeleteTime` value rule (schema 1.8.67,
/// line 13454) is enforced driver-side: secret chats accept arbitrary
/// non-negative seconds; other chats need 0 or day-multiples up to a
/// year. Unknown chats are rejected too.
#[test]
fn driver_validates_auto_delete_time_values() {
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
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":11,"title":"Cloud","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Secret chat: arbitrary seconds are fine; negatives are not.
    for secs in [0, 5, 90, 3600, 604800] {
        let extra = driver
            .set_chat_message_auto_delete_time(ChatId(31), secs)
            .expect("secret chat accepts arbitrary seconds");
        let sent = recorder.snapshot();
        let last = sent.last().unwrap();
        assert!(last.contains("setChatMessageAutoDeleteTime"));
        assert!(last.contains(&format!("\"message_auto_delete_time\":{secs}")));
        assert!(last.contains(&format!("\"@extra\":\"{}\"", extra.0)));
    }
    assert_eq!(
        driver.set_chat_message_auto_delete_time(ChatId(31), -1),
        Err(ConnectSendError::InvalidRequest)
    );
    // Regular chat: 0 or day-multiples up to 365 days.
    for secs in [0, 86_400, 604_800, 2_592_000, 365 * 86_400] {
        driver
            .set_chat_message_auto_delete_time(ChatId(11), secs)
            .expect("regular chat accepts 0 / day multiples");
    }
    for secs in [-1, 5, 3600, 90_000, 365 * 86_400 + 86_400] {
        assert_eq!(
            driver.set_chat_message_auto_delete_time(ChatId(11), secs),
            Err(ConnectSendError::InvalidRequest),
            "regular chat rejects {secs}"
        );
    }
    // Unknown chat: rejected.
    assert_eq!(
        driver.set_chat_message_auto_delete_time(ChatId(999), 3600),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase D3c: `fetch_chat_event_log` sends one `getChatEventLog`
/// (null filters, 100-event page, cursor 0), dedupes while a request
/// is in flight, pages older via `fetch_chat_event_log_more`, and
/// no-ops for non-admins / unknown chats / an inactive chats path
/// (schema 1.8.67, line 15252).
#[test]
fn driver_fetch_chat_event_log_sends_dedupes_and_gates() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);

    // Chats path inactive before authorization is Ready: rejected.
    assert_eq!(
        driver.fetch_chat_event_log(ChatId(13)),
        Err(ConnectSendError::InvalidRequest)
    );

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
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":13,"title":"Demo channel","type":{"@type":"chatTypeSupergroup","supergroup_id":13,"is_channel":true},"unread_count":0}}"#,
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    // Unknown chat and non-admin channel: quiet no-ops (other Ready
    // bookkeeping like loadChats may still send).
    assert_eq!(driver.fetch_chat_event_log(ChatId(999)), Ok(None));
    assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
    let event_log_sends = || {
        recorder
            .snapshot()
            .into_iter()
            .filter(|sent| sent.contains("getChatEventLog"))
            .collect::<Vec<_>>()
    };
    assert!(event_log_sends().is_empty());

    // Become an administrator: the first page goes out.
    driver
        .session
        .chats
        .get_mut(&13)
        .unwrap()
        .set_member_status(ChannelMemberStatus::Administrator, None);
    let extra = driver
        .fetch_chat_event_log(ChatId(13))
        .expect("event log fetch")
        .expect("request id");
    let sent = event_log_sends();
    assert_eq!(sent.len(), 1);
    let first: Value = serde_json::from_str(&sent[0]).unwrap();
    assert_eq!(first["@type"], "getChatEventLog");
    assert_eq!(first["@extra"], extra.0.to_string());
    assert_eq!(first["chat_id"], 13);
    assert_eq!(first["query"], "");
    assert_eq!(first["from_event_id"], 0);
    assert_eq!(first["limit"], 100);
    assert!(first["filters"].is_null());
    assert!(first["user_ids"].as_array().unwrap().is_empty());

    // In-flight dedupe: further fetches no-op while the first is out.
    assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
    assert_eq!(driver.fetch_chat_event_log_more(ChatId(13)), Ok(None));
    assert_eq!(event_log_sends().len(), 1);

    // Feed a full page (100 events) back through the reducer; the
    // next older page goes out with the oldest event id as cursor.
    let events: Vec<String> = (401..=500)
            .rev()
            .map(|id| {
                format!(
                    r#"{{"@type":"chatEvent","id":{id},"date":1700000000,"member_id":{{"@type":"messageSenderUser","user_id":777}},"action":{{"@type":"chatEventMemberJoined"}}}}"#
                )
            })
            .collect();
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chatEvents","@extra":"{}","events":[{}]}}"#,
                    extra.0,
                    events.join(",")
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    let extra_more = driver
        .fetch_chat_event_log_more(ChatId(13))
        .expect("event log more")
        .expect("request id");
    let sent = event_log_sends();
    assert_eq!(sent.len(), 2);
    let more: Value = serde_json::from_str(&sent[1]).unwrap();
    assert_eq!(more["@type"], "getChatEventLog");
    assert_eq!(more["@extra"], extra_more.0.to_string());
    assert_eq!(more["from_event_id"], 401);

    // A plain fetch after a load is cached: no-op.
    assert_eq!(driver.fetch_chat_event_log(ChatId(13)), Ok(None));
    assert_eq!(event_log_sends().len(), 2);

    // Complete the "more" request with a short page (log exhausted),
    // then refresh clears the cache and re-sends from the top.
    driver
        .ingest(
            copy_and_parse(
                &format!(
                    r#"{{"@type":"chatEvents","@extra":"{}","events":[]}}"#,
                    extra_more.0
                ),
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(driver.fetch_chat_event_log_more(ChatId(13)), Ok(None));
    let extra_refresh = driver
        .refresh_chat_event_log(ChatId(13))
        .expect("event log refresh")
        .expect("request id");
    let sent = event_log_sends();
    assert_eq!(sent.len(), 3);
    let refresh: Value = serde_json::from_str(&sent[2]).unwrap();
    assert_eq!(refresh["@extra"], extra_refresh.0.to_string());
    assert_eq!(refresh["from_event_id"], 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Phase B1: the ordinary `sendMessage` path works for a Ready
/// secret chat (secret chats are plain chat ids at the send layer);
/// Pending chats are rejected by the same `can_post` gate as
/// everything else.
#[test]
fn driver_sends_into_ready_secret_chat_only() {
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
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":31,"user_id":7,"state":{"@type":"secretChatStateReady"},"is_outbound":true,"key_hash":"","layer":144}}"#,
        r#"{"@type":"updateSecretChat","secret_chat":{"@type":"secretChat","id":33,"user_id":7,"state":{"@type":"secretChatStatePending"},"is_outbound":true,"key_hash":"","layer":144}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Secret","type":{"@type":"chatTypeSecret","secret_chat_id":31,"user_id":7},"unread_count":0}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":33,"title":"Pending secret","type":{"@type":"chatTypeSecret","secret_chat_id":33,"user_id":7},"unread_count":0}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    driver.select_chat(ChatId(31)).unwrap();
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(31),
        driver.session.view_generation,
        "CANARYSECRETSEND",
    );
    let extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    assert!(send_json.contains("sendMessage"));
    assert!(send_json.contains("\"chat_id\":31"));
    assert!(send_json.contains("CANARYSECRETSEND"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", extra.0)));

    driver.select_chat(ChatId(33)).unwrap();
    let pending_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(33),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&pending_snap),
        Err(ConnectSendError::InvalidRequest)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
