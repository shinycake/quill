//! Q1 / R5: rate-limit retries for idempotent reads, the user-action flood
//! notice, the stale pending-request sweep, and the history Retry state.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::{PENDING_REQUEST_TTL, RequestPurpose, Session};
use crate::telegram::client::copy_and_parse;
use crate::telegram::envelope::{ErrorClass, TdError};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

struct Harness {
    driver: ConnectDriver<Arc<RecordingSender>>,
    recorder: Arc<RecordingSender>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
    _dir: std::path::PathBuf,
}

impl Harness {
    /// Chat 7 (read, nothing unread), connection ready.
    fn new() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let recorder = Arc::new(RecordingSender::new());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let driver = ConnectDriver::new(session, recorder.clone(), test_credentials(), prepared);
        let mut h = Harness {
            driver,
            recorder,
            seq: AtomicU64::new(0),
            sink,
            _dir: dir,
        };
        h.ingest(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#);
        h.ingest(r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#);
        h.ingest(r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7}}}"#);
        h.ingest(r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#);
        h
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).unwrap())
            .unwrap();
    }

    fn flood(&mut self, extra: RequestId, message: &str) {
        self.ingest(&format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"{message}"}}"#,
            extra.0
        ));
    }

    fn sends_of(&self, extra: RequestId) -> usize {
        self.recorder
            .snapshot()
            .iter()
            .filter(|json| {
                json.contains("\"@type\":\"getChatHistory\"")
                    && json.contains(&format!("\"@extra\":\"{}\"", extra.0))
            })
            .count()
    }

    fn open_history(&mut self) -> RequestId {
        self.driver
            .select_chat(ChatId(7))
            .unwrap()
            .expect("first page")
    }
}

#[test]
fn retry_after_is_parsed_from_both_flood_messages() {
    use crate::telegram::envelope::parse_flood_wait_secs;
    assert_eq!(parse_flood_wait_secs("FLOOD_WAIT_30"), Some(30));
    assert_eq!(
        parse_flood_wait_secs("Too Many Requests: retry after 7"),
        Some(7)
    );
    assert_eq!(
        parse_flood_wait_secs("Too Many Requests: retry after x"),
        None
    );
    assert_eq!(parse_flood_wait_secs("CHAT_ADMIN_REQUIRED"), None);
    // A 429 without a number is still a flood, just without a countdown.
    let err = TdError::from_code(429);
    assert_eq!(err.class, ErrorClass::Flood);
    assert_eq!(
        err.flood_notice().as_deref(),
        Some("Too many attempts. Try again later.")
    );
}

#[test]
fn flood_notice_names_the_wait() {
    let mut err = TdError::from_code(429);
    err.flood_wait_secs = Some(12);
    assert_eq!(
        err.flood_notice().as_deref(),
        Some("Too many attempts. Try again in 12 seconds.")
    );
    err.flood_wait_secs = Some(1);
    assert_eq!(
        err.flood_notice().as_deref(),
        Some("Too many attempts. Try again in 1 second.")
    );
    assert!(TdError::from_code(400).flood_notice().is_none());
}

#[test]
fn rate_limited_history_page_is_retried_after_the_wait() {
    let mut h = Harness::new();
    let extra = h.open_history();
    assert_eq!(h.sends_of(extra), 1);

    h.flood(extra, "Too Many Requests: retry after 2");
    // Nothing surfaced: the request stays pending, so the dedupe guard
    // stays closed and no duplicate goes out meanwhile.
    assert!(h.driver.session.requests.get(extra).is_some());
    assert!(!h.driver.session.history_load_failed(ChatId(7)));
    assert!(h.driver.session.flood_notice.is_none());
    assert!(h.driver.fetch_history().unwrap().is_none());
    assert_eq!(h.sends_of(extra), 1);

    let t0 = Instant::now();
    h.driver.request_tick(t0 + Duration::from_secs(1));
    assert_eq!(h.sends_of(extra), 1, "still waiting");
    h.driver.request_tick(t0 + Duration::from_secs(4));
    assert_eq!(h.sends_of(extra), 2, "re-sent with the same @extra");
    // The sweep never took it: it was re-stamped by the retry.
    assert!(h.driver.session.requests.get(extra).is_some());
}

#[test]
fn flood_retries_are_capped_and_then_surface() {
    let mut h = Harness::new();
    let extra = h.open_history();
    let mut now = Instant::now();
    for attempt in 1..=FLOOD_RETRY_MAX_ATTEMPTS {
        h.flood(extra, "Too Many Requests: retry after 1");
        now += Duration::from_secs(5);
        h.driver.request_tick(now);
        assert_eq!(h.sends_of(extra), usize::from(attempt) + 1);
    }
    // The next 429 exceeds the cap: the error reaches the reducer and the
    // empty window offers Retry.
    h.flood(extra, "Too Many Requests: retry after 1");
    assert!(h.driver.session.requests.get(extra).is_none());
    assert!(h.driver.session.history_load_failed(ChatId(7)));
    assert_eq!(h.sends_of(extra), usize::from(FLOOD_RETRY_MAX_ATTEMPTS) + 1);
}

#[test]
fn waits_beyond_the_cap_surface_immediately() {
    let mut h = Harness::new();
    let extra = h.open_history();
    h.flood(extra, "Too Many Requests: retry after 600");
    assert!(h.driver.session.requests.get(extra).is_none());
    assert!(h.driver.session.history_load_failed(ChatId(7)));
    h.driver
        .request_tick(Instant::now() + Duration::from_secs(900));
    assert_eq!(h.sends_of(extra), 1, "no retry was scheduled");
}

#[test]
fn user_actions_are_not_retried_and_show_the_flood_notice() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .session
        .request(RequestPurpose::SendMessage, Some(ChatId(7)));
    h.flood(extra, "Too Many Requests: retry after 12");
    assert_eq!(
        h.driver.session.flood_notice.take().as_deref(),
        Some("Too many attempts. Try again in 12 seconds.")
    );
    h.driver
        .request_tick(Instant::now() + Duration::from_secs(60));
    assert!(
        h.recorder
            .snapshot()
            .iter()
            .all(|j| !j.contains("sendMessage"))
    );
}

#[test]
fn background_lookups_stay_quiet_when_flood_persists() {
    let mut h = Harness::new();
    let extra = h
        .driver
        .session
        .request(RequestPurpose::GetUserFullInfo, None);
    h.flood(extra, "FLOOD_WAIT_900");
    assert!(h.driver.session.flood_notice.is_none());
}

#[test]
fn failed_send_with_flood_keeps_the_retry_affordance() {
    let mut h = Harness::new();
    h.ingest(
        r#"{"@type":"updateMessageSendFailed","old_message_id":-1,"error":{"code":429,"message":"Too Many Requests: retry after 9"},"message":{"@type":"message","id":5,"chat_id":7,"is_outgoing":true,"date":1700000000,"sending_state":{"@type":"messageSendingStateFailed","error":{"code":429,"message":"Too Many Requests: retry after 9"},"can_retry":true,"need_another_sender":false,"retry_after":9},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#,
    );
    assert_eq!(
        h.driver.session.flood_notice.take().as_deref(),
        Some("Too many attempts. Try again in 9 seconds.")
    );
    let row = h
        .driver
        .session
        .histories
        .get(&7)
        .and_then(|history| history.messages.get(&5))
        .expect("failed row kept");
    assert!(
        row.failed && row.can_retry,
        "text stays, Retry send offered"
    );
}

#[test]
fn stale_history_request_is_swept_into_the_retry_state() {
    let mut h = Harness::new();
    let extra = h.open_history();
    assert!(h.driver.session.history_loading(ChatId(7)));
    h.driver
        .request_tick(Instant::now() + PENDING_REQUEST_TTL + Duration::from_secs(1));
    assert!(h.driver.session.requests.get(extra).is_none());
    assert!(h.driver.session.history_load_failed(ChatId(7)));
    assert!(!h.driver.session.history_loading(ChatId(7)));
    // Auto-paging stays quiet until the user presses Retry.
    let before = h.recorder.snapshot().len();
    assert!(h.driver.fetch_history().unwrap().is_none());
    assert_eq!(h.recorder.snapshot().len(), before);

    let retry = h.driver.retry_history().unwrap().expect("retry sends");
    assert_ne!(retry, extra);
    assert!(!h.driver.session.history_load_failed(ChatId(7)));
    assert!(h.driver.session.history_loading(ChatId(7)));
}

#[test]
fn history_error_answer_also_offers_retry() {
    let mut h = Harness::new();
    let extra = h.open_history();
    h.ingest(&format!(
        r#"{{"@type":"error","@extra":"{}","code":500,"message":"Timeout"}}"#,
        extra.0
    ));
    assert!(h.driver.session.history_load_failed(ChatId(7)));
    assert!(!h.driver.session.history_loading(ChatId(7)));
}

#[test]
fn sweep_only_touches_dedupe_guarded_purposes_and_a_live_connection() {
    let mut h = Harness::new();
    let history = h.open_history();
    let download = h.driver.session.request(RequestPurpose::DownloadFile, None);
    let upload_like = h
        .driver
        .session
        .request(RequestPurpose::SendMessage, Some(ChatId(7)));
    let late = Instant::now() + PENDING_REQUEST_TTL + Duration::from_secs(1);

    // Not before the TTL.
    h.driver
        .request_tick(Instant::now() + Duration::from_secs(5));
    assert!(h.driver.session.requests.get(history).is_some());

    // Offline: TDLib queues requests, so nothing is swept.
    h.ingest(
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateWaitingForNetwork"}}"#,
    );
    h.driver.request_tick(late);
    assert!(h.driver.session.requests.get(history).is_some());

    h.ingest(r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#);
    h.driver.request_tick(late + Duration::from_secs(30));
    assert!(h.driver.session.requests.get(history).is_none());
    assert!(h.driver.session.requests.get(download).is_some());
    assert!(h.driver.session.requests.get(upload_like).is_some());
}

#[test]
fn flooded_request_is_exempt_from_the_sweep_while_it_waits() {
    let mut h = Harness::new();
    let extra = h.open_history();
    h.flood(extra, "Too Many Requests: retry after 60");
    // Due at +61 s, but the TTL (60 s since the original send) has passed
    // at +60.5 s: the entry survives because it is waiting to be re-sent.
    h.driver
        .request_tick(Instant::now() + Duration::from_millis(60_500));
    assert!(h.driver.session.requests.get(extra).is_some());
    assert!(!h.driver.session.history_load_failed(ChatId(7)));
}

#[test]
fn only_idempotent_reads_are_retained_for_retry() {
    let stash = RetryStash::default();
    stash.note(r#"{"@type":"getChatHistory","@extra":"3","chat_id":7}"#);
    stash.note(r#"{"@type":"sendMessage","@extra":"4","chat_id":7}"#);
    stash.note(r#"{"@type":"checkAuthenticationPassword","@extra":"5","password":"x"}"#);
    assert_eq!(stash.len(), 1);
    assert!(stash.take(RequestId(4)).is_none());
    assert!(stash.take(RequestId(3)).is_some());
    assert!(stash.is_empty());
}
