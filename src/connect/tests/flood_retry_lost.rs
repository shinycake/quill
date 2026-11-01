//! A flood-retry re-send that cannot go out settles the request through
//! the same failure path an answer would take.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::{AccountKey, ChatId, RequestId};
use crate::platform::MemorySecretStore;
use crate::state::Session;
use crate::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub(super) struct SwitchSender {
    pub(super) inner: Arc<RecordingSender>,
    pub(super) down: Arc<AtomicBool>,
}

impl JsonSender for SwitchSender {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(ConnectSendError::Native);
        }
        self.inner.send_json(request)
    }

    fn take_retained(&self, extra: RequestId) -> Option<String> {
        self.inner.take_retained(extra)
    }

    fn forget_retained(&self, extra: RequestId) {
        self.inner.forget_retained(extra)
    }
}

fn ingest(
    driver: &mut ConnectDriver<SwitchSender>,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
    json: &str,
) {
    driver
        .ingest(copy_and_parse(json, seq, sink).unwrap())
        .unwrap();
}

#[test]
fn failed_resend_after_a_flood_wait_offers_the_history_retry() {
    let store = MemorySecretStore::new();
    let (_dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let down = Arc::new(AtomicBool::new(false));
    let sender = SwitchSender {
        inner: Arc::new(RecordingSender::new()),
        down: down.clone(),
    };
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, sender, test_credentials(), prepared);
    let seq = AtomicU64::new(0);
    for json in [
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
        r#"{"@type":"updateConnectionState","state":{"@type":"connectionStateReady"}}"#,
        r#"{"@type":"updateNewChat","chat":{"id":7,"title":"Alice","type":{"@type":"chatTypePrivate","user_id":7}}}"#,
        r#"{"@type":"updateChatPosition","chat_id":7,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"9","is_pinned":false}}"#,
    ] {
        ingest(&mut driver, &seq, &sink, json);
    }
    let extra = driver.select_chat(ChatId(7)).unwrap().expect("first page");
    assert!(driver.session.history_loading(ChatId(7)));

    ingest(
        &mut driver,
        &seq,
        &sink,
        &format!(
            r#"{{"@type":"error","@extra":"{}","code":429,"message":"Too Many Requests: retry after 3"}}"#,
            extra.0
        ),
    );
    assert!(driver.session.requests.get(extra).is_some());

    down.store(true, Ordering::SeqCst);
    assert!(driver.request_tick(Instant::now() + Duration::from_secs(10)));
    assert!(driver.session.requests.get(extra).is_none());
    assert!(driver.session.history_load_failed(ChatId(7)));
    assert!(!driver.session.history_loading(ChatId(7)));
}
