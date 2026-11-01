//! Quill's live host against the real `quill-webview --carrier` and a
//! local echo page. Needs a desktop session and a debug helper build:
//!
//! `cargo build -p quill-webview && QUILL_CARRIER_HELPER=target/debug/quill-webview
//!  QUILL_WEBVIEW_CARRIER_LOOPBACK=1 cargo test --no-default-features --lib
//!  helper_live -- --ignored`
use super::helper::{Capability, HelperCarrierHost, HelperConfig};
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

const ECHO_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => b.postMessage(new Uint8Array(e.data).slice().reverse());
b.postMessage('{"t":"hello"}');
</script></head><body></body></html>"#;

fn serve() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
                    break;
                }
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{ECHO_PAGE}",
                ECHO_PAGE.len()
            );
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        }
    });
    port
}

/// A relay that hands each request to the test and pokes
/// [`crate::wake::UPDATES`], the signal a carrier failure pokes too.
fn relay() -> (RelaySink, Receiver<serde_json::Value>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let sink: RelaySink = Arc::new(move |request: &str| {
        let value = serde_json::from_str(request).unwrap();
        let sent = tx.lock().unwrap().send(value).is_ok();
        crate::wake::UPDATES.poke();
        sent
    });
    (sink, rx)
}

/// The next relayed message as `(carrier_id, message)`. A helper that
/// dies or goes quiet instead fails the test through `poll`: its watchdog
/// reports it and pokes the same signal, so no wait here needs a bound.
fn next(
    host: &mut HelperCarrierHost,
    relayed: &Receiver<serde_json::Value>,
) -> (i64, serde_json::Value) {
    let request = loop {
        let seen = crate::wake::UPDATES.epoch();
        match relayed.try_recv() {
            Ok(request) => break request,
            Err(TryRecvError::Disconnected) => panic!("the relay was dropped"),
            Err(TryRecvError::Empty) => {}
        }
        let failures = host.poll();
        assert!(failures.is_empty(), "carrier failed: {failures:?}");
        futures_lite::future::block_on(crate::wake::UPDATES.wait_since(seen));
    };
    assert_eq!(request["@type"], "processWebProxyCarrierMessage");
    assert_eq!(request["@extra"], CARRIER_RELAY_EXTRA.as_extra());
    (
        request["carrier_id"].as_i64().unwrap(),
        request["message"].clone(),
    )
}

#[test]
#[ignore = "needs a desktop session and QUILL_CARRIER_HELPER"]
fn helper_live_relay_round_trip() {
    let program = std::env::var_os("QUILL_CARRIER_HELPER").expect("QUILL_CARRIER_HELPER");
    let port = serve();
    let origin = format!("http://127.0.0.1:{port}");
    let data_dir = crate::scratch::process_dir("helper-live").join("carrier");
    let (sink, relayed) = relay();
    let mut host = HelperCarrierHost::new(HelperConfig {
        program: Some(program.into()),
        data_dir: Some(data_dir.clone()),
        probe_cache: None,
        relay: sink,
        capability: Capability::default(),
    });
    let request = CarrierRequest {
        id: 21,
        proxy_id: 1,
        url: CarrierUrl::new(format!("{origin}/app/?bridge=cap#android=n")),
        origin,
        path_prefix: "/app/".into(),
        bridge_object_name: "TelegramWebProxy".into(),
        temporary: false,
    };
    host.open(&request).unwrap();
    // Sent before the page listens: held until it does.
    host.deliver(21, "AQID".into());
    let (carrier, hello) = next(&mut host, &relayed);
    assert_eq!(carrier, 21);
    assert_eq!(hello["@type"], "webProxyCarrierMessageText");
    assert_eq!(hello["text"], r#"{"t":"hello"}"#);
    let (_, early) = next(&mut host, &relayed);
    assert_eq!(early["data"], "AwIB");
    // Relay latency through the pipe and the page, both ways.
    let mut rounds = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        host.deliver(21, "AQID".into());
        let (carrier, message) = next(&mut host, &relayed);
        assert_eq!(carrier, 21);
        assert_eq!(message["@type"], "webProxyCarrierMessageBinary");
        assert_eq!(message["data"], "AwIB");
        rounds.push(started.elapsed());
    }
    rounds.sort();
    eprintln!(
        "round trip median {:?}, max {:?}",
        rounds[rounds.len() / 2],
        rounds[rounds.len() - 1]
    );
    assert!(host.poll().is_empty());
    host.close(21, true);
    host.shutdown();
    assert!(host.poll().is_empty());
    let left: Vec<_> = std::fs::read_dir(&data_dir)
        .map(|entries| entries.flatten().map(|entry| entry.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
#[ignore = "needs a desktop session and QUILL_CARRIER_HELPER"]
fn helper_live_capability_check_passes() {
    let program = std::env::var_os("QUILL_CARRIER_HELPER").expect("QUILL_CARRIER_HELPER");
    let data_dir = crate::scratch::process_dir("helper-live").join("probe");
    let started = Instant::now();
    assert_eq!(
        super::helper::probe(std::path::Path::new(&program), &data_dir),
        Some(true)
    );
    eprintln!("capability check took {:?}", started.elapsed());
}
