//! Connect-driver tests: shared fixtures and helper senders.
use super::super::*;
use super::{POLL_CLOSED_JSON, POLL_OPEN_JSON, READY_CALL_JSON};
use crate::calls::engine::MockEngine;
use crate::credentials::TelegramCredentials;
use crate::diagnostics::DiagnosticSink;
use crate::diagnostics::MemorySink;
use crate::ids::{AccountKey, ChatId};
use crate::platform::MemorySecretStore;
use crate::state::ActiveGroupCall;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::ParsedSession;
use serde_json::Value;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub(crate) static TDJSON_ENV_LOCK: StdMutex<()> = StdMutex::new(());

pub(crate) fn test_credentials() -> TelegramCredentials {
    TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    }
}

pub(crate) fn prepared_tmp(store: &MemorySecretStore) -> (std::path::PathBuf, PreparedConnect) {
    let dir = std::env::temp_dir().join(format!(
        "quill-connect-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let prepared =
        prepare_connect(&dir, AccountKey::primary(), store, &test_credentials()).expect("prepare");
    (dir, prepared)
}

/// Phase 6: ready driver with no chats (contacts tests don't need any).
pub(crate) fn ready_driver(
    recorder: &Arc<RecordingSender>,
    prepared: PreparedConnect,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    seq: &AtomicU64,
) -> ConnectDriver<Arc<RecordingSender>> {
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
}

pub(crate) struct ViewCtlSender {
    pub(crate) sent: Mutex<Vec<String>>,
    pub(crate) fail_view: Mutex<bool>,
}

impl ViewCtlSender {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            sent: Mutex::new(Vec::new()),
            fail_view: Mutex::new(false),
        })
    }

    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("view ctl sender").clone()
    }

    pub(crate) fn set_fail_view(&self, fail: bool) {
        *self.fail_view.lock().expect("view ctl sender") = fail;
    }

    pub(crate) fn view_count(&self) -> usize {
        self.snapshot()
            .iter()
            .filter(|j| j.contains("viewMessages"))
            .count()
    }
}

pub(crate) fn ready_private_chat(
    driver: &mut ConnectDriver<Arc<ViewCtlSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":1}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":11,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
                    seq,
                    sink,
                )
                .unwrap(),
            )
            .unwrap();
}

pub(crate) fn commit_typed_search<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    query: &str,
) -> SearchFlight {
    match driver.set_search_query(query).unwrap() {
        SearchQueryOutcome::Debounced { token } => driver
            .commit_debounced_search(token)
            .unwrap()
            .expect("debounced search"),
        other => panic!("expected Debounced, got {other:?}"),
    }
}

pub(crate) fn seed_ready_alice<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    seq: &AtomicU64,
    dyn_sink: &Arc<dyn DiagnosticSink>,
) {
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7},"unread_count":0}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver
            .ingest(
                copy_and_parse(
                    r#"{"@type":"updateNewMessage","message":{"id":50,"chat_id":7,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hello already here","entities":[]}}}}"#,
                    seq,
                    dyn_sink,
                )
                .unwrap(),
            )
            .unwrap();
    driver.select_chat(ChatId(7)).unwrap();
}

pub(crate) fn commit_typed_chat_search<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    query: &str,
) -> ChatSearchFlight {
    match driver.set_chat_search_query(query).unwrap() {
        ChatSearchQueryOutcome::Debounced { token } => driver
            .commit_debounced_chat_search(token)
            .unwrap()
            .expect("debounced chat search"),
        other => panic!("expected Debounced, got {other:?}"),
    }
}

/// Phase 9.2: ingest a full `story` object into the driver's session,
/// like the `getStory` response would.
pub(crate) fn seed_story<S: JsonSender>(
    driver: &mut ConnectDriver<S>,
    seq: &AtomicU64,
    dyn_sink: &Arc<dyn DiagnosticSink>,
    chat_id: i64,
    story_id: i32,
    content_type: &str,
    flags: &str,
) {
    let json = format!(
        r#"{{"@type":"story","id":{story_id},"poster_chat_id":{chat_id},"date":1700000000,"content":{{"@type":"{content_type}"}},{flags}"caption":{{"@type":"formattedText","text":"","entities":[]}}}}"#,
    );
    driver
        .ingest(copy_and_parse(&json, seq, dyn_sink).unwrap())
        .unwrap();
}

pub(crate) fn poll_driver() -> PollDriverHarness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    driver
        .ingest(copy_and_parse(POLL_OPEN_JSON, &seq, &dyn_sink).unwrap())
        .unwrap();
    driver
        .ingest(copy_and_parse(POLL_CLOSED_JSON, &seq, &dyn_sink).unwrap())
        .unwrap();
    (dir, driver, recorder, sink, dyn_sink, seq)
}

pub(crate) fn assert_invalid<T: std::fmt::Debug>(result: Result<T, ConnectSendError>) {
    match result {
        Err(ConnectSendError::InvalidRequest) => {}
        other => panic!("expected InvalidRequest, got {other:?}"),
    }
}

pub(crate) fn call_driver() -> CallDriverHarness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    (dir, driver, recorder, sink, AtomicU64::new(0))
}

pub(crate) fn ingest_call_json(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

pub(crate) fn sent_request(recorder: &RecordingSender, type_name: &str) -> Value {
    recorder
        .snapshot()
        .into_iter()
        .rev()
        .map(|json| serde_json::from_str::<Value>(&json).unwrap())
        .find(|value| value["@type"] == type_name)
        .unwrap_or_else(|| panic!("missing {type_name} request"))
}

pub(crate) fn seed_ready_call_user(
    driver: &mut ConnectDriver<Arc<RecordingSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) {
    ingest_call_json(
        driver,
        seq,
        sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        driver,
        seq,
        sink,
        r#"{"@type":"updateUser","user":{"id":41,"first_name":"Ada","type":{"@type":"userTypeRegular"}}}"#,
    );
}

pub(crate) fn ready_call_driver() -> (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    MockEngine,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
) {
    let (dir, mut driver, _recorder, sink, seq) = call_driver();
    let mock = MockEngine::new();
    let handle = mock.clone();
    driver.set_call_engine(Box::new(mock));
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest_call_json(
        &mut driver,
        &seq,
        &sink,
        r#"{"@type":"updateCall","call":{"@type":"call","id":77,"unique_id":"99","user_id":41,"is_outgoing":true,"is_video":false,"state":{"@type":"callStatePending","is_created":true,"is_received":false}}}"#,
    );
    (dir, driver, handle, sink, seq)
}

/// Phase C2b: sender that fails the next sendCallSignalingData send once,
/// then records. Mirrors the `ViewCtlSender` fail-one-message pattern.
pub(crate) struct FailFirstCallSender {
    pub(crate) sent: Mutex<Vec<String>>,
    pub(crate) fail_next: Mutex<bool>,
}

impl FailFirstCallSender {
    pub(crate) fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            fail_next: Mutex::new(true),
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<String> {
        self.sent.lock().expect("failing sender").clone()
    }
}

/// Bots slice: fails the first `getInlineQueryResults` send.
pub(crate) struct FailFirstInlineQuerySender {
    pub(crate) sent: Mutex<Vec<String>>,
    pub(crate) fail_next: Mutex<bool>,
}

impl FailFirstInlineQuerySender {
    pub(crate) fn new() -> Self {
        Self {
            sent: Mutex::new(Vec::new()),
            fail_next: Mutex::new(true),
        }
    }
}

pub(crate) fn failing_call_driver() -> FailingCallDriverHarness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let sender = Arc::new(FailFirstCallSender::new());
    let session = Session::new(AccountKey::primary(), sink.clone());
    let driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
    (dir, driver, sender, sink, AtomicU64::new(0))
}

pub(crate) fn ingest_failing(
    driver: &mut ConnectDriver<Arc<FailFirstCallSender>>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) -> Result<(), ConnectSendError> {
    driver.ingest(copy_and_parse(json, seq, sink).unwrap())
}

/// Phase C2e: video-call variant of `READY_CALL_JSON`.
pub(crate) fn ready_video_call_json() -> String {
    READY_CALL_JSON.replace("\"is_video\":false", "\"is_video\":true")
}

pub(crate) fn tracked_group_call(
    need_rejoin: bool,
    can_be_managed: bool,
    is_owned: bool,
) -> ActiveGroupCall {
    ActiveGroupCall {
        id: 77,
        title: "Team voice".into(),
        is_video_chat: false,
        is_joined: true,
        need_rejoin,
        can_be_managed,
        is_owned,
        is_muted_self: true,
        participant_count: 1,
        ..ActiveGroupCall::fresh(77)
    }
}

pub(crate) fn call_state_for_rejoin(driver: &mut ConnectDriver<Arc<RecordingSender>>) {
    let call = tracked_group_call(true, false, false);
    driver.session.active_group_call = Some(call);
    driver
        .session
        .active_group_call
        .as_mut()
        .unwrap()
        .reconnecting = true;
}

pub(crate) fn group_participant_json(
    user_id: i64,
    current: bool,
    video_info: &str,
    screen_info: &str,
) -> String {
    format!(
        r#"{{"@type":"updateGroupCallParticipant","group_call_id":555,"participant":{{"@type":"groupCallParticipant","participant_id":{{"@type":"messageSenderUser","user_id":{user_id}}},"audio_source_id":0,"screen_sharing_audio_source_id":0,"video_info":{video_info},"screen_sharing_video_info":{screen_info},"bio":"","is_current_user":{current},"is_speaking":false,"is_hand_raised":false,"can_be_muted_for_all_users":false,"can_be_unmuted_for_all_users":false,"can_be_muted_for_current_user":false,"can_be_unmuted_for_current_user":false,"is_muted_for_all_users":false,"is_muted_for_current_user":false,"can_unmute_self":false,"volume_level":10000,"order":"a1"}}}}"#,
    )
}

/// MED3: sender that fails every `downloadFile` send with a transport
/// error.
pub(crate) struct FailDownloadSender {
    pub(crate) sent: Mutex<Vec<String>>,
}

pub(crate) fn sessions_driver() -> SessionsDriverHarness {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink = Arc::new(MemorySink::new());
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    let recorder = Arc::new(RecordingSender::new());
    let session = Session::new(AccountKey::primary(), dyn_sink.clone());
    let mut driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    seed_ready_alice(&mut driver, &seq, &dyn_sink);
    (dir, driver, recorder, sink, dyn_sink, seq)
}

pub(crate) fn session_fixture(id: i64, current: bool, pending: bool) -> ParsedSession {
    ParsedSession {
        id,
        is_current: current,
        is_password_pending: pending,
        can_accept_secret_chats: false,
        can_accept_calls: true,
        device_model: format!("Device {id}"),
        application_name: "Quill".into(),
        application_version: "0.1".into(),
        platform: "Linux".into(),
        system_version: "6.8".into(),
        last_active_date: 1759000000,
        ip_address: "1.2.3.4".into(),
        location: "Austin".into(),
    }
}

// Phase 4.2: driver guards around `send_poll_answer` / `send_poll_draft`.
pub(crate) type PollDriverHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<MemorySink>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

pub(crate) type CallDriverHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

/// Phase C2b: driver harness with a failing-first sender.
pub(crate) type FailingCallDriverHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<FailFirstCallSender>>,
    Arc<FailFirstCallSender>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

/// Phase C2g: driver with a chat-bound unjoined group call (chat 51
/// -> call 555) and an available mock engine.
pub(crate) type GroupCallDriverHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    MockEngine,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);

// Slice A3: driver harness for the sessions flow.
pub(crate) type SessionsDriverHarness = (
    std::path::PathBuf,
    ConnectDriver<Arc<RecordingSender>>,
    Arc<RecordingSender>,
    Arc<MemorySink>,
    Arc<dyn DiagnosticSink>,
    AtomicU64,
);
