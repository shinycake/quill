use super::*;
use crate::diagnostics::MemorySink;
use crate::telegram::envelope::EnvelopePayload;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

/// A hub reading from a channel, so tests decide what `td_receive` returns.
fn channel_hub() -> (Arc<TdHub>, Sender<String>, Arc<MemorySink>) {
    let (tx, rx) = mpsc::channel::<String>();
    let source: ReceiveSource =
        Box::new(move |timeout| rx.recv_timeout(Duration::from_secs_f64(timeout)).ok());
    let sink = Arc::new(MemorySink::new());
    let hub = TdHub::with_source(source, sink.clone()).expect("hub thread");
    (Arc::new(hub), tx, sink)
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
        thread::sleep(Duration::from_millis(5));
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
        thread::sleep(Duration::from_millis(5));
        None
    });
    let hub = Arc::new(TdHub::with_source(source, Arc::new(MemorySink::new())).unwrap());
    thread::sleep(Duration::from_millis(150));
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
    let hub = Arc::new(TdHub::with_source(source, Arc::new(MemorySink::new())).unwrap());
    let first = hub.bridge(1);
    let second = hub.bridge(2);
    assert!(!first.stopped_unexpectedly());
    tx.send(()).unwrap();
    assert!(wait_until(|| first.stopped_unexpectedly()));
    assert!(second.stopped_unexpectedly());
    assert!(hub.stopped_unexpectedly());
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
                    EnvelopePayload::Auth(
                        crate::telegram::envelope::AuthPayload::UpdateAuthorizationState(
                            crate::telegram::envelope::AuthorizationState::Closed
                        )
                    )
                );
            }
        }
        assert!(closed, "client {id} closed");
    }
}
