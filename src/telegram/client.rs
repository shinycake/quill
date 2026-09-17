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
    thread: Option<JoinHandle<()>>,
}

impl ReceiveBridge {
    pub fn spawn_injected(sink: Arc<dyn DiagnosticSink>) -> Self {
        let (tx_out, rx_out) = mpsc::channel();
        let (tx_cmd, rx_cmd) = mpsc::channel();
        let seq = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let seq_thread = seq.clone();
        let shutdown_thread = shutdown.clone();
        let thread = thread::Builder::new()
            .name("quill-td-receive".into())
            .spawn(move || injected_loop(rx_cmd, tx_out, seq_thread, shutdown_thread, sink))
            .expect("receive thread");
        Self {
            rx: rx_out,
            tx_cmd,
            seq,
            shutdown,
            thread: Some(thread),
        }
    }

    /// Live `td_receive` loop on a dedicated thread. JSON is copied before the next receive.
    pub fn spawn_live(api: Arc<TdJson>, sink: Arc<dyn DiagnosticSink>) -> Self {
        let (tx_out, rx_out) = mpsc::channel();
        let (tx_cmd, _rx_cmd) = mpsc::channel();
        let seq = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let seq_thread = seq.clone();
        let shutdown_thread = shutdown.clone();
        let thread = thread::Builder::new()
            .name("quill-td-receive".into())
            .spawn(move || ordered_receive_loop(api, tx_out, seq_thread, shutdown_thread, sink))
            .expect("receive thread");
        Self {
            rx: rx_out,
            tx_cmd,
            seq,
            shutdown,
            thread: Some(thread),
        }
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
}

impl Drop for ReceiveBridge {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = self.tx_cmd.send(BridgeCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn injected_loop(
    rx_cmd: Receiver<BridgeCommand>,
    tx_out: Sender<OwnedEnvelope>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    sink: Arc<dyn DiagnosticSink>,
) {
    while !shutdown.load(Ordering::Relaxed) {
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
        Err(ParseError::InvalidJson) => {
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: None,
                extra: None,
                seq: Some(next),
                note: "invalid-json",
            });
            None
        }
        Err(_) => {
            sink.record(Diagnostic {
                category: "td-receive",
                type_name: None,
                extra: None,
                seq: Some(next),
                note: "parse-error",
            });
            None
        }
    }
}

pub struct LiveTdJson {
    pub api: Arc<TdJson>,
    pub client_id: i32,
}

impl LiveTdJson {
    pub fn connect() -> Result<Self, crate::telegram::ffi::TdJsonError> {
        let api = Arc::new(TdJson::load_default()?);
        api.install_redacted_log(1);
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
    while !shutdown.load(Ordering::Relaxed) {
        let Some(json) = api.receive(0.2) else {
            continue;
        };
        if let Some(owned) = copy_and_parse(&json, &seq, &sink)
            && tx_out.send(owned).is_err()
        {
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
        let bridge = ReceiveBridge::spawn_injected(sink.clone());
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
        let bridge = ReceiveBridge::spawn_injected(sink.clone());
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
}
