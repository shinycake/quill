//! The carrier mode against a real system web view and a local relay page.
//! Needs a desktop session (a window server on macOS, a display on Linux),
//! so the tests are ignored by default:
//!
//! `cargo test -p quill-webview --test carrier_live -- --ignored --test-threads=1`
//!
//! Debug builds of the helper accept `http://127.0.0.1:<port>` when
//! `QUILL_WEBVIEW_CARRIER_LOOPBACK` is set; release builds never do.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness"
)]

use quill_webview_protocol::carrier::{CarrierCommand, CarrierEvent, CarrierFailure};
use quill_webview_protocol::{decode, encode};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RELAY_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => {
  if (typeof e.data === 'string') { b.postMessage('got-text:' + e.data); return; }
  b.postMessage(new Uint8Array(e.data).slice().reverse());
};
let threw = false;
try { b.postMessage({}); } catch (error) { threw = error instanceof TypeError; }
let frozen = false;
try { window.TelegramWebProxy = null; frozen = window.TelegramWebProxy === b; } catch (error) { frozen = true; }
b.postMessage(JSON.stringify({ t: 'init', rejects: threw, frozen, keys: Object.keys(b).sort().join(',') }));
fetch('/other/secret').then(() => b.postMessage('outside:allowed'), () => b.postMessage('outside:blocked'));
fetch('/relay/ok').then((r) => b.postMessage('inside:' + r.status), () => b.postMessage('inside:blocked'));
</script></head><body>relay</body></html>"#;

const NAVIGATING_PAGE: &str = r#"<!doctype html><html><head><script>
window.TelegramWebProxy.postMessage('leaving');
setTimeout(() => { window.location.href = '/other/'; }, 100);
</script></head><body></body></html>"#;

const HANGING_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = () => { const end = Date.now() + 60000; while (Date.now() < end) {} };
b.postMessage('ready');
</script></head><body></body></html>"#;

/// A one-thread HTTP server that records every path asked for.
fn serve() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut first = String::new();
            if reader.read_line(&mut first).is_err() {
                continue;
            }
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                    break;
                }
            }
            let path = first.split(' ').nth(1).unwrap_or("/").to_string();
            log.lock().unwrap().push(path.clone());
            let bare = path.split(['?', '#']).next().unwrap_or("/");
            let body = match bare {
                "/relay/" => RELAY_PAGE,
                "/nav/" => NAVIGATING_PAGE,
                "/hang/" => HANGING_PAGE,
                "/relay/ok" => "ok",
                _ => "",
            };
            let status = if body.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.flush();
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        }
    });
    (port, seen)
}

struct Helper {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<CarrierEvent>,
}

impl Helper {
    fn open(port: u16, prefix: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_quill-webview"))
            .arg("--carrier")
            .env("QUILL_WEBVIEW_CARRIER_LOOPBACK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(event) = decode::<CarrierEvent>(&line) {
                    let _ = tx.send(event);
                }
            }
        });
        let origin = format!("http://127.0.0.1:{port}");
        let open = CarrierCommand::Open {
            url: format!("{origin}{prefix}?bridge=cap#android=nonce"),
            origin,
            path_prefix: prefix.into(),
            bridge_object_name: "TelegramWebProxy".into(),
        };
        stdin.write_all(encode(&open).as_bytes()).unwrap();
        Self {
            child,
            stdin,
            events,
        }
    }

    fn send(&mut self, command: &CarrierCommand) {
        self.stdin.write_all(encode(command).as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// The next event that is not `Alive`.
    fn next(&self, within: Duration) -> CarrierEvent {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(CarrierEvent::Alive) => continue,
                Ok(event) => return event,
                Err(_) => panic!("no event within {within:?}"),
            }
        }
    }

    fn text(&self, within: Duration) -> String {
        match self.next(within) {
            CarrierEvent::Text { text } => text,
            other => panic!("expected text, got {other:?}"),
        }
    }

    fn exits_within(&mut self, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        while Instant::now() < deadline {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn relays_both_ways_inside_its_scope_and_closes() {
    let (port, seen) = serve();
    let mut helper = Helper::open(port, "/relay/");
    assert_eq!(helper.next(Duration::from_secs(10)), CarrierEvent::Ready);
    let init = helper.text(Duration::from_secs(15));
    assert_eq!(
        init,
        r#"{"t":"init","rejects":true,"frozen":true,"keys":"onmessage,postMessage"}"#
    );
    let mut probes = [
        helper.text(Duration::from_secs(5)),
        helper.text(Duration::from_secs(5)),
    ];
    probes.sort();
    assert_eq!(
        probes,
        ["inside:200".to_string(), "outside:blocked".to_string()]
    );
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.starts_with("/other/")),
        "the policy let a request out of the prefix: {:?}",
        seen.lock().unwrap()
    );

    // Bytes in, reversed bytes out: [1, 2, 3] and back as [3, 2, 1].
    helper.send(&CarrierCommand::Deliver {
        data: "AQID".into(),
    });
    assert_eq!(
        helper.next(Duration::from_secs(5)),
        CarrierEvent::Binary {
            data: "AwIB".into()
        }
    );
    // Many in a row stay in order.
    for _ in 0..50 {
        helper.send(&CarrierCommand::Deliver {
            data: "AAEC".into(),
        });
    }
    for _ in 0..50 {
        assert_eq!(
            helper.next(Duration::from_secs(5)),
            CarrierEvent::Binary {
                data: "AgEA".into()
            }
        );
    }
    if let Ok(wait) = std::env::var("CARRIER_LINGER_SECS") {
        eprintln!("helper pid {}", helper.child.id());
        std::thread::sleep(Duration::from_secs(wait.parse().unwrap_or(10)));
    }

    helper.send(&CarrierCommand::Close { notify_page: true });
    assert!(helper.exits_within(Duration::from_secs(2)));
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn leaving_the_scope_fails_the_carrier() {
    let (port, seen) = serve();
    let helper = Helper::open(port, "/nav/");
    assert_eq!(helper.next(Duration::from_secs(10)), CarrierEvent::Ready);
    assert_eq!(helper.text(Duration::from_secs(15)), "leaving");
    assert_eq!(
        helper.next(Duration::from_secs(5)),
        CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed {
                message: "the page tried to leave its origin".into()
            }
        }
    );
    assert!(!seen.lock().unwrap().iter().any(|path| path == "/other/"));
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn a_hung_page_is_reported_terminated() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/hang/");
    assert_eq!(helper.next(Duration::from_secs(10)), CarrierEvent::Ready);
    assert_eq!(helper.text(Duration::from_secs(15)), "ready");
    helper.send(&CarrierCommand::Deliver {
        data: "AQID".into(),
    });
    assert_eq!(
        helper.next(Duration::from_secs(25)),
        CarrierEvent::Failed {
            failure: CarrierFailure::Terminated
        }
    );
    assert!(helper.exits_within(Duration::from_secs(3)));
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn an_address_outside_the_rules_is_refused_before_loading() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quill-webview"))
        .arg("--carrier")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // No loopback switch: plain http is refused.
    let open = CarrierCommand::Open {
        url: "http://127.0.0.1:9/relay/".into(),
        origin: "http://127.0.0.1:9".into(),
        path_prefix: "/relay/".into(),
        bridge_object_name: "TelegramWebProxy".into(),
    };
    stdin.write_all(encode(&open).as_bytes()).unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert!(matches!(
        decode::<CarrierEvent>(&line).unwrap(),
        CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed { .. }
        }
    ));
    let _ = child.wait();
}
