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
    match &driver.session.inline_query {
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
        driver.session.inline_query.as_ref().map(|slot| &slot.fetch),
        Some(InlineQueryFetch::Loading)
    ));
    assert_eq!(inline_query_sends().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);

    // (c) a first-page send failure rolls the slot back to None.
    let (dir2, mut driver2) = failing_inline_query_driver();
    let failed = driver2.inline_query(77, ChatId(1), "@gif cats", "");
    assert!(matches!(failed, Err(ConnectSendError::Native)));
    assert!(driver2.session.inline_query.is_none());
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

#[test]
fn driver_sends_parameters_then_reaches_wait_phone_via_injection() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let kick = driver.kickoff().unwrap();
    assert_eq!(kick.0, 1);

    let seq = AtomicU64::new(0);
    let wait_params = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_params).unwrap();
    assert!(driver.parameters_sent());

    let sent = recorder.snapshot();
    assert_eq!(sent.len(), 2); // getAuthorizationState + setTdlibParameters
    assert!(sent[0].contains("getAuthorizationState"));
    assert!(sent[1].contains("setTdlibParameters"));
    assert!(sent[1].contains("\"api_id\":99"));
    assert!(sent[1].contains("unit-test-hash-not-for-network"));
    assert!(!sink.rendered().contains("unit-test-hash"));

    let ok = copy_and_parse(r#"{"@type":"ok","@extra":"2"}"#, &seq, &dyn_sink).unwrap();
    driver.ingest(ok).unwrap();
    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitPhoneNumber
    ));
    assert_eq!(
        driver.session.auth_view.action,
        crate::auth::AuthAction::EnterPhone
    );

    let phone_extra = driver.submit_phone("+15551212").unwrap();
    let sent = recorder.snapshot();
    let phone_json = sent.last().unwrap();
    assert!(phone_json.contains("setAuthenticationPhoneNumber"));
    assert!(phone_json.contains(&format!("\"@extra\":\"{}\"", phone_extra.0)));
    assert!(phone_json.contains("+15551212"));
    assert!(!sink.rendered().contains("+15551212"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_submits_code_and_password_only_in_matching_states() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    assert_eq!(
        driver.submit_code("12345"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.submit_password("secret"),
        Err(ConnectSendError::InvalidRequest)
    );

    let seq = AtomicU64::new(0);
    let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_code).unwrap();
    assert!(matches!(
        driver.session.auth,
        AuthorizationState::WaitCode {
            code_length: Some(5)
        }
    ));
    assert_eq!(
        driver.submit_password("secret"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.submit_code("  "),
        Err(ConnectSendError::InvalidRequest)
    );
    let code_extra = driver.submit_code("  12345 ").unwrap();
    let sent = recorder.snapshot();
    let code_json = sent.last().unwrap();
    assert!(code_json.contains("checkAuthenticationCode"));
    assert!(code_json.contains(&format!("\"@extra\":\"{}\"", code_extra.0)));
    assert!(code_json.contains("\"code\":\"12345\""));
    assert!(!sink.rendered().contains("12345"));

    let wait_password = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPassword","password_hint":"CANARY_HINT","has_recovery_email_address":true}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_password).unwrap();
    assert_eq!(
        driver.submit_code("12345"),
        Err(ConnectSendError::InvalidRequest)
    );
    let pw_extra = driver.submit_password(" unit-pw ").unwrap();
    let sent = recorder.snapshot();
    let pw_json = sent.last().unwrap();
    assert!(pw_json.contains("checkAuthenticationPassword"));
    assert!(pw_json.contains(&format!("\"@extra\":\"{}\"", pw_extra.0)));
    // Password is not trimmed.
    assert!(pw_json.contains("\"password\":\" unit-pw \""));
    assert!(!sink.rendered().contains("unit-pw"));
    assert!(!sink.rendered().contains("CANARY_HINT"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_resend_code_and_qr_login_only_in_matching_states() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);

    // Both actions are gated: nothing valid to do in the initial state.
    assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
    assert_eq!(
        driver.request_qr_login(),
        Err(ConnectSendError::InvalidRequest)
    );

    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();
    // Resend needs WaitCode; QR login needs WaitPhoneNumber.
    assert_eq!(driver.resend_code(), Err(ConnectSendError::InvalidRequest));
    let qr_extra = driver.request_qr_login().unwrap();
    let sent = recorder.snapshot();
    let qr_json = sent.last().unwrap();
    assert!(qr_json.contains("requestQrCodeAuthentication"));
    assert!(qr_json.contains("\"other_user_ids\":[]"));
    assert!(qr_json.contains(&format!("\"@extra\":\"{}\"", qr_extra.0)));

    let wait_code = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitCode","code_info":{"@type":"authenticationCodeInfo","type":{"@type":"authenticationCodeTypeSms","length":5}}}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_code).unwrap();
    assert_eq!(
        driver.request_qr_login(),
        Err(ConnectSendError::InvalidRequest)
    );
    let resend_extra = driver.resend_code().unwrap();
    let sent = recorder.snapshot();
    let resend_json = sent.last().unwrap();
    assert!(resend_json.contains("resendAuthenticationCode"));
    assert!(resend_json.contains("resendCodeReasonUserRequest"));
    assert!(resend_json.contains(&format!("\"@extra\":\"{}\"", resend_extra.0)));

    // The QR link from the auth update lands on the session state and is
    // never written to diagnostics.
    let wait_qr = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitOtherDeviceConfirmation","link":"tg://login/?token=unit-test-token"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_qr).unwrap();
    assert!(matches!(
        &driver.session.auth,
        AuthorizationState::WaitOtherDeviceConfirmation { link }
        if link == "tg://login/?token=unit-test-token"
    ));
    assert!(!sink.rendered().contains("unit-test-token"));

    let _ = std::fs::remove_dir_all(&dir);
}

/// Slice A2: the 2FA driver gates sends, dedupes the fetch, shapes
/// `setPassword` correctly, and never leaks passwords into
/// diagnostics.
#[test]
fn driver_two_step_password_ops_gate_dedupe_and_shape() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);

    // Gated before the chats path is active.
    assert_eq!(
        driver.fetch_password_state(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_two_step_password("", "s3cret", "", None),
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

    // Fetch sends `getPasswordState` once; a second fetch dedupes.
    let fetch_extra = driver.fetch_password_state().unwrap().unwrap();
    assert_eq!(driver.fetch_password_state(), Ok(None));
    let sent = recorder.snapshot();
    let fetch_json = sent.last().unwrap();
    assert!(fetch_json.contains("\"getPasswordState\""));
    assert!(fetch_json.contains(&format!("\"@extra\":\"{}\"", fetch_extra.0)));

    // Doomed requests are rejected before leaving: disable needs the
    // current password; recovery email needs password + address.
    assert_eq!(
        driver.set_two_step_password("", "", "", None),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_recovery_email("", "me@example.com"),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.set_recovery_email("s3cret", ""),
        Err(ConnectSendError::InvalidRequest)
    );
    // One op in flight: a second send is rejected (no double-send).
    assert_eq!(
        driver.set_two_step_password("", "s3cret", "hint", Some("me@example.com")),
        Err(ConnectSendError::InvalidRequest)
    );

    // The `passwordState` answer lands on the session, clears loading,
    // and the password never reaches diagnostics.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":false,"password_hint":"","has_recovery_email_address":false,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        fetch_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(driver.session.password_state.is_some());
    assert!(!driver.session.password_state_loading);

    // Enable: empty old password, email in the same call.
    let enable_extra = driver
        .set_two_step_password("", "s3cret", "hint", Some("me@example.com"))
        .unwrap();
    let sent = recorder.snapshot();
    let enable_json: serde_json::Value = serde_json::from_str(sent.last().unwrap()).unwrap();
    assert_eq!(enable_json["@type"], "setPassword");
    assert_eq!(enable_json["old_password"], "");
    assert_eq!(enable_json["new_password"], "s3cret");
    assert_eq!(enable_json["new_hint"], "hint");
    assert_eq!(enable_json["set_recovery_email_address"], true);
    assert_eq!(enable_json["new_recovery_email_address"], "me@example.com");
    assert_eq!(
        enable_json["@extra"],
        serde_json::Value::String(enable_extra.0.to_string())
    );

    // Answer the enable: password now set, recovery email confirmed.
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        enable_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert!(
        driver
            .session
            .password_state
            .as_ref()
            .is_some_and(|s| s.has_password)
    );

    // Resend/abort are meaningless without a pending confirmation.
    assert_eq!(
        driver.resend_recovery_email_code(),
        Err(ConnectSendError::InvalidRequest)
    );
    assert_eq!(
        driver.cancel_recovery_email_setup(),
        Err(ConnectSendError::InvalidRequest)
    );

    // New recovery email → pending confirmation state.
    let email_extra = driver
        .set_recovery_email("s3cret", "new@example.com")
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    assert_eq!(
        driver
            .session
            .password_state
            .as_ref()
            .and_then(|s| s.pending_email_pattern.clone())
            .as_deref(),
        Some("n***@example.com")
    );

    // Now resend and abort send their requests.
    let resend_extra = driver.resend_recovery_email_code().unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.last()
            .unwrap()
            .contains("resendRecoveryEmailAddressCode")
    );
    // One in flight blocks the abort until the resend answers.
    assert_eq!(
        driver.cancel_recovery_email_setup(),
        Err(ConnectSendError::InvalidRequest)
    );
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":null,"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        resend_extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    // Pending again → abort sends `cancelRecoveryEmailAddressVerification`.
    let email_extra2 = driver
        .set_recovery_email("s3cret", "new@example.com")
        .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        r#"{{"@type":"passwordState","has_password":true,"password_hint":"hint","has_recovery_email_address":true,"has_passport_data":false,"recovery_email_address_code_info":{{"@type":"emailAddressAuthenticationCodeInfo","email_address_pattern":"n***@example.com","length":6}},"login_email_address_pattern":"","pending_reset_date":0,"@extra":"{}"}}"#,
                        email_extra2.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.cancel_recovery_email_setup().unwrap();
    let sent = recorder.snapshot();
    assert!(
        sent.last()
            .unwrap()
            .contains("cancelRecoveryEmailAddressVerification")
    );

    // Passwords ride request JSON only — never diagnostics.
    for token in ["s3cret", "me@example.com", "new@example.com"] {
        assert!(
            !sink.rendered().contains(token),
            "secret leaked to diagnostics: {token}"
        );
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn wait_closed_sends_close_and_reaches_closed_via_injection() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    let seq = AtomicU64::new(0);
    let wait_phone = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitPhoneNumber"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(wait_phone).unwrap();

    let envelopes = vec![
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
            copy_and_parse(
                r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        ];
    let mut iter = envelopes.into_iter();
    assert!(wait_closed(
        &mut driver,
        |_| iter.next(),
        Duration::from_secs(2)
    ));
    assert!(matches!(driver.session.auth, AuthorizationState::Closed));
    let sent = recorder.snapshot();
    let close_json = sent.last().expect("close request");
    assert!(close_json.contains("\"@type\":\"close\""));
    assert!(!sink.rendered().contains("unit-test-hash"));
    // Second call is a no-op once Closed (no extra send).
    let before = sent.len();
    assert!(wait_closed(&mut driver, |_| None, Duration::ZERO));
    assert_eq!(recorder.snapshot().len(), before);
    drop(driver);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_tdjson_message_is_actionable() {
    let msg = ConnectBlocker::MissingTdjson.user_message();
    assert!(msg.contains("QUILL_TDJSON_PATH"));
    assert!(msg.contains("never searched"));
}

#[test]
fn driver_loads_chats_after_ready_then_send_text() {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);

    assert_eq!(
        driver.select_chat(ChatId(1)),
        Err(ConnectSendError::InvalidRequest)
    );

    let seq = AtomicU64::new(0);
    let ready = copy_and_parse(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(ready).unwrap();
    assert!(matches!(driver.session.auth, AuthorizationState::Ready));

    let sent = recorder.snapshot();
    // Phase 9.1: `loadActiveStories(storyListMain)` follows `loadChats`
    // after Ready (feeds the story tray). Parity slice: the notification
    // slice then fetches the saved-sound list and the three scope
    // defaults (`getSavedNotificationSounds`,
    // `getScopeNotificationSettings` × 3).
    let types: Vec<&str> = sent
        .iter()
        .map(|json| {
            if json.contains(r#""@type":"loadChats""#) {
                "loadChats"
            } else if json.contains(r#""@type":"loadActiveStories""#) {
                "loadActiveStories"
            } else if json.contains(r#""@type":"getSavedNotificationSounds""#) {
                "getSavedNotificationSounds"
            } else if json.contains(r#""@type":"getScopeNotificationSettings""#) {
                "getScopeNotificationSettings"
            } else {
                "other"
            }
        })
        .collect();
    assert_eq!(
        types,
        vec![
            "loadChats",
            "loadActiveStories",
            "getSavedNotificationSounds",
            "getScopeNotificationSettings",
            "getScopeNotificationSettings",
            "getScopeNotificationSettings",
        ]
    );
    let load = &sent[0];
    assert!(load.contains("chatListMain"));
    assert!(load.contains(&format!("\"limit\":{MAIN_CHAT_LOAD_LIMIT}")));
    let load_extra = driver
        .session
        .requests
        .has_purpose(RequestPurpose::LoadChats);
    assert!(load_extra);

    let new_chat = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(new_chat).unwrap();
    let position = copy_and_parse(
            r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"12","is_pinned":false}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(position).unwrap();
    assert_eq!(driver.session.ordered_chats()[0].id.0, 7);

    // In-flight loadChats: ingest of unrelated updates must not send another page.
    let loads_before = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_before, 1);

    assert!(
        driver
            .session
            .requests
            .has_purpose(RequestPurpose::LoadChats)
    );
    let load_ok = copy_and_parse(r#"{"@type":"ok","@extra":"1"}"#, &seq, &dyn_sink).unwrap();
    driver.ingest(load_ok).unwrap();
    // ok on loadChats means more may exist — one continuation page, not a per-tick loop.
    let loads_after_ok = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_after_ok, 2);

    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateUpdating"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count(),
        2,
        "unrelated ingest must not re-page loadChats"
    );

    let last_load = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("loadChats"))
        .unwrap();
    let v: Value = serde_json::from_str(&last_load).unwrap();
    let extra = v["@extra"].as_str().unwrap();
    let err404 = copy_and_parse(
        &format!(
            r#"{{"@type":"error","code":404,"message":"Not Found","@extra":"{id}"}}"#,
            id = extra
        ),
        &seq,
        &dyn_sink,
    )
    .unwrap();
    driver.ingest(err404).unwrap();
    assert!(driver.session.chats_exhausted);
    let loads_done = recorder
        .snapshot()
        .iter()
        .filter(|j| j.contains("loadChats"))
        .count();
    assert_eq!(loads_done, 2);
    driver
        .ingest(
            copy_and_parse(
                r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
                &seq,
                &dyn_sink,
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        recorder
            .snapshot()
            .iter()
            .filter(|j| j.contains("loadChats"))
            .count(),
        loads_done
    );
    assert!(!sink.rendered().contains("Not Found"));

    let history_extra = driver.select_chat(ChatId(7)).unwrap().expect("history");
    let sent = recorder.snapshot();
    assert!(
        sent.iter()
            .any(|j| j.contains("\"@type\":\"openChat\"") && j.contains("\"chat_id\":7")),
        "select_chat must send openChat"
    );
    let history_json = sent.last().unwrap();
    assert!(history_json.contains("getChatHistory"));
    assert!(history_json.contains("\"chat_id\":7"));
    assert!(history_json.contains(&format!("\"@extra\":\"{}\"", history_extra.0)));

    let messages = copy_and_parse(
            &format!(
                r#"{{"@type":"messages","@extra":"{}","messages":[{{"id":11,"chat_id":7,"is_outgoing":false,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"hi","entities":[]}}}}}}]}}"#,
                history_extra.0
            ),
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(messages).unwrap();
    assert!(
        driver
            .session
            .histories
            .get(&7)
            .unwrap()
            .messages
            .contains_key(&11)
    );
    let view_json = recorder
        .snapshot()
        .into_iter()
        .rev()
        .find(|j| j.contains("viewMessages"))
        .expect("viewMessages after history");
    let view: Value = serde_json::from_str(&view_json).unwrap();
    assert_eq!(view["@type"], "viewMessages");
    assert_eq!(view["chat_id"], 7);
    assert_eq!(view["message_ids"], serde_json::json!([11]));
    assert_eq!(view["source"]["@type"], "messageSourceChatHistory");
    assert_eq!(view["force_read"], true);
    assert!(!view_json.contains("hi"));

    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(7),
        driver.session.view_generation,
        "CANARYSENDping",
    );
    let send_extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    assert!(send_json.contains("sendMessage"));
    assert!(send_json.contains("\"topic_id\":null"));
    assert!(send_json.contains("CANARYSENDping"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));
    assert!(!sink.rendered().contains("CANARYSEND"));

    let channel_no_post = copy_and_parse(
            r#"{"@type":"updateNewChat","chat":{"id":8,"title":"News","type":{"@type":"chatTypeSupergroup","supergroup_id":8,"is_channel":true},"unread_count":0}}"#,
            &seq,
            &dyn_sink,
        )
        .unwrap();
    driver.ingest(channel_no_post).unwrap();
    let channel_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(8),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&channel_snap),
        Err(ConnectSendError::InvalidRequest)
    );
    assert!(!sink.rendered().contains("CANARYSEND"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_send_text_snapshot_in_topic_addresses_message_topic_forum() {
    // Parity slice 4: a send from a topic view carries
    // `topic_id = messageTopicForum{forum_topic_id}` (schema 1.8.67,
    // lines 12200 / 3004); a closed topic rejects the send.
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
    for json in [
        r#"{"@type":"updateNewChat","chat":{"id":16,"title":"Demo forum","type":{"@type":"chatTypeSupergroup","supergroup_id":16,"is_channel":false},"unread_count":0}}"#,
        r#"{"@type":"updateChatPosition","chat_id":16,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"25","is_pinned":false}}"#,
        r#"{"@type":"updateSupergroup","supergroup":{"@type":"supergroup","id":16,"is_forum":true}}"#,
    ] {
        driver
            .ingest(copy_and_parse(json, &seq, &dyn_sink).unwrap())
            .unwrap();
    }
    // Inject the topic list through the reducer; topic 3 is closed.
    let extra = driver
        .session
        .request(RequestPurpose::GetForumTopics, Some(ChatId(16)));
    let topics = [(2, "General", false), (3, "Random", true)]
            .iter()
            .map(|(id, name, closed)| {
                format!(
                    "{{\"info\":{{\"@type\":\"forumTopicInfo\",\"chat_id\":16,\"forum_topic_id\":{id},\"name\":\"{name}\",\"is_general\":false,\"is_closed\":{closed}}},\"order\":\"{id}\",\"is_pinned\":false,\"unread_count\":0}}"
                )
            })
            .collect::<Vec<_>>()
            .join(",");
    driver
            .ingest(
                copy_and_parse(
                    &format!(
                        "{{\"@type\":\"forumTopics\",\"@extra\":\"{}\",\"total_count\":2,\"topics\":[{topics}],\"next_offset_date\":0,\"next_offset_message_id\":0,\"next_offset_forum_topic_id\":0}}",
                        extra.0
                    ),
                    &seq,
                    &dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();

    driver.session.open_chat(ChatId(16));
    driver.session.select_topic(ChatId(16), 2);
    let snap = crate::composer::ComposerSnapshot::capture(
        ChatId(16),
        driver.session.view_generation,
        "CANARYTOPICping",
    );
    let send_extra = driver.send_text_snapshot(&snap).unwrap();
    let sent = recorder.snapshot();
    let send_json = sent.last().unwrap();
    let v: Value = serde_json::from_str(send_json).unwrap();
    assert_eq!(v["@type"], "sendMessage");
    assert_eq!(v["chat_id"], 16);
    assert_eq!(v["topic_id"]["@type"], "messageTopicForum");
    assert_eq!(v["topic_id"]["forum_topic_id"], 2);
    assert!(send_json.contains("CANARYTOPICping"));
    assert!(send_json.contains(&format!("\"@extra\":\"{}\"", send_extra.0)));

    // A closed topic rejects the send (the composer is hidden there;
    // this guards a stale-snapshot race).
    driver.session.select_topic(ChatId(16), 3);
    let closed_snap = crate::composer::ComposerSnapshot::capture(
        ChatId(16),
        driver.session.view_generation,
        "nope",
    );
    assert_eq!(
        driver.send_text_snapshot(&closed_snap),
        Err(ConnectSendError::InvalidRequest)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
