use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::envelope::{AuthPayload, AuthorizationState, EnvelopePayload};
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

/// A hub reading from a channel, so tests decide what `td_receive` returns.
fn channel_hub() -> (Arc<TdHub>, Sender<String>, Arc<MemorySink>) {
    let (tx, rx) = mpsc::channel::<String>();
    let source: ReceiveSource =
        Box::new(move |timeout| rx.recv_timeout(Duration::from_secs_f64(timeout)).ok());
    let sink = Arc::new(MemorySink::new());
    let hub = TdHub::with_source(source, wake_on(&tx), sink.clone()).expect("hub thread");
    (Arc::new(hub), tx, sink)
}

fn wake_on(tx: &Sender<String>) -> ReceiveWake {
    let tx = tx.clone();
    Box::new(move || {
        let _ = tx.send(String::new());
    })
}

fn no_wake() -> ReceiveWake {
    Box::new(|| {})
}

fn option_update(client: Option<i32>, value: i64) -> String {
    let client = client
        .map(|id| format!(r#","@client_id":{id}"#))
        .unwrap_or_default();
    format!(
        r#"{{"@type":"updateOption","name":"unix_time","value":{{"@type":"optionValueInteger","value":"{value}"}}{client}}}"#
    )
}

fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    done()
}

#[test]
fn envelopes_go_to_the_client_they_name_in_order() {
    let (hub, tx, sink) = channel_hub();
    let first = hub.bridge(1);
    let second = hub.bridge(2);
    for json in [
        option_update(Some(1), 10),
        option_update(Some(2), 20),
        option_update(Some(3), 30),
        option_update(None, 40),
        option_update(Some(1), 11),
    ] {
        tx.send(json).unwrap();
    }
    let a = first.next_timeout(Duration::from_secs(2)).unwrap();
    let b = first.next_timeout(Duration::from_secs(2)).unwrap();
    let c = second.next_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(
        (a.client_id, b.client_id, c.client_id),
        (Some(1), Some(1), Some(2))
    );
    assert!(a.seq < b.seq, "one client's envelopes keep their order");
    assert!(matches!(a.envelope.payload, EnvelopePayload::Common(_)));
    // Client 3 is not registered and the last update names no client:
    // both are dropped, never handed to another account.
    assert!(first.next_timeout(Duration::from_millis(100)).is_none());
    assert!(second.next_timeout(Duration::from_millis(100)).is_none());
    assert!(wait_until(|| sink
        .rendered()
        .matches("unrouted-client")
        .count()
        == 2));
}

#[test]
fn a_shut_down_bridge_stops_receiving_and_frees_its_route() {
    let (hub, tx, sink) = channel_hub();
    let mut closing = hub.bridge(5);
    let staying = hub.bridge(6);
    assert!(!closing.is_joined());
    closing.shutdown();
    assert!(closing.is_joined());
    assert!(!hub.routes().is_registered(5));
    assert!(hub.routes().is_registered(6));
    tx.send(option_update(Some(5), 1)).unwrap();
    tx.send(option_update(Some(6), 2)).unwrap();
    assert_eq!(
        staying
            .next_timeout(Duration::from_secs(2))
            .and_then(|owned| owned.client_id),
        Some(6)
    );
    assert!(wait_until(|| sink.rendered().contains("unrouted-client")));
    // Shutting down twice is harmless.
    closing.shutdown();
}

#[test]
fn a_dropped_reader_loses_its_route() {
    let (hub, tx, sink) = channel_hub();
    let routes = hub.routes();
    let rx = routes.register(9);
    drop(rx);
    tx.send(option_update(Some(9), 1)).unwrap();
    assert!(wait_until(|| !routes.is_registered(9)));
    assert!(sink.rendered().contains("client-reader-gone"));
}

#[test]
fn deliver_reports_each_outcome() {
    let routes = Routes::default();
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let parse = |json: String| copy_and_parse(&json, &seq, &sink).unwrap();
    let rx = routes.register(4);
    assert_eq!(
        routes.deliver(parse(option_update(Some(4), 1))),
        Delivery::Delivered
    );
    assert_eq!(rx.try_recv().unwrap().client_id, Some(4));
    assert_eq!(
        routes.deliver(parse(option_update(Some(8), 1))),
        Delivery::Unrouted
    );
    assert_eq!(
        routes.deliver(parse(option_update(None, 1))),
        Delivery::Unrouted
    );
    drop(rx);
    assert_eq!(
        routes.deliver(parse(option_update(Some(4), 1))),
        Delivery::ReaderGone
    );
    assert!(!routes.is_registered(4));
}

#[test]
fn the_receive_thread_sleeps_while_no_client_is_registered() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let source: ReceiveSource = Box::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(5));
        None
    });
    let hub = Arc::new(TdHub::with_source(source, no_wake(), Arc::new(MemorySink::new())).unwrap());
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no client, no td_receive");
    let mut bridge = hub.bridge(1);
    assert!(wait_until(|| calls.load(Ordering::SeqCst) > 0));
    bridge.shutdown();
}

#[test]
fn a_dead_receive_thread_marks_every_bridge_stopped() {
    let (tx, rx) = mpsc::channel::<()>();
    let source: ReceiveSource = Box::new(move |_| {
        let _ = rx.recv();
        panic!("receive source failed");
    });
    let hub = Arc::new(TdHub::with_source(source, no_wake(), Arc::new(MemorySink::new())).unwrap());
    let first = hub.bridge(1);
    let second = hub.bridge(2);
    assert!(!first.stopped_unexpectedly());
    // The loop survives single panics; a source that keeps failing (the
    // closed channel panics at once from now on) ends it.
    tx.send(()).unwrap();
    drop(tx);
    assert!(wait_until(|| first.stopped_unexpectedly()));
    assert!(second.stopped_unexpectedly());
    assert!(hub.stopped_unexpectedly());
}

#[test]
fn one_panicking_envelope_does_not_end_the_receive_thread() {
    let (tx, rx) = mpsc::channel::<String>();
    let mut panicked = false;
    let source: ReceiveSource = Box::new(move |timeout| {
        let json = rx.recv_timeout(Duration::from_secs_f64(timeout)).ok()?;
        if !panicked {
            panicked = true;
            panic!("one envelope failed to parse");
        }
        Some(json)
    });
    let hub = TdHub::with_source(source, wake_on(&tx), Arc::new(MemorySink::new())).unwrap();
    let hub = Arc::new(hub);
    let bridge = hub.bridge(1);
    tx.send(option_update(Some(1), 1)).unwrap();
    tx.send(option_update(Some(1), 2)).unwrap();
    // The first envelope is lost with its panic; the second one arrives.
    let owned = bridge.next_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(owned.client_id, Some(1));
    assert!(!bridge.stopped_unexpectedly());
    assert!(!hub.stopped_unexpectedly());
}

#[test]
fn a_stopped_shared_hub_is_replaced_but_a_running_one_is_kept() {
    assert!(matches!(reusable(None), Reuse::None));
    let (live, _tx, _sink) = channel_hub();
    assert!(matches!(reusable(Some(&live)), Reuse::Running(_)));

    let (dying_tx, dying_rx) = mpsc::channel::<()>();
    let source: ReceiveSource = Box::new(move |_| {
        let _ = dying_rx.recv();
        panic!("receive source failed");
    });
    let dead =
        Arc::new(TdHub::with_source(source, no_wake(), Arc::new(MemorySink::new())).unwrap());
    // The loop only reads while a client is registered.
    let _bridge = dead.bridge(1);
    dying_tx.send(()).unwrap();
    drop(dying_tx);
    assert!(wait_until(|| dead.stopped_unexpectedly()));
    assert!(matches!(reusable(Some(&dead)), Reuse::Stopped(_)));
}

#[test]
fn a_requested_shutdown_is_not_a_failure() {
    let (hub, _tx, _sink) = channel_hub();
    let bridge = hub.bridge(1);
    hub.shutdown();
    assert!(!bridge.stopped_unexpectedly());
}

/// Three real TDLib clients on the shared hub each get only their own
/// envelopes. No network and no account: `getAuthorizationState` before
/// `setTdlibParameters`. Run with
/// `QUILL_TDJSON_PATH=<libtdjson> cargo test --no-default-features live_clients -- --ignored`.
#[test]
#[ignore = "needs libtdjson (QUILL_TDJSON_PATH)"]
fn live_clients_get_only_their_own_envelopes() {
    let Ok(hub) = TdHub::shared(Arc::new(MemorySink::new())) else {
        eprintln!("libtdjson not found; skipped");
        return;
    };
    let api = hub.api().expect("live hub").clone();
    let ids: Vec<i32> = (0..3).map(|_| api.create_client_id()).collect();
    let bridges: Vec<ReceiveBridge> = ids.iter().map(|&id| hub.bridge(id)).collect();
    for (n, &id) in ids.iter().enumerate() {
        let request = format!(
            r#"{{"@type":"getAuthorizationState","@extra":"{}"}}"#,
            100 + n
        );
        api.send(id, &request).unwrap();
    }
    for (n, (bridge, &id)) in bridges.iter().zip(&ids).enumerate() {
        let wanted = crate::ids::RequestId(100 + n as u64);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut answered = false;
        while !answered && Instant::now() < deadline {
            let Some(owned) = bridge.next_timeout(Duration::from_millis(200)) else {
                continue;
            };
            assert_eq!(owned.client_id, Some(id), "another client's envelope");
            answered = owned.envelope.extra == Some(wanted);
        }
        assert!(answered, "client {id} got its answer");
    }
    // Close every client before the process exits: TDLib's static
    // destructors racing live clients crash at exit now and then (3 in 60
    // runs left open, 0 in 60 closed), which is why `LiveConnect` always
    // closes its client.
    for (bridge, &id) in bridges.iter().zip(&ids) {
        api.send(id, r#"{"@type":"close"}"#).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut closed = false;
        while !closed && Instant::now() < deadline {
            if let Some(owned) = bridge.next_timeout(Duration::from_millis(200)) {
                closed = matches!(
                    owned.envelope.payload,
                    EnvelopePayload::Auth(AuthPayload::UpdateAuthorizationState(
                        AuthorizationState::Closed
                    ))
                );
            }
        }
        assert!(closed, "client {id} closed");
    }
}

#[test]
fn a_stop_ends_the_receive_thread_with_or_without_clients() {
    let (idle, _tx, _sink) = channel_hub();
    assert!(idle.stop_within(Duration::from_secs(30)));

    let (busy, _tx, _sink) = channel_hub();
    let bridge = busy.bridge(1);
    assert!(busy.stop_within(Duration::from_secs(30)));
    assert!(!bridge.stopped_unexpectedly());
}

#[test]
fn a_stop_wakes_a_receive_that_is_blocked() {
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let source: ReceiveSource = Box::new(move |_| {
        let _ = entered_tx.send(());
        let _ = wake_rx.recv();
        None
    });
    let wake: ReceiveWake = Box::new(move || {
        let _ = wake_tx.send(());
    });
    let hub = TdHub::with_source(source, wake, Arc::new(MemorySink::new())).unwrap();
    let hub = Arc::new(hub);
    let _client = hub.bridge(1);
    entered_rx.recv().unwrap();
    assert!(hub.stop_within(Duration::from_secs(30)));
}

#[test]
fn what_ends_the_receive_of_a_stopped_hub_is_not_delivered() {
    let (tx, rx) = mpsc::channel::<String>();
    let (entered_tx, entered) = mpsc::channel();
    let source: ReceiveSource = Box::new(move |_| {
        let _ = entered_tx.send(());
        rx.recv().ok()
    });
    let wake: ReceiveWake = Box::new(move || {
        let _ = tx.send(
            r#"{"@type":"error","code":400,"message":"Invalid TDLib instance specified"}"#.into(),
        );
    });
    let sink = Arc::new(MemorySink::new());
    let hub = Arc::new(TdHub::with_source(source, wake, sink.clone()).unwrap());
    let _client = hub.bridge(1);
    entered.recv().unwrap();
    assert!(hub.stop_within(Duration::from_secs(30)));
    assert!(sink.snapshot().is_empty());
}

#[test]
fn a_stop_gives_up_on_a_receive_that_does_not_return() {
    let (release, released) = mpsc::channel::<()>();
    let (entered_tx, entered) = mpsc::channel();
    let source: ReceiveSource = Box::new(move |_| {
        let _ = entered_tx.send(());
        let _ = released.recv();
        None
    });
    let hub = Arc::new(TdHub::with_source(source, no_wake(), Arc::new(MemorySink::new())).unwrap());
    let _client = hub.bridge(1);
    entered.recv().unwrap();
    assert!(!hub.stop_within(Duration::ZERO));
    drop(release);
}

#[test]
#[ignore = "QUILL_TDJSON_PATH=<libtdjson> cargo test --lib blocked_receive -- --ignored"]
fn a_send_to_an_unmade_client_ends_a_blocked_receive() {
    let api = Arc::new(TdJson::load_default().expect("libtdjson"));
    let receive = api.clone();
    let (entering_tx, entering_rx) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let _ = entering_tx.send(());
        receive.receive(60.0)
    });
    entering_rx.recv().unwrap();
    api.wake_receive();
    let answer = thread.join().unwrap().expect("an answer, not the timeout");
    assert!(answer.contains("Invalid TDLib instance"), "{answer}");
}

const EXIT_CHILD: &str = "QUILL_TEST_HUB_EXIT_CHILD";

#[expect(
    clippy::exit,
    reason = "the re-executed child test process is the exit under test"
)]
fn exit_child_main() -> ! {
    let Ok(live) = crate::telegram::client::LiveTdJson::connect(Arc::new(MemorySink::new())) else {
        std::process::exit(3);
    };
    let bridge = live.bridge();
    let _ = live.send(r#"{"@type":"close"}"#);
    while let Some(owned) = bridge.next_timeout(Duration::from_secs(10)) {
        if matches!(
            owned.envelope.payload,
            EnvelopePayload::Auth(AuthPayload::UpdateAuthorizationState(
                AuthorizationState::Closed
            ))
        ) {
            std::process::exit(0);
        }
    }
    std::process::exit(4);
}

#[test]
#[ignore = "QUILL_TDJSON_PATH=<libtdjson> QUILL_TEST_HUB_EXIT_RUNS=700 cargo test --lib exit_with -- --ignored --nocapture"]
fn exit_with_the_receive_thread_blocked_stays_clean() {
    if std::env::var_os(EXIT_CHILD).is_some() {
        exit_child_main();
    }
    let runs: u32 = std::env::var("QUILL_TEST_HUB_EXIT_RUNS")
        .ok()
        .and_then(|runs| runs.parse().ok())
        .unwrap_or(100);
    let exe = std::env::current_exe().unwrap();
    let mut unclean = 0;
    let mut slowest = Duration::ZERO;
    for _ in 0..runs {
        let started = Instant::now();
        let status = std::process::Command::new(&exe)
            .args([
                "--ignored",
                "--exact",
                "telegram::hub::tests::exit_with_the_receive_thread_blocked_stays_clean",
            ])
            .env(EXIT_CHILD, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        slowest = slowest.max(started.elapsed());
        if status.code() != Some(0) {
            eprintln!("{status}");
            unclean += 1;
        }
    }
    eprintln!("{runs} exits, {unclean} unclean, slowest {slowest:?}");
    assert_eq!(unclean, 0);
}

#[test]
fn each_route_wakes_only_its_own_poll_loop_and_a_switch_swaps_them() {
    static ON_SCREEN: crate::wake::Signal = crate::wake::Signal::new();
    static BACKGROUND: crate::wake::Signal = crate::wake::Signal::new();
    let routes = Routes::default();
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let seq = AtomicU64::new(0);
    let parse = |json: String| copy_and_parse(&json, &seq, &sink).unwrap();
    let _first = routes.register(1);
    let _second = routes.register(2);
    routes.wake_with(1, &ON_SCREEN);
    routes.wake_with(2, &BACKGROUND);
    let epochs = || (ON_SCREEN.epoch(), BACKGROUND.epoch());
    let before = epochs();
    routes.deliver(parse(option_update(Some(2), 1)));
    assert_eq!(epochs(), (before.0, before.1 + 1));
    routes.deliver(parse(option_update(Some(1), 1)));
    assert_eq!(epochs(), (before.0 + 1, before.1 + 1));
    // Account switch: client 2 comes on screen, client 1 goes behind.
    routes.wake_with(2, &ON_SCREEN);
    routes.wake_with(1, &BACKGROUND);
    routes.deliver(parse(option_update(Some(2), 2)));
    routes.deliver(parse(option_update(Some(2), 3)));
    assert_eq!(epochs(), (before.0 + 3, before.1 + 1));
    routes.deliver(parse(option_update(Some(1), 2)));
    assert_eq!(epochs(), (before.0 + 3, before.1 + 2));
}
