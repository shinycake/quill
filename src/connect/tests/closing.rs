//! Closing clients without blocking (`connect::closing`): a client that
//! never answers `close` costs one poll, not a wait; one that answers
//! Closed leaves the set; on quit every client shares one deadline.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::telegram::client::{OwnedEnvelope, copy_and_parse};
use crate::telegram::envelope::AuthorizationState;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CLOSING_JSON: &str = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosing"}}"#;
const CLOSED_JSON: &str = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#;

/// A ready client on the recording sender whose replies come from a
/// channel the test feeds (or never feeds: a client that never answers).
struct TestClient {
    driver: ConnectDriver<Arc<RecordingSender>>,
    rx: Receiver<OwnedEnvelope>,
    finished: Arc<AtomicBool>,
    auth: Arc<Mutex<AuthorizationState>>,
    dir: std::path::PathBuf,
}

impl Closable for TestClient {
    fn begin_close(&mut self) {
        crate::connect::closing::begin_close(&mut self.driver);
    }

    fn drain_closed(&mut self, budget: Duration) -> bool {
        let rx = &self.rx;
        let closed = crate::connect::closing::drain_until_closed(
            &mut self.driver,
            || rx.try_recv().ok(),
            budget,
        );
        *self.auth.lock().expect("auth") = self.driver.session.auth.clone();
        closed
    }

    fn finish(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Handle {
    recorder: Arc<RecordingSender>,
    tx: Sender<OwnedEnvelope>,
    finished: Arc<AtomicBool>,
    auth: Arc<Mutex<AuthorizationState>>,
    seq: AtomicU64,
    sink: Arc<dyn DiagnosticSink>,
}

impl Handle {
    fn answer(&self, json: &str) {
        let owned = copy_and_parse(json, &self.seq, &self.sink).expect("parse");
        self.tx.send(owned).expect("send");
    }

    fn sent(&self) -> Vec<String> {
        self.recorder.snapshot()
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }
}

fn client() -> (Box<dyn Closable>, Handle) {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    let (tx, rx) = mpsc::channel();
    let finished = Arc::new(AtomicBool::new(false));
    let auth = Arc::new(Mutex::new(driver.session.auth.clone()));
    let client = TestClient {
        driver,
        rx,
        finished: finished.clone(),
        auth: auth.clone(),
        dir,
    };
    let handle = Handle {
        recorder,
        tx,
        finished,
        auth,
        seq,
        sink,
    };
    (Box::new(client), handle)
}

fn key(n: u32) -> AccountKey {
    AccountKey::new(&format!("account-{n}")).expect("account key")
}

#[test]
fn push_sends_offline_then_close_and_returns_at_once() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    let started = Instant::now();
    set.push(key(1), client, false, deadline_after(CLOSE_GRACE));
    assert!(started.elapsed() < Duration::from_millis(100));
    let sent = handle.sent();
    let close = sent
        .iter()
        .position(|json| json.contains("\"@type\":\"close\""));
    let offline = sent
        .iter()
        .position(|json| json.contains("\"online\"") && json.contains("false"));
    assert!(close.is_some(), "close sent: {sent:?}");
    assert!(
        offline.is_some() && offline < close,
        "offline first: {sent:?}"
    );
    assert!(set.contains(&key(1)));
    assert!(!set.contains(&key(2)));
}

#[test]
fn a_client_that_never_answers_costs_one_poll_not_a_wait() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    let now = Instant::now();
    set.push(key(1), client, false, now + CLOSE_GRACE);
    let started = Instant::now();
    assert!(set.poll(Instant::now()).is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "poll waited {:?}",
        started.elapsed()
    );
    assert!(set.contains(&key(1)));
    assert!(!handle.finished());
    // Past the deadline it is finished and dropped, still without waiting.
    let done = set.poll(now + CLOSE_GRACE);
    assert_eq!(
        done,
        vec![ClosedClient {
            key: key(1),
            restart: false,
            timed_out: true
        }]
    );
    assert!(set.is_empty());
    assert!(handle.finished());
}

#[test]
fn a_client_that_answers_closed_leaves_the_set() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    set.push(key(2), client, true, deadline_after(CLOSE_GRACE));
    handle.answer(CLOSING_JSON);
    assert!(set.poll(Instant::now()).is_empty(), "Closing is not Closed");
    assert!(set.contains(&key(2)));
    handle.answer(CLOSED_JSON);
    let done = set.poll(Instant::now());
    assert_eq!(
        done,
        vec![ClosedClient {
            key: key(2),
            restart: true,
            timed_out: false
        }]
    );
    assert!(!set.contains(&key(2)));
    assert!(handle.finished());
}

#[test]
fn cancel_restarts_keeps_a_wiped_account_from_starting_again() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    set.push(key(3), client, true, deadline_after(CLOSE_GRACE));
    set.cancel_restarts();
    handle.answer(CLOSED_JSON);
    let done = set.poll(Instant::now());
    assert_eq!(done.len(), 1);
    assert!(done.iter().all(|closed| !closed.restart));
}

#[test]
fn quit_waits_for_everyone_together_within_one_deadline() {
    let mut set = ClosingSet::new();
    let mut handles = Vec::new();
    for n in 1..=4 {
        let (client, handle) = client();
        set.push(key(n), client, false, deadline_after(CLOSE_GRACE));
        handles.push(handle);
    }
    let shared = Duration::from_millis(300);
    let started = Instant::now();
    let timed_out = set.close_all_within(started + shared);
    let took = started.elapsed();
    assert_eq!(timed_out, 4);
    assert!(set.is_empty());
    assert!(took >= shared, "waited the shared deadline: {took:?}");
    // One deadline for all four, not 4 x 300 ms.
    assert!(took < shared * 2, "took {took:?}");
    assert!(handles.iter().all(Handle::finished));
}

#[test]
fn quit_returns_as_soon_as_every_client_is_closed() {
    let mut set = ClosingSet::new();
    let (a, a_handle) = client();
    let (b, b_handle) = client();
    set.push(key(1), a, false, deadline_after(CLOSE_GRACE));
    set.push(key(2), b, false, deadline_after(CLOSE_GRACE));
    a_handle.answer(CLOSED_JSON);
    let b_tx = b_handle.tx.clone();
    let late_closed = copy_and_parse(CLOSED_JSON, &b_handle.seq, &b_handle.sink).expect("parse");
    let answerer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let _ = b_tx.send(late_closed);
    });
    let started = Instant::now();
    let timed_out = set.close_all_within(started + Duration::from_secs(5));
    let _ = answerer.join();
    assert_eq!(timed_out, 0);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(a_handle.finished() && b_handle.finished());
    assert!(matches!(
        *b_handle.auth.lock().expect("auth"),
        AuthorizationState::Closed
    ));
}

#[test]
fn a_closed_client_gets_no_requests_and_leaves_on_the_first_poll() {
    let (mut client, handle) = client();
    handle.answer(CLOSED_JSON);
    // Reach Closed before it is handed over.
    assert!(client.drain_closed(Duration::from_millis(10)));
    let before = handle.sent().len();
    let mut set = ClosingSet::new();
    set.push(key(1), client, false, deadline_after(CLOSE_GRACE));
    assert_eq!(
        handle.sent().len(),
        before,
        "nothing sent to a Closed client"
    );
    assert_eq!(set.poll(Instant::now()).len(), 1);
}

#[test]
fn dropping_the_set_finishes_its_clients_within_the_quit_deadline() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    set.push(key(1), client, false, deadline_after(CLOSE_GRACE));
    let started = Instant::now();
    drop(set);
    let took = started.elapsed();
    assert!(handle.finished());
    assert!(
        took < QUIT_CLOSE_DEADLINE + Duration::from_millis(500),
        "{took:?}"
    );
}
