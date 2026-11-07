//! A second call ringing during a call, and hang-ups that fail.
use super::*;
use crate::ids::RequestId;
use crate::notify_accounts::ringing_incoming_call;
use crate::telegram::envelope::CallState;
use std::sync::Mutex;

const READY_AUTH: &str = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#;

fn active_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"98","user_id":{user_id},"is_outgoing":true,"is_video":false,"state":{{"@type":"callStateExchangingKeys"}}}}}}"#
    )
}

fn pending_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"99","user_id":{user_id},"is_outgoing":false,"is_video":false,"state":{{"@type":"callStatePending","is_created":true,"is_received":false}}}}}}"#
    )
}

fn discarded_json(id: i32, user_id: i64) -> String {
    format!(
        r#"{{"@type":"updateCall","call":{{"@type":"call","id":{id},"unique_id":"97","user_id":{user_id},"is_outgoing":false,"is_video":false,"state":{{"@type":"callStateDiscarded","reason":{{"@type":"callDiscardReasonHungUp"}},"need_rating":false,"need_debug_information":false,"need_log":false}}}}}}"#
    )
}

fn error_json(request: RequestId) -> String {
    format!(
        r#"{{"@type":"error","@extra":"{}","code":400,"message":"CALL_PEER_INVALID"}}"#,
        request.as_extra()
    )
}

/// Records every request, failing those of one TDLib type while set.
#[derive(Default)]
struct FailTypeSender {
    sent: Mutex<Vec<String>>,
    fail: Mutex<Option<&'static str>>,
}

impl FailTypeSender {
    fn fail(&self, type_name: Option<&'static str>) {
        *self.fail.lock().expect("fail type sender") = type_name;
    }

    fn sent_types(&self) -> Vec<String> {
        self.sent
            .lock()
            .expect("fail type sender")
            .iter()
            .map(|json| {
                let value: Value = serde_json::from_str(json).expect("request json");
                value["@type"].as_str().unwrap_or_default().to_string()
            })
            .collect()
    }
}

impl JsonSender for Arc<FailTypeSender> {
    fn send_json(&self, request: &str) -> Result<(), ConnectSendError> {
        if let Some(type_name) = *self.fail.lock().expect("fail type sender")
            && request.contains(&format!(r#""@type":"{type_name}""#))
        {
            return Err(ConnectSendError::Native);
        }
        self.sent
            .lock()
            .expect("fail type sender")
            .push(request.to_string());
        Ok(())
    }
}

struct Harness {
    dir: std::path::PathBuf,
    driver: ConnectDriver<Arc<FailTypeSender>>,
    sender: Arc<FailTypeSender>,
    engine: MockEngine,
    sink: Arc<dyn DiagnosticSink>,
    seq: AtomicU64,
}

impl Harness {
    /// In call 77 with user 41; user 42's call 78 raised the swap prompt.
    fn in_call_with_prompt() -> Self {
        let store = MemorySecretStore::new();
        let (dir, prepared) = prepared_tmp(&store);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let sender = Arc::new(FailTypeSender::default());
        let session = Session::new(AccountKey::primary(), sink.clone());
        let mut driver = ConnectDriver::new(session, sender.clone(), test_credentials(), prepared);
        let engine = MockEngine::new();
        driver.set_call_engine(Box::new(engine.clone()));
        let mut harness = Self {
            dir,
            driver,
            sender,
            engine,
            sink,
            seq: AtomicU64::new(0),
        };
        harness.ingest(READY_AUTH);
        harness.ingest(&active_json(77, 41));
        harness.ingest(&pending_json(78, 42));
        assert_eq!(
            harness.driver.session.calls.second_call.prompt(),
            Some((78, 42, false))
        );
        harness
    }

    fn ingest(&mut self, json: &str) {
        self.driver
            .ingest(copy_and_parse(json, &self.seq, &self.sink).expect("parse"))
            .expect("ingest");
    }

    fn state(&self) -> Option<(i32, CallState)> {
        self.driver
            .session
            .calls
            .active_call
            .as_ref()
            .map(|call| (call.id, call.state.clone()))
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn the_second_call_rings_when_the_call_ends_under_the_prompt() {
    let mut h = Harness::in_call_with_prompt();
    h.ingest(&discarded_json(77, 41));
    assert_eq!(
        ringing_incoming_call(&h.driver.session),
        Some((78, 42, false))
    );
    assert_eq!(h.driver.session.calls.second_call.prompt(), None);
    // The old call's media goes, the second call's starts.
    assert_eq!(h.engine.hung_up_calls(), vec![77]);
    assert_eq!(h.engine.started_calls().last(), Some(&(78, 42, false)));
    // And it can be answered.
    h.driver.accept_call().expect("answer the second call");
    assert_eq!(
        h.sender.sent_types().last().map(String::as_str),
        Some("acceptCall")
    );
}

#[test]
fn a_swap_accept_that_cannot_be_sent_leaves_the_call_ringing() {
    let mut h = Harness::in_call_with_prompt();
    h.driver.accept_swap_call().expect("swap");
    h.sender.fail(Some("acceptCall"));
    h.ingest(&discarded_json(77, 41));
    assert!(!h.sender.sent_types().iter().any(|t| t == "acceptCall"));
    assert_eq!(
        ringing_incoming_call(&h.driver.session),
        Some((78, 42, false))
    );
    assert!(h.driver.session.calls.error.is_some(), "the failure shows");
    assert_eq!(h.driver.session.calls.answered_call, None);
    // Answering again works once sending does.
    h.sender.fail(None);
    h.driver.accept_call().expect("answer again");
    assert_eq!(
        h.sender.sent_types().last().map(String::as_str),
        Some("acceptCall")
    );
}

#[test]
fn a_failed_hang_up_puts_the_call_and_the_swap_prompt_back() {
    let mut h = Harness::in_call_with_prompt();
    h.driver.accept_swap_call().expect("swap");
    assert_eq!(h.state(), Some((77, CallState::HangingUp)));
    let request = h
        .driver
        .session
        .calls
        .hang_up
        .as_ref()
        .map(|hang_up| hang_up.request)
        .expect("hang-up in flight");
    h.ingest(&error_json(request));
    assert_eq!(h.state(), Some((77, CallState::ExchangingKeys)));
    assert_eq!(
        h.driver.session.calls.second_call.prompt(),
        Some((78, 42, false))
    );
    assert!(h.driver.session.calls.error.is_some());
    // No accept fires for a call the person no longer agreed to.
    assert!(!h.sender.sent_types().iter().any(|t| t == "acceptCall"));
}

#[test]
fn a_queued_update_does_not_undo_a_hang_up_in_flight() {
    let mut h = Harness::in_call_with_prompt();
    h.driver.discard_call().expect("hang up");
    h.ingest(&active_json(77, 41));
    assert_eq!(h.state(), Some((77, CallState::HangingUp)));
    assert!(!h.driver.call_in_progress(), "nothing left to hang up");
}

#[test]
fn hanging_up_twice_sends_one_discard() {
    let mut h = Harness::in_call_with_prompt();
    let first = h.driver.discard_call().expect("hang up");
    let second = h.driver.discard_call().expect("hang up again");
    assert_eq!(first, second);
    let discards = h
        .sender
        .sent_types()
        .iter()
        .filter(|t| *t == "discardCall")
        .count();
    assert_eq!(discards, 1);
}
