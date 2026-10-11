//! One TDLib receive loop for every client in the process.
//!
//! `td_receive` is process-wide: it hands back the next response or update
//! of *any* client made with `td_create_client_id`, tagged with
//! `@client_id`. Two receive threads would steal each other's updates, so
//! Quill runs exactly one ([`TdHub`]) and routes every envelope to the
//! channel of the client it belongs to. Each live account (the one on screen
//! and every background account) registers its client id here before its
//! first request and reads a [`ReceiveBridge`] the hub feeds.
//!
//! The process-wide hub ([`TdHub::shared`]) is made once and never dropped,
//! so `libtdjson` is never unloaded while TDLib worker threads run. With no
//! client registered the receive thread sleeps on a condition variable
//! instead of polling `td_receive`.

use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::telegram::client::{OwnedEnvelope, ReceiveBridge, copy_and_parse};
use crate::telegram::ffi::{MAX_NATIVE_LOG_VERBOSITY, TdJson, TdJsonError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Seconds one `td_receive` call may block. Shutdown is noticed after it.
const RECEIVE_TIMEOUT_SECS: f64 = 0.2;

/// Where the receive thread reads JSON from: `td_receive` live, a channel
/// in tests. The argument is the timeout in seconds.
pub type ReceiveSource = Box<dyn FnMut(f64) -> Option<String> + Send>;

/// What happened to one received envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Handed to the channel of its client.
    Delivered,
    /// No client with that id is registered (closed already, or the
    /// envelope carries no `@client_id`); dropped.
    Unrouted,
    /// The client's reader is gone; its route was removed.
    ReaderGone,
}

/// Client id → the channel its envelopes go to.
#[derive(Default)]
pub struct Routes {
    map: Mutex<HashMap<i32, Sender<OwnedEnvelope>>>,
    /// Signalled when a route is added, so an idle receive thread wakes.
    changed: Condvar,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Routes {
    /// Start routing `client_id`'s envelopes to the returned receiver. A
    /// second registration for the same id replaces the first.
    pub fn register(&self, client_id: i32) -> Receiver<OwnedEnvelope> {
        let (tx, rx) = mpsc::channel();
        lock(&self.map).insert(client_id, tx);
        self.changed.notify_all();
        rx
    }

    pub fn unregister(&self, client_id: i32) {
        lock(&self.map).remove(&client_id);
    }

    pub fn is_registered(&self, client_id: i32) -> bool {
        lock(&self.map).contains_key(&client_id)
    }

    /// Route one envelope by its `@client_id`.
    pub fn deliver(&self, owned: OwnedEnvelope) -> Delivery {
        let Some(client_id) = owned.client_id else {
            return Delivery::Unrouted;
        };
        let mut map = lock(&self.map);
        let Some(tx) = map.get(&client_id) else {
            return Delivery::Unrouted;
        };
        if tx.send(owned).is_ok() {
            Delivery::Delivered
        } else {
            map.remove(&client_id);
            Delivery::ReaderGone
        }
    }

    /// Block while no client is registered (up to `max`), so a process
    /// with no live account does not wake `td_receive` five times a second.
    fn wait_for_client(&self, max: Duration) {
        let map = lock(&self.map);
        if map.is_empty() {
            let _ = self.changed.wait_timeout(map, max);
        }
    }
}

/// The single receive loop and its routing table.
pub struct TdHub {
    api: Option<Arc<TdJson>>,
    routes: Arc<Routes>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    /// Set when the receive loop ends without a shutdown request (a panic):
    /// every bridge then reports itself stopped.
    stopped: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// Marks the hub stopped when the loop ends for any reason other than a
/// requested shutdown, including a panic unwind.
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

static SHARED: Mutex<Option<Arc<TdHub>>> = Mutex::new(None);

impl TdHub {
    /// The process-wide hub over `libtdjson`, loaded and secured on first
    /// use. A failed load is not remembered, so a later call can retry.
    pub fn shared(sink: Arc<dyn DiagnosticSink>) -> Result<Arc<TdHub>, TdJsonError> {
        let mut slot = lock(&SHARED);
        if let Some(hub) = slot.as_ref() {
            return Ok(hub.clone());
        }
        let api = Arc::new(TdJson::load_default()?);
        // Fail closed: no client until the native log stream is emptied.
        // install_redacted_log alone does not stop TDLib writing to stderr.
        api.secure_native_logging()?;
        api.install_redacted_log(MAX_NATIVE_LOG_VERBOSITY);
        let receive = api.clone();
        let source: ReceiveSource = Box::new(move |timeout| receive.receive(timeout));
        let mut hub = TdHub::with_source(source, sink).map_err(|_| TdJsonError::Load)?;
        hub.api = Some(api);
        let hub = Arc::new(hub);
        *slot = Some(hub.clone());
        Ok(hub)
    }

    /// A hub reading from `source` on its own thread.
    pub fn with_source(
        source: ReceiveSource,
        sink: Arc<dyn DiagnosticSink>,
    ) -> std::io::Result<Self> {
        let routes = Arc::new(Routes::default());
        let seq = Arc::new(AtomicU64::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let guard = StopGuard {
            shutdown: shutdown.clone(),
            stopped: stopped.clone(),
        };
        let thread = {
            let routes = routes.clone();
            let seq = seq.clone();
            let shutdown = shutdown.clone();
            thread::Builder::new()
                .name("quill-td-receive".into())
                .spawn(move || {
                    let _guard = guard;
                    receive_loop(source, &routes, &seq, &shutdown, &sink);
                })?
        };
        Ok(Self {
            api: None,
            routes,
            seq,
            shutdown,
            stopped,
            thread: Mutex::new(Some(thread)),
        })
    }

    /// The loaded library (live hubs only).
    pub fn api(&self) -> Option<&Arc<TdJson>> {
        self.api.as_ref()
    }

    pub fn routes(&self) -> &Routes {
        &self.routes
    }

    /// A bridge that receives `client_id`'s envelopes. Call before the
    /// client's first request: TDLib sends nothing for a new client until
    /// then, so nothing is lost.
    pub fn bridge(self: &Arc<Self>, client_id: i32) -> ReceiveBridge {
        let rx = self.routes.register(client_id);
        ReceiveBridge::routed(
            rx,
            self.clone(),
            client_id,
            self.seq.clone(),
            self.stopped.clone(),
        )
    }

    pub fn stopped_unexpectedly(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    /// Stop the loop and join it. Only test hubs are ever stopped; the
    /// shared one lives as long as the process.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        self.routes.changed.notify_all();
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }
}

impl Drop for TdHub {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn receive_loop(
    mut source: ReceiveSource,
    routes: &Routes,
    seq: &AtomicU64,
    shutdown: &AtomicBool,
    sink: &Arc<dyn DiagnosticSink>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        routes.wait_for_client(Duration::from_secs(1));
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let Some(json) = source(RECEIVE_TIMEOUT_SECS) else {
            continue;
        };
        let Some(owned) = copy_and_parse(&json, seq, sink) else {
            continue;
        };
        let note = match routes.deliver(owned) {
            Delivery::Delivered => continue,
            Delivery::Unrouted => "unrouted-client",
            Delivery::ReaderGone => "client-reader-gone",
        };
        sink.record(Diagnostic {
            category: "td-receive",
            type_name: None,
            extra: None,
            seq: None,
            note,
        });
    }
}

#[cfg(test)]
mod tests;
