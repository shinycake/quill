//! The carrier mode against a real system web view and a local relay page.
//! Needs a desktop session (a window server on macOS, a display on Linux),
//! so the tests are ignored by default:
//!
//! `cargo test -p quill-webview --test carrier_live -- --ignored --test-threads=1`
//!
//! The idle soak (`an_idle_carrier_stays_alive`) waits `CARRIER_SOAK_SECS`
//! (default 130) with nothing to relay.
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

use quill_webview_protocol::carrier::{CarrierCommand, CarrierEvent, CarrierFailure, SecretUrl};
use quill_webview_protocol::{decode, encode};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const RELAY_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => {
  if (typeof e.data === 'string') { fetch('/relay/got?' + encodeURIComponent(e.data)); return; }
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

const ECHO_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => b.postMessage(new Uint8Array(e.data).slice().reverse());
b.postMessage('ready');
</script></head><body></body></html>"#;

/// Listens only after a while: deliveries sent before then must wait.
const LATE_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.postMessage('loaded');
setTimeout(() => { b.onmessage = (e) => b.postMessage(new Uint8Array(e.data).slice().reverse()); }, 1500);
</script></head><body></body></html>"#;

/// Everything that would raise a native dialog, in the page and in a
/// blank frame of its own (a fresh realm).
const DIALOG_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => b.postMessage(new Uint8Array(e.data).slice().reverse());
const run = (w) => [String(w.alert('a')), String(w.confirm('c')), String(w.prompt('p', 'x')), String(w.print())].join(',');
b.postMessage('top:' + run(window));
const frame = document.createElement('iframe');
document.documentElement.appendChild(frame);
const inner = frame.contentWindow;
b.postMessage('frame:' + run(inner) + ':' + (typeof inner.TelegramWebProxy) + ':' + (typeof inner.RTCPeerConnection));
b.postMessage('rtc:' + (typeof window.RTCPeerConnection) + ',' + (typeof window.WebAssembly));
b.postMessage('open:' + String(window.open('https://evil.example/')));
</script></head><body></body></html>"#;

/// Telegram Desktop's restrictions probe (`web_proxy_webview.cpp`,
/// `RestrictionsProbeScript`): no WebRTC, WebTransport or WebAssembly in
/// the page or in a fresh frame. Then the same for a frame reached by
/// index (no element getter in the way), a worker and service workers.
const RESTRICTIONS_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
b.onmessage = (e) => b.postMessage(new Uint8Array(e.data).slice().reverse());
const probe = (w) => typeof w.RTCPeerConnection === 'undefined' && typeof w.WebTransport === 'undefined' && typeof w.WebAssembly === 'undefined';
let ok = probe(window);
const frame = document.createElement('iframe');
(document.documentElement || document).appendChild(frame);
try { ok = ok && !!frame.contentWindow && probe(frame.contentWindow); } finally { frame.remove(); }
b.postMessage('tdesktop:' + (ok ? '1' : '0'));
const second = document.createElement('iframe');
document.documentElement.appendChild(second);
b.postMessage('indexed:' + (probe(window[0]) ? '1' : '0'));
second.remove();
b.postMessage('sw:' + (typeof navigator.serviceWorker));
try {
  const worker = new Worker('/restrict/worker.js');
  worker.onmessage = (e) => b.postMessage('worker:' + e.data);
  worker.onerror = () => b.postMessage('worker:error');
} catch (error) { b.postMessage('worker:threw'); }
</script></head><body></body></html>"#;

const RESTRICTIONS_WORKER: &str = "postMessage([typeof WebAssembly, typeof WebTransport, typeof RTCPeerConnection, typeof Worker].join(','));";

const NAVIGATING_PAGE: &str = r#"<!doctype html><html><head><script>
window.TelegramWebProxy.postMessage('leaving');
setTimeout(() => { window.location.href = '/other/'; }, 100);
</script></head><body></body></html>"#;

const BLANK_PAGE: &str = r#"<!doctype html><html><head><script>
window.TelegramWebProxy.postMessage('leaving');
setTimeout(() => { window.location.href = 'about:blank'; }, 100);
</script></head><body></body></html>"#;

const REDIRECT_PAGE: &str = r#"<!doctype html><html><head><script>
const b = window.TelegramWebProxy;
fetch('/redirect/out').then(() => b.postMessage('redirect:allowed'), () => b.postMessage('redirect:blocked'));
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
            let (status, extra, body) = match bare {
                "/relay/" => ("200 OK", "", RELAY_PAGE),
                "/echo/" => ("200 OK", "", ECHO_PAGE),
                "/late/" => ("200 OK", "", LATE_PAGE),
                "/dialogs/" => ("200 OK", "", DIALOG_PAGE),
                "/restrict/" => ("200 OK", "", RESTRICTIONS_PAGE),
                "/restrict/worker.js" => ("200 OK", "", RESTRICTIONS_WORKER),
                "/nav/" => ("200 OK", "", NAVIGATING_PAGE),
                "/blank/" => ("200 OK", "", BLANK_PAGE),
                "/redirect/" => ("200 OK", "", REDIRECT_PAGE),
                "/redirect/out" => ("302 Found", "Location: /other/redirected\r\n", ""),
                "/hang/" => ("200 OK", "", HANGING_PAGE),
                "/relay/ok" => ("200 OK", "", "ok"),
                _ => (
                    "404 Not Found",
                    "",
                    "<!doctype html><title>404</title>not here",
                ),
            };
            let kind = if bare.ends_with(".js") {
                "text/javascript"
            } else {
                "text/html"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.flush();
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        }
    });
    (port, seen)
}

/// A folder for the helper's data, fresh per test.
fn data_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("quill-carrier-live-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn spawn() -> (Child, ChildStdin, Receiver<CarrierEvent>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_quill-webview"))
        .arg("--carrier")
        .env("QUILL_WEBVIEW_CARRIER_LOOPBACK", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
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
    (child, stdin, events)
}

struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    events: Receiver<CarrierEvent>,
    alive: usize,
}

impl Helper {
    fn open(port: u16, prefix: &str) -> Self {
        Self::open_at(format!("http://127.0.0.1:{port}"), prefix)
    }

    fn open_at(origin: String, prefix: &str) -> Self {
        let (child, mut stdin, events) = spawn();
        let open = CarrierCommand::Open {
            url: SecretUrl::new(format!("{origin}{prefix}?bridge=cap#android=nonce")),
            origin,
            path_prefix: prefix.into(),
            bridge_object_name: "TelegramWebProxy".into(),
            data_dir: Some(data_dir(&prefix.replace('/', "")).display().to_string()),
        };
        stdin.write_all(encode(&open).as_bytes()).unwrap();
        Self {
            child,
            stdin: Some(stdin),
            events,
            alive: 0,
        }
    }

    fn send(&mut self, command: &CarrierCommand) {
        let stdin = self.stdin.as_mut().unwrap();
        stdin.write_all(encode(command).as_bytes()).unwrap();
        stdin.flush().unwrap();
    }

    fn deliver(&mut self, data: &str) {
        self.send(&CarrierCommand::Deliver { data: data.into() });
    }

    /// The next event that is not `Alive`.
    fn next(&mut self, within: Duration) -> CarrierEvent {
        let deadline = Instant::now() + within;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.events.recv_timeout(left) {
                Ok(CarrierEvent::Alive) => self.alive += 1,
                Ok(event) => return event,
                Err(_) => panic!("no event within {within:?}"),
            }
        }
    }

    fn text(&mut self, within: Duration) -> String {
        match self.next(within) {
            CarrierEvent::Text { text } => text,
            other => panic!("expected text, got {other:?}"),
        }
    }

    fn ready(&mut self) {
        assert_eq!(self.next(Duration::from_secs(20)), CarrierEvent::Ready);
    }

    fn echoes(&mut self) {
        self.deliver("AQID");
        assert_eq!(
            self.next(Duration::from_secs(10)),
            CarrierEvent::Binary {
                data: "AwIB".into()
            }
        );
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
    helper.ready();
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
        "a request left the prefix: {:?}",
        seen.lock().unwrap()
    );

    // Bytes in, reversed bytes out: [1, 2, 3] and back as [3, 2, 1].
    helper.echoes();
    // Many in a row stay in order.
    for _ in 0..50 {
        helper.deliver("AAEC");
    }
    for _ in 0..50 {
        assert_eq!(
            helper.next(Duration::from_secs(5)),
            CarrierEvent::Binary {
                data: "AgEA".into()
            }
        );
    }

    // The page hears the close before the view goes (and nothing it says
    // after that is passed on, so the server is the witness).
    helper.send(&CarrierCommand::Close { notify_page: true });
    assert!(helper.exits_within(Duration::from_secs(2)));
    let closed = "/relay/got?%7B%22t%22%3A%22close%22%7D".to_string();
    assert!(
        seen.lock().unwrap().contains(&closed),
        "{:?}",
        seen.lock().unwrap()
    );
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn deliveries_before_the_page_listens_wait_for_it() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/late/");
    helper.ready();
    // Sent while the page is still loading, and again before it listens.
    helper.deliver("AQID");
    assert_eq!(helper.text(Duration::from_secs(15)), "loaded");
    helper.deliver("BAUG");
    assert_eq!(
        helper.next(Duration::from_secs(10)),
        CarrierEvent::Binary {
            data: "AwIB".into()
        }
    );
    assert_eq!(
        helper.next(Duration::from_secs(5)),
        CarrierEvent::Binary {
            data: "BgUE".into()
        }
    );
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn dialogs_printing_and_webrtc_never_reach_the_page() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/dialogs/");
    helper.ready();
    // Every call returns at once, as if dismissed, in the page and in a
    // frame of its own; a native dialog would block until the timeout.
    assert_eq!(
        helper.text(Duration::from_secs(15)),
        "top:undefined,false,null,undefined"
    );
    let frame = helper.text(Duration::from_secs(5));
    // The frame's own RTCPeerConnection depends on whether the engine runs
    // the bridge script in blank frames; logged for the decision doc.
    eprintln!("{frame}");
    assert!(
        frame.starts_with("frame:undefined,false,null,undefined:undefined:"),
        "{frame}"
    );
    assert_eq!(
        helper.text(Duration::from_secs(5)),
        "rtc:undefined,undefined"
    );
    assert_eq!(helper.text(Duration::from_secs(5)), "open:null");
    helper.echoes();
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn no_webrtc_webtransport_or_webassembly_in_frames_or_workers() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/restrict/");
    helper.ready();
    assert_eq!(helper.text(Duration::from_secs(15)), "tdesktop:1");
    assert_eq!(helper.text(Duration::from_secs(5)), "indexed:1");
    assert_eq!(helper.text(Duration::from_secs(5)), "sw:undefined");
    assert_eq!(
        helper.text(Duration::from_secs(10)),
        "worker:undefined,undefined,undefined,undefined"
    );
    helper.echoes();
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn leaving_the_scope_fails_the_carrier() {
    let (port, seen) = serve();
    for prefix in ["/nav/", "/blank/"] {
        let mut helper = Helper::open(port, prefix);
        helper.ready();
        assert_eq!(helper.text(Duration::from_secs(15)), "leaving");
        assert_eq!(
            helper.next(Duration::from_secs(5)),
            CarrierEvent::Failed {
                failure: CarrierFailure::LoadFailed {
                    message: "the page tried to leave its origin".into()
                }
            },
            "{prefix}"
        );
    }
    assert!(!seen.lock().unwrap().iter().any(|path| path == "/other/"));
}

/// WebKit's content rules hold after a redirect; WebView2's request
/// handler does not see the redirected request (`docs/decisions/web-proxies.md`).
#[test]
#[cfg(not(windows))]
#[ignore = "needs a desktop session and a system web view"]
fn a_redirect_out_of_the_prefix_is_blocked() {
    let (port, seen) = serve();
    let mut helper = Helper::open(port, "/redirect/");
    helper.ready();
    assert_eq!(helper.text(Duration::from_secs(15)), "redirect:blocked");
    assert!(
        !seen
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.starts_with("/other/")),
        "{:?}",
        seen.lock().unwrap()
    );
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn an_http_error_page_fails_to_load() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/missing/");
    helper.ready();
    let started = Instant::now();
    match helper.next(Duration::from_secs(25)) {
        // The status where the engine reports it, else the start timeout.
        CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed { message },
        } => eprintln!("after {:?}: {message}", started.elapsed()),
        other => panic!("{other:?}"),
    }
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn a_page_that_cannot_be_reached_fails_to_load() {
    // A port nobody listens on.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut helper = Helper::open(port, "/relay/");
    helper.ready();
    assert!(matches!(
        helper.next(Duration::from_secs(40)),
        CarrierEvent::Failed {
            failure: CarrierFailure::LoadFailed { .. }
        }
    ));
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn a_hung_page_is_reported_terminated() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/hang/");
    helper.ready();
    assert_eq!(helper.text(Duration::from_secs(15)), "ready");
    helper.deliver("AQID");
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
fn the_helper_ends_when_quill_hangs_up() {
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/echo/");
    helper.ready();
    assert_eq!(helper.text(Duration::from_secs(15)), "ready");
    drop(helper.stdin.take());
    assert!(helper.exits_within(Duration::from_secs(2)));
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn an_idle_carrier_stays_alive() {
    let soak = std::env::var("CARRIER_SOAK_SECS")
        .ok()
        .and_then(|secs| secs.parse().ok())
        .unwrap_or(130);
    let (port, _) = serve();
    let mut helper = Helper::open(port, "/echo/");
    helper.ready();
    assert_eq!(helper.text(Duration::from_secs(15)), "ready");
    helper.echoes();
    let started = Instant::now();
    let before = helper.alive;
    // Nothing but signs of life for the whole stretch.
    while started.elapsed() < Duration::from_secs(soak) {
        match helper.events.recv_timeout(Duration::from_secs(6)) {
            Ok(CarrierEvent::Alive) => helper.alive += 1,
            Ok(other) => panic!("after {:?}: {other:?}", started.elapsed()),
            Err(_) => panic!("no sign of life for 6 s after {:?}", started.elapsed()),
        }
    }
    let beats = helper.alive - before;
    eprintln!("idle {soak} s: {beats} alive lines");
    assert!(
        beats as u64 >= soak / 3 - 2,
        "{beats} alive lines in {soak} s"
    );
    helper.echoes();
}

#[test]
#[ignore = "needs a desktop session and a system web view"]
fn a_probe_starts_the_view_and_exits() {
    let (mut child, mut stdin, events) = spawn();
    let probe = CarrierCommand::Probe {
        data_dir: Some(data_dir("probe").display().to_string()),
    };
    stdin.write_all(encode(&probe).as_bytes()).unwrap();
    assert_eq!(
        events.recv_timeout(Duration::from_secs(30)).unwrap(),
        CarrierEvent::Ready
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "the probe did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
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
        url: SecretUrl::new("http://127.0.0.1:9/relay/".into()),
        origin: "http://127.0.0.1:9".into(),
        path_prefix: "/relay/".into(),
        bridge_object_name: "TelegramWebProxy".into(),
        data_dir: None,
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
