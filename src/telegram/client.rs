use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::telegram::envelope::{Envelope, ParseError, parse_envelope};
use crate::telegram::ffi::TdJson;
use crate::telegram::hub::TdHub;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// The sequence of an answer Quill makes itself instead of TDLib (a read
/// the agent driver refused): applied where it lands, outside the receive
/// order, so it never drops or is dropped by TDLib's envelopes.
pub const LOCAL_ANSWER_SEQ: u64 = 0;

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

/// One client's stream of envelopes. Tests inject JSON on a thread of the
/// bridge's own; live clients read what the process-wide [`TdHub`] routes
/// to their client id (`td_receive` serves every client at once).
pub struct ReceiveBridge {
    pub rx: Receiver<OwnedEnvelope>,
    tx_cmd: Sender<BridgeCommand>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    /// Set by the receive loop's exit guard when it ends without a
    /// shutdown request (channel closed, thread panicked).
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<Option<()>>>,
    /// A hub-fed bridge: the route to remove on shutdown.
    route: Option<(Arc<TdHub>, i32)>,
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
        let thread = crate::thread_guard::spawn_guarded("quill-td-receive", move || {
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
            route: None,
        })
    }

    /// A bridge fed by `hub` for `client_id` ([`TdHub::bridge`]). It owns
    /// no thread; shutting it down removes the route. `inject` is a no-op.
    pub(crate) fn routed(
        rx: Receiver<OwnedEnvelope>,
        hub: Arc<TdHub>,
        client_id: i32,
        seq: Arc<AtomicU64>,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        let (tx_cmd, _rx_cmd) = mpsc::channel();
        Self {
            rx,
            tx_cmd,
            seq,
            shutdown: Arc::new(AtomicBool::new(false)),
            stopped,
            thread: None,
            route: Some((hub, client_id)),
        }
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

    /// Signal the receive loop and join it (an injected bridge), or stop
    /// routing to this client (a hub-fed one). Safe to call twice.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.tx_cmd.send(BridgeCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Some((hub, client_id)) = self.route.take() {
            hub.routes().unregister(client_id);
        }
    }

    pub fn is_joined(&self) -> bool {
        self.thread.is_none() && self.route.is_none()
    }

    /// Wake `wake`'s poll loop for this client's envelopes
    /// ([`crate::telegram::hub::Routes::wake_with`]), and wake it now for
    /// what arrived while the other loop was woken instead. An injected
    /// bridge wakes nothing.
    pub fn wake_with(&self, wake: &'static crate::wake::Signal) {
        if let Some((hub, client_id)) = &self.route {
            hub.routes().wake_with(*client_id, wake);
            wake.poke();
        }
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
                crate::wake::UPDATES.poke();
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
        crate::json_access::set(&mut synthetic, "@client_id", client_id.clone());
    }
    parse_envelope(&synthetic.to_string()).ok()
}

/// One TDLib client in the process-wide [`TdHub`].
pub struct LiveTdJson {
    pub api: Arc<TdJson>,
    pub client_id: i32,
    pub hub: Arc<TdHub>,
}

impl LiveTdJson {
    /// A new client on the shared hub, which loads and secures `libtdjson`
    /// the first time (fail closed: no client before the native log stream
    /// is emptied). Every account gets a client id of its own.
    pub fn connect(
        sink: Arc<dyn DiagnosticSink>,
    ) -> Result<Self, crate::telegram::ffi::TdJsonError> {
        let hub = TdHub::shared(sink)?;
        let api = hub
            .api()
            .cloned()
            .ok_or(crate::telegram::ffi::TdJsonError::Load)?;
        let client_id = api.create_client_id();
        crate::telegram::hub::stop_shared_before_tdlib_teardown();
        Ok(Self {
            api,
            client_id,
            hub,
        })
    }

    /// The bridge that receives this client's envelopes. Make it before the
    /// client's first request.
    pub fn bridge(&self) -> ReceiveBridge {
        self.hub.bridge(self.client_id)
    }

    pub fn send(&self, request: &str) -> Result<(), crate::telegram::ffi::TdJsonError> {
        self.api.send(self.client_id, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::MemorySink;
    use crate::telegram::envelope::EnvelopePayload;
    use crate::telegram::envelope::{AuthPayload, MessagesPayload};

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
            EnvelopePayload::Auth(AuthPayload::UpdateAuthorizationState(_))
        ));
        assert!(matches!(b.envelope.payload, EnvelopePayload::Ok));
        assert!(matches!(
            c.envelope.payload,
            EnvelopePayload::Messages(MessagesPayload::UpdateNewMessage(_))
        ));
        assert_eq!(a.seq + 1, b.seq);
    }

    #[test]
    fn an_update_wakes_the_ui_without_waiting_for_a_poll_tick() {
        // The UI poll loop sleeps up to two seconds when idle; the receive
        // side pokes `wake::UPDATES` after queueing, so the update is
        // already readable when the loop wakes.
        let sink = Arc::new(MemorySink::new());
        let bridge = ReceiveBridge::spawn_injected(sink).unwrap();
        let mut seen = crate::wake::UPDATES.epoch();
        let start = std::time::Instant::now();
        bridge
            .inject(r#"{"@type":"updateOption","name":"x","value":{"@type":"optionValueEmpty"}}"#);
        // Other tests running in parallel poke the same global wake-up
        // (call engine hooks), so a poke with nothing queued is noise:
        // wait for the next one. Ours must arrive with the update readable.
        let mut received = false;
        while !received && start.elapsed() < Duration::from_secs(2) {
            if crate::wake::UPDATES.epoch() == seen {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            seen = crate::wake::UPDATES.epoch();
            received = bridge.rx.try_recv().is_ok();
        }
        assert!(received, "no wake-up with the update queued");
        assert!(start.elapsed() < Duration::from_millis(500));
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
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(bridge.stopped_unexpectedly());
        let sink = Arc::new(MemorySink::new());
        let mut clean = ReceiveBridge::spawn_injected(sink).unwrap();
        clean.shutdown();
        assert!(!clean.stopped_unexpectedly());
    }

    #[test]
    fn a_closed_authorization_update_does_not_stop_the_bridge() {
        // A clean logout ends with TDLib's Closed state; that is data the
        // loop delivers, not the end of the loop, so it never reads as a
        // lost connection.
        let sink = Arc::new(MemorySink::new());
        let mut bridge = ReceiveBridge::spawn_injected(sink).unwrap();
        bridge.inject(
            r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateClosed"}}"#,
        );
        let got = bridge.next_timeout(Duration::from_secs(1));
        assert!(got.is_some(), "the Closed update is delivered");
        std::thread::sleep(Duration::from_millis(150));
        assert!(!bridge.stopped_unexpectedly());
        bridge.shutdown();
    }
}
