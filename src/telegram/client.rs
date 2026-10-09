use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::telegram::envelope::{Envelope, ParseError, parse_envelope};
use crate::telegram::ffi::TdJson;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Owned JSON copied off the TDLib receive pointer, with a monotonic sequence.
#[derive(Debug, Clone)]
pub struct OwnedEnvelope {
    pub seq: u64,
    pub client_id: Option<i32>,
    pub envelope: Envelope,
}

pub enum BridgeCommand {
    Json(String),
    Shutdown,
}

/// One dedicated receive loop. Tests inject JSON; live mode copies `td_receive`.
pub struct ReceiveBridge {
    pub rx: Receiver<OwnedEnvelope>,
    tx_cmd: Sender<BridgeCommand>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    /// Set by the receive loop's exit guard when it ends without a
    /// shutdown request (channel closed, thread panicked).
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Marks the bridge as stopped when the receive loop ends for any reason
/// other than a requested shutdown, including a panic unwind.
struct StopGuard {
    shutdown: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

impl Drop for StopGuard {
    fn drop(&mut self) {
        if !self.shutdown.load(Ordering::SeqCst) {
            self.stopped.store(true, Ordering::SeqCst);
        }
    }
}

impl ReceiveBridge {
    pub fn spawn_injected(sink: Arc<dyn DiagnosticSink>) -> std::io::Result<Self> {
        let (tx_out, rx_out) = mpsc::channel();
        let (tx_cmd, rx_cmd) = mpsc::channel();
        let seq = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let seq_thread = seq.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let guard = StopGuard {
            shutdown: shutdown.clone(),
            stopped: stopped.clone(),
        };
        let shutdown_thread = shutdown.clone();
        let thread = thread::Builder::new()
            .name("quill-td-receive".into())
            .spawn(move || {
                let _guard = guard;
                injected_loop(rx_cmd, tx_out, seq_thread, shutdown_thread, sink)
            })?;
        Ok(Self {
            rx: rx_out,
            tx_cmd,
            seq,
            shutdown,
            stopped,
            thread: Some(thread),
        })
    }

    /// Live `td_receive` loop on a dedicated thread. JSON is copied before the next receive.
    pub fn spawn_live(api: Arc<TdJson>, sink: Arc<dyn DiagnosticSink>) -> std::io::Result<Self> {
        let (tx_out, rx_out) = mpsc::channel();
        let (tx_cmd, _rx_cmd) = mpsc::channel();
        let seq = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let seq_thread = seq.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let guard = StopGuard {
            shutdown: shutdown.clone(),
            stopped: stopped.clone(),
        };
        let shutdown_thread = shutdown.clone();
        let thread = thread::Builder::new()
            .name("quill-td-receive".into())
            .spawn(move || {
                let _guard = guard;
                ordered_receive_loop(api, tx_out, seq_thread, shutdown_thread, sink)
            })?;
        Ok(Self {
            rx: rx_out,
            tx_cmd,
            seq,
            shutdown,
            stopped,
            thread: Some(thread),
        })
    }

    /// The receive loop ended without a shutdown request, so no more
    /// envelopes will arrive. Queued envelopes may still be drained first.
    pub fn stopped_unexpectedly(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn inject(&self, json: impl Into<String>) {
        let _ = self.tx_cmd.send(BridgeCommand::Json(json.into()));
    }

    pub fn next_timeout(&self, timeout: Duration) -> Option<OwnedEnvelope> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Signal the receive loop and join it. Safe to call twice.
    /// Live mode only notices this after the current `td_receive` timeout (~200ms).
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.tx_cmd.send(BridgeCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    pub fn is_joined(&self) -> bool {
        self.thread.is_none()
    }
}

impl Drop for ReceiveBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn injected_loop(
    rx_cmd: Receiver<BridgeCommand>,
    tx_out: Sender<OwnedEnvelope>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    sink: Arc<dyn DiagnosticSink>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match rx_cmd.recv_timeout(Duration::from_millis(50)) {
            Ok(BridgeCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => continue,
            Ok(BridgeCommand::Json(json)) => {
                if let Some(owned) = copy_and_parse(&json, &seq, &sink)
                    && tx_out.send(owned).is_err()
                {
                    break;
                }
            }
        }
    }
}

pub fn copy_and_parse(
    json: &str,
    seq: &AtomicU64,
    sink: &Arc<dyn DiagnosticSink>,
) -> Option<OwnedEnvelope> {
    let next = seq.fetch_add(1, Ordering::SeqCst) + 1;
    match parse_envelope(json) {
        Ok(envelope) => {
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: Some(envelope.type_name.clone()),
                extra: envelope.extra.map(|id| id.0),
                seq: Some(next),
                note: "ok",
            });
            Some(OwnedEnvelope {
                seq: next,
                client_id: envelope.client_id,
                envelope,
            })
        }
        Err(err) => {
            let note = if err == ParseError::InvalidJson {
                "invalid-json"
            } else {
                "parse-error"
            };
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: None,
                extra: None,
                seq: Some(next),
                note,
            });
            // A response nobody can parse would leave its request pending
            // forever. When the `@extra` survives, deliver a synthetic
            // error so the UI shows a failure instead of a spinner.
            let envelope = recover_failed_response(json)?;
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: None,
                extra: envelope.extra.map(|id| id.0),
                seq: Some(next),
                note: "unparsable-response-failed",
            });
            Some(OwnedEnvelope {
                seq: next,
                client_id: envelope.client_id,
                envelope,
            })
        }
    }
}

/// Build the error envelope for an unparsable response whose `@extra` can
/// still be read. Returns `None` for pushed updates (no `@extra`) and for
/// non-JSON input. The raw message is never copied into the error.
pub fn recover_failed_response(json: &str) -> Option<Envelope> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let extra = value.get("@extra")?;
    if !(extra.is_string() || extra.is_number()) {
        return None;
    }
    let mut synthetic = serde_json::json!({
        "@type": "error",
        "code": 500,
        "message": "QUILL_UNPARSABLE_RESPONSE",
        "@extra": extra,
    });
    if let Some(client_id) = value.get("@client_id") {
        synthetic["@client_id"] = client_id.clone();
    }
    parse_envelope(&synthetic.to_string()).ok()
}

pub struct LiveTdJson {
    pub api: Arc<TdJson>,
    pub client_id: i32,
}

impl LiveTdJson {
    pub fn connect() -> Result<Self, crate::telegram::ffi::TdJsonError> {
        let api = Arc::new(TdJson::load_default()?);
        // Fail closed: no client until the native log stream is emptied.
        // install_redacted_log alone does not stop TDLib writing to stderr.
        api.secure_native_logging()?;
        api.install_redacted_log(crate::telegram::ffi::MAX_NATIVE_LOG_VERBOSITY);
        let client_id = api.create_client_id();
        Ok(Self { api, client_id })
    }

    pub fn send(&self, request: &str) -> Result<(), crate::telegram::ffi::TdJsonError> {
        self.api.send(self.client_id, request)
    }
}

/// Blocking receive on a dedicated thread. The JSON pointer is copied immediately.
pub fn ordered_receive_loop(
    api: Arc<TdJson>,
    tx_out: Sender<OwnedEnvelope>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    sink: Arc<dyn DiagnosticSink>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        let Some(json) = api.receive(0.2) else {
            continue;
        };
        if let Some(owned) = copy_and_parse(&json, &seq, &sink)
            && tx_out.send(owned).is_err()
        {
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: None,
                extra: None,
                seq: None,
                note: "bridge-stopped",
            });
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::telegram::envelope::EnvelopePayload;

    #[test]
    fn receive_order_matches_injection_order() {
        let sink = Arc::new(MemorySink::new());
        let bridge = ReceiveBridge::spawn_injected(sink.clone()).unwrap();
        bridge.inject(r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"},"@extra":"1"}"#);
        bridge.inject(r#"{"@type":"ok","@extra":"1"}"#);
        bridge.inject(r#"{"@type":"updateNewMessage","message":{"id":10,"chat_id":1,"is_outgoing":false,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"hi","entities":[]}}}}"#);

        let a = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        let b = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        let c = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(a.seq, 1);
        assert_eq!(b.seq, 2);
        assert_eq!(c.seq, 3);
        assert!(matches!(
            a.envelope.payload,
            EnvelopePayload::UpdateAuthorizationState(_)
        ));
        assert!(matches!(b.envelope.payload, EnvelopePayload::Ok));
        assert!(matches!(
            c.envelope.payload,
            EnvelopePayload::UpdateNewMessage(_)
        ));
        assert_eq!(a.seq + 1, b.seq);
    }

    #[test]
    fn canary_secret_is_absent_from_diagnostics() {
        let sink = Arc::new(MemorySink::new());
        let bridge = ReceiveBridge::spawn_injected(sink.clone()).unwrap();
        bridge.inject(
            r#"{"@type":"updateSomethingSecret","phone":"CANARY_PHONE_+15551212","text":"CANARY_MSG"}"#,
        );
        let env = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(env.envelope.payload, EnvelopePayload::Unknown(_)));
        let rendered = sink.rendered();
        assert!(!rendered.contains("CANARY_PHONE"));
        assert!(!rendered.contains("CANARY_MSG"));
        assert!(!rendered.contains("+15551212"));
        assert!(rendered.contains("updateSomethingSecret"));
    }

    #[test]
    fn drop_injected_bridge_joins_without_panic() {
        let sink = Arc::new(MemorySink::new());
        let mut bridge = ReceiveBridge::spawn_injected(sink).unwrap();
        bridge.inject(r#"{"@type":"ok"}"#);
        let _ = bridge.next_timeout(Duration::from_secs(1));
        bridge.shutdown();
        assert!(bridge.is_joined());
        bridge.shutdown();
        drop(bridge);
    }

    #[test]
    fn unparsable_response_with_extra_resolves_as_error() {
        let sink = Arc::new(MemorySink::new());
        let bridge = ReceiveBridge::spawn_injected(sink.clone()).unwrap();
        // A response whose required `message` field is missing.
        bridge.inject(r#"{"@type":"updateNewMessage","note":"CANARY_BAD","@extra":"41"}"#);
        // A pushed update that cannot parse has no request to fail.
        bridge.inject(r#"{"@type":"updateNewMessage","message":"CANARY_BAD"}"#);
        bridge.inject(r#"{"@type":"ok"}"#);
        let first = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(first.envelope.extra, Some(crate::ids::RequestId(41)));
        assert!(matches!(first.envelope.payload, EnvelopePayload::Error(_)));
        let next = bridge.next_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(next.envelope.payload, EnvelopePayload::Ok));
        assert!(!sink.rendered().contains("CANARY_BAD"));
    }

    #[test]
    fn recover_failed_response_needs_a_readable_extra() {
        assert!(recover_failed_response("not json").is_none());
        assert!(recover_failed_response(r#"{"@type":"x"}"#).is_none());
        assert!(recover_failed_response(r#"{"@type":"x","@extra":{"a":1}}"#).is_none());
        let env = recover_failed_response(r#"{"@type":"x","@extra":7,"@client_id":3}"#).unwrap();
        assert_eq!(env.extra, Some(crate::ids::RequestId(7)));
        assert_eq!(env.client_id, Some(3));
    }

    #[test]
    fn bridge_reports_unexpected_stop_but_not_requested_shutdown() {
        let sink = Arc::new(MemorySink::new());
        let mut bridge = ReceiveBridge::spawn_injected(sink).unwrap();
        assert!(!bridge.stopped_unexpectedly());
        // Dropping the command side ends the loop without a shutdown flag.
        let (dead_tx, _) = mpsc::channel();
        bridge.tx_cmd = dead_tx;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !bridge.stopped_unexpectedly() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(bridge.stopped_unexpectedly());
        let sink = Arc::new(MemorySink::new());
        let mut clean = ReceiveBridge::spawn_injected(sink).unwrap();
        clean.shutdown();
        assert!(!clean.stopped_unexpectedly());
    }
}
