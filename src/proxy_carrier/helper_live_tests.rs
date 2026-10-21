//! Quill's live host against the real `quill-webview --carrier` and a
//! local echo page. Needs a desktop session and a debug helper build:
//!
//! `cargo build -p quill-webview && QUILL_CARRIER_HELPER=target/debug/quill-webview
//!  QUILL_WEBVIEW_CARRIER_LOOPBACK=1 cargo test --no-default-features --lib
//!  helper_live -- --ignored`
use super::helper::HelperCarrierHost;
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

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

fn next(host: &mut HelperCarrierHost, within: Duration) -> (i32, HostEvent) {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if let Some(event) = host.poll().into_iter().next() {
            return event;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    panic!("no host event within {within:?}");
}

#[test]
#[ignore = "needs a desktop session and QUILL_CARRIER_HELPER"]
fn helper_live_relay_round_trip() {
    let program = std::env::var_os("QUILL_CARRIER_HELPER").expect("QUILL_CARRIER_HELPER");
    let port = serve();
    let origin = format!("http://127.0.0.1:{port}");
    let mut host = HelperCarrierHost::new(Some(program.into()));
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
    assert_eq!(
        next(&mut host, Duration::from_secs(20)),
        (
            21,
            HostEvent::Message(CarrierMessage::Text(r#"{"t":"hello"}"#.into()))
        )
    );
    // Relay latency through the pipe and the page, both ways.
    let mut rounds = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        host.deliver(21, "AQID".into());
        assert_eq!(
            next(&mut host, Duration::from_secs(5)),
            (
                21,
                HostEvent::Message(CarrierMessage::Binary("AwIB".into()))
            )
        );
        rounds.push(started.elapsed());
    }
    rounds.sort();
    eprintln!(
        "round trip median {:?}, max {:?}",
        rounds[rounds.len() / 2],
        rounds[rounds.len() - 1]
    );
    host.close(21, true);
    std::thread::sleep(Duration::from_millis(1500));
    assert!(host.poll().is_empty());
}
