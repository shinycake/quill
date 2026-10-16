//! Closing clients without blocking (`connect::closing`): a client that
//! never answers `close` costs one poll, not a wait, and stays until TDLib
//! confirms Closed; one that answers Closed leaves the set; a dead receive
//! loop never counts as Closed; on quit every client shares one deadline.
//!
//! The clients here run `LiveConnect`'s own close code
//! (`begin_close_bridged` / `drain_bridged`) on the recording sender, fed
//! through a real routed `ReceiveBridge` of a test `TdHub`, so only the
//! native library is missing.
use super::super::*;
use super::*;
use crate::diagnostics::{DiagnosticSink, MemorySink};
use crate::ids::AccountKey;
use crate::platform::MemorySecretStore;
use crate::telegram::client::ReceiveBridge;
use crate::telegram::envelope::AuthorizationState;
use crate::telegram::hub::{ReceiveSource, TdHub};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CLIENT_ID: i32 = 1;

fn auth_json(state: &str) -> String {
    format!(
        r#"{{"@type":"updateAuthorizationState","authorization_state":{{"@type":"{state}"}},"@client_id":{CLIENT_ID}}}"#
    )
}

/// What `LiveConnect` is, minus the native library: a ready driver on the
/// recording sender and a bridge routed by the hub.
struct TestClient {
    driver: ConnectDriver<Arc<RecordingSender>>,
    bridge: ReceiveBridge,
    finished: Arc<AtomicBool>,
    auth: Arc<Mutex<AuthorizationState>>,
    dir: std::path::PathBuf,
}

impl Closable for TestClient {
    fn begin_close(&mut self) {
        crate::connect::closing::begin_close_bridged(&mut self.driver, &self.bridge);
    }

    fn drain(&mut self, budget: Duration) -> CloseProgress {
        let progress =
            crate::connect::closing::drain_bridged(&mut self.driver, &self.bridge, budget);
        *self.auth.lock().expect("auth") = self.driver.session.auth.clone();
        progress
    }

    fn finish(&mut self) {
        self.bridge.shutdown();
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
    /// What the hub's `td_receive` returns next.
    tx: Sender<String>,
    hub: Arc<TdHub>,
    finished: Arc<AtomicBool>,
    auth: Arc<Mutex<AuthorizationState>>,
}

impl Handle {
    fn answer(&self, state: &str) {
        self.tx.send(auth_json(state)).expect("send");
    }

    fn sent(&self) -> Vec<String> {
        self.recorder.snapshot()
    }

    fn finished(&self) -> bool {
        self.finished.load(Ordering::SeqCst)
    }

    fn auth(&self) -> AuthorizationState {
        self.auth.lock().expect("auth").clone()
    }
}

/// A hub reading from a channel the test feeds.
fn channel_hub() -> (Arc<TdHub>, Sender<String>) {
    let (tx, rx) = mpsc::channel::<String>();
    let source: ReceiveSource =
        Box::new(move |timeout| rx.recv_timeout(Duration::from_secs_f64(timeout)).ok());
    let hub = TdHub::with_source(source, Arc::new(MemorySink::new())).expect("hub thread");
    (Arc::new(hub), tx)
}

/// A hub whose receive loop hands over what the test sends, then dies once
/// the channel closes (its source keeps panicking).
fn dying_hub() -> (Arc<TdHub>, Sender<String>) {
    let (tx, rx) = mpsc::channel::<String>();
    let source: ReceiveSource = Box::new(move |_| match rx.recv() {
        Ok(json) => Some(json),
        Err(_) => panic!("receive source failed"),
    });
    let hub = TdHub::with_source(source, Arc::new(MemorySink::new())).expect("hub thread");
    (Arc::new(hub), tx)
}

fn client_on(hub: Arc<TdHub>, tx: Sender<String>) -> (Box<dyn Closable>, Handle) {
    let store = MemorySecretStore::new();
    let (dir, prepared) = prepared_tmp(&store);
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let seq = AtomicU64::new(0);
    let driver = ready_driver(&recorder, prepared, &sink, &seq);
    let bridge = hub.bridge(CLIENT_ID);
    // The hub numbers envelopes for every client (`seq`); the driver
    // already applied seq 1 (Ready) and drops anything not newer, so use
    // up the hub's first number.
    tx.send(format!(
        r#"{{"@type":"updateOption","name":"unix_time","value":{{"@type":"optionValueInteger","value":"1"}},"@client_id":{CLIENT_ID}}}"#
    ))
    .expect("send");
    assert!(bridge.next_timeout(Duration::from_secs(2)).is_some());
    let finished = Arc::new(AtomicBool::new(false));
    let auth = Arc::new(Mutex::new(driver.session.auth.clone()));
    let client = TestClient {
        driver,
        bridge,
        finished: finished.clone(),
        auth: auth.clone(),
        dir,
    };
    let handle = Handle {
        recorder,
        tx,
        hub,
        finished,
        auth,
    };
    (Box::new(client), handle)
}

fn client() -> (Box<dyn Closable>, Handle) {
    let (hub, tx) = channel_hub();
    client_on(hub, tx)
}

fn key(n: u32) -> AccountKey {
    AccountKey::new(&format!("account-{n}")).expect("account key")
}

/// Poll until `set` finishes something or two seconds pass (the hub hands
/// envelopes over on its own thread).
fn poll_until_done(set: &mut ClosingSet) -> Vec<ClosedClient> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let done = set.poll(Instant::now());
        if !done.is_empty() || Instant::now() >= deadline {
            return done;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn push_sends_offline_then_close_and_returns_at_once() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    let started = Instant::now();
    set.push(key(1), client, false);
    assert!(started.elapsed() < Duration::from_millis(500));
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
fn a_client_that_never_answers_stays_until_closed_without_waiting() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    let now = Instant::now();
    set.push(key(1), client, false);
    let started = Instant::now();
    assert!(set.poll(Instant::now()).is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "poll waited {:?}",
        started.elapsed()
    );
    // Long past the grace period it is still drained, never dropped: its
    // database may still be open.
    assert!(set.poll(now + CLOSE_GRACE * 10).is_empty());
    assert!(set.may_be_open(&key(1)));
    assert!(!handle.finished());
    handle.answer("authorizationStateClosed");
    let done = poll_until_done(&mut set);
    assert_eq!(
        done,
        vec![ClosedClient {
            key: key(1),
            restart: false,
            confirmed: true
        }]
    );
    assert!(!set.may_be_open(&key(1)));
    assert!(handle.finished());
}

#[test]
fn a_client_that_answers_closed_leaves_the_set() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    set.push(key(2), client, true);
    handle.answer("authorizationStateClosing");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !matches!(handle.auth(), AuthorizationState::Closing) && Instant::now() < deadline {
        assert!(set.poll(Instant::now()).is_empty(), "Closing is not Closed");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(handle.auth(), AuthorizationState::Closing));
    assert!(set.contains(&key(2)));
    handle.answer("authorizationStateClosed");
    let done = poll_until_done(&mut set);
    assert_eq!(
        done,
        vec![ClosedClient {
            key: key(2),
            restart: true,
            confirmed: true
        }]
    );
    assert!(!set.contains(&key(2)));
    assert!(handle.finished());
    // Its route is gone from the hub.
    assert!(!handle.hub.routes().is_registered(CLIENT_ID));
}

#[test]
fn a_dead_receive_loop_is_never_taken_for_closed() {
    let (hub, tx) = dying_hub();
    let (client, handle) = client_on(hub, tx);
    let mut set = ClosingSet::new();
    set.push(key(1), client, true);
    drop(handle.tx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.hub.stopped_unexpectedly() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(handle.hub.stopped_unexpectedly());
    let done = poll_until_done(&mut set);
    assert_eq!(
        done,
        vec![ClosedClient {
            key: key(1),
            restart: true,
            confirmed: false
        }]
    );
    assert!(set.is_empty(), "nothing more to drain");
    assert!(set.unconfirmed(&key(1)));
    assert!(
        set.may_be_open(&key(1)),
        "TDLib may still be closing it: never reopen or delete"
    );
    assert_eq!(set.keys(), vec![key(1)]);
}

#[test]
fn closed_queued_before_the_receive_loop_died_still_counts() {
    let (hub, tx) = dying_hub();
    let (client, handle) = client_on(hub, tx);
    let mut set = ClosingSet::new();
    set.push(key(1), client, false);
    handle.answer("authorizationStateClosed");
    drop(handle.tx);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !handle.hub.stopped_unexpectedly() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let done = poll_until_done(&mut set);
    assert_eq!(done.len(), 1);
    assert!(done.iter().all(|closed| closed.confirmed));
    assert!(!set.may_be_open(&key(1)));
}

#[test]
fn cancelling_restarts_keeps_an_account_from_starting_again() {
    let mut set = ClosingSet::new();
    let (a, a_handle) = client();
    let (b, b_handle) = client();
    set.push(key(3), a, true);
    set.push(key(4), b, true);
    set.cancel_restart(&key(3));
    a_handle.answer("authorizationStateClosed");
    b_handle.answer("authorizationStateClosed");
    let mut done = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while done.len() < 2 && Instant::now() < deadline {
        done.extend(set.poll(Instant::now()));
        std::thread::sleep(Duration::from_millis(5));
    }
    done.sort_by(|x, y| x.key.0.cmp(&y.key.0));
    assert_eq!(
        done.iter().map(|closed| closed.restart).collect::<Vec<_>>(),
        vec![false, true]
    );
    let (c, c_handle) = client();
    set.push(key(5), c, true);
    set.cancel_restarts();
    c_handle.answer("authorizationStateClosed");
    let done = poll_until_done(&mut set);
    assert!(done.iter().all(|closed| !closed.restart));
}

#[test]
fn quit_waits_for_everyone_together_within_one_deadline() {
    let mut set = ClosingSet::new();
    let mut handles = Vec::new();
    for n in 1..=4 {
        let (client, handle) = client();
        set.push(key(n), client, false);
        handles.push(handle);
    }
    let shared = Duration::from_millis(300);
    let started = Instant::now();
    let unconfirmed = set.close_all_within(started + shared);
    let took = started.elapsed();
    assert_eq!(unconfirmed, 4);
    assert!(set.is_empty());
    assert!(took >= shared, "waited the shared deadline: {took:?}");
    // One deadline for all four, not 4 x 300 ms (generous for a loaded CI).
    assert!(took < shared * 3, "took {took:?}");
    assert!(handles.iter().all(Handle::finished));
    // Cut off, not confirmed: still may be open for this process.
    assert!((1..=4).all(|n| set.may_be_open(&key(n))));
}

#[test]
fn quit_returns_as_soon_as_every_client_is_closed() {
    let mut set = ClosingSet::new();
    let (a, a_handle) = client();
    let (b, b_handle) = client();
    set.push(key(1), a, false);
    set.push(key(2), b, false);
    a_handle.answer("authorizationStateClosed");
    let b_tx = b_handle.tx.clone();
    let answerer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        let _ = b_tx.send(auth_json("authorizationStateClosed"));
    });
    let started = Instant::now();
    let unconfirmed = set.close_all_within(started + Duration::from_secs(5));
    let _ = answerer.join();
    assert_eq!(unconfirmed, 0);
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(a_handle.finished() && b_handle.finished());
    assert!(matches!(b_handle.auth(), AuthorizationState::Closed));
    assert!(!set.may_be_open(&key(1)) && !set.may_be_open(&key(2)));
}

#[test]
fn a_closed_client_gets_no_requests_and_leaves_on_the_first_poll() {
    let (mut client, handle) = client();
    handle.answer("authorizationStateClosed");
    // Reach Closed before it is handed over.
    let deadline = Instant::now() + Duration::from_secs(2);
    while client.drain(Duration::from_millis(10)) != CloseProgress::Closed
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let before = handle.sent().len();
    let mut set = ClosingSet::new();
    set.push(key(1), client, false);
    assert_eq!(
        handle.sent().len(),
        before,
        "nothing sent to a Closed client"
    );
    assert_eq!(set.poll(Instant::now()).len(), 1);
}

#[test]
fn a_shut_down_bridge_is_sent_nothing_and_never_counts_as_closed() {
    let (hub, tx) = channel_hub();
    let (mut client, handle) = client_on(hub, tx);
    client.finish();
    let before = handle.sent().len();
    let mut set = ClosingSet::new();
    set.push(key(1), client, false);
    assert_eq!(handle.sent().len(), before);
    let done = set.poll(Instant::now());
    assert_eq!(done.len(), 1);
    assert!(done.iter().all(|closed| !closed.confirmed));
    assert!(set.may_be_open(&key(1)));
}

#[test]
fn dropping_the_set_finishes_its_clients_within_the_quit_deadline() {
    let (client, handle) = client();
    let mut set = ClosingSet::new();
    set.push(key(1), client, false);
    let started = Instant::now();
    drop(set);
    let took = started.elapsed();
    assert!(handle.finished());
    assert!(took < QUIT_CLOSE_DEADLINE * 3, "{took:?}");
}
