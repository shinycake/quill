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
//! instead of calling `td_receive`, and with one it sleeps inside
//! `td_receive` until something arrives.
//!
//! It is stopped once, at process exit, before TDLib's own teardown: `exit`
//! destroys TDLib's static client manager, and a receive thread still inside
//! `td_receive` then reads freed memory ([`stop_shared_before_tdlib_teardown`]).
//! While a client may still be open, exit then skips TDLib's teardown, on
//! which TDLib's workers abort.

use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::telegram::client::{OwnedEnvelope, ReceiveBridge, copy_and_parse};
use crate::telegram::ffi::{MAX_NATIVE_LOG_VERBOSITY, TdJson, TdJsonError};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

mod exit;
pub(crate) use exit::note_made;
pub use exit::{set_exit_status, stop_shared_before_tdlib_teardown};

/// Seconds one `td_receive` call may block. Anything TDLib has to say ends
/// it sooner, and a stop makes TDLib say something ([`ReceiveWake`]), so
/// nothing depends on this length.
const RECEIVE_TIMEOUT_SECS: f64 = 60.0;

/// How long process exit waits for the receive thread to leave
/// `td_receive`, which [`ReceiveWake`] takes milliseconds to bring about.
/// Past it, exit goes on.
const EXIT_STOP_WAIT: Duration = Duration::from_millis(500);

/// Where the receive thread reads JSON from: `td_receive` live, a channel
/// in tests. The argument is the timeout in seconds.
pub type ReceiveSource = Box<dyn FnMut(f64) -> Option<String> + Send>;

/// Ends a call into the [`ReceiveSource`] that is blocked, or the next one.
pub type ReceiveWake = Box<dyn Fn() + Send + Sync>;

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

/// Where one client's envelopes go, and the poll loop to wake for them.
struct Route {
    tx: Sender<OwnedEnvelope>,
    wake: &'static crate::wake::Signal,
}

/// Client id → the channel its envelopes go to.
#[derive(Default)]
pub struct Routes {
    map: Mutex<HashMap<i32, Route>>,
    /// Signalled when a route is added, so an idle receive thread wakes.
    changed: Condvar,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    crate::locks::lock_or_recover(mutex)
}

impl Routes {
    /// Start routing `client_id`'s envelopes to the returned receiver,
    /// waking the on-screen poll loop ([`crate::wake::UPDATES`]) until
    /// [`Routes::wake_with`] says otherwise. A second registration for the
    /// same id replaces the first.
    pub fn register(&self, client_id: i32) -> Receiver<OwnedEnvelope> {
        let (tx, rx) = mpsc::channel();
        let wake = &crate::wake::UPDATES;
        lock(&self.map).insert(client_id, Route { tx, wake });
        self.changed.notify_all();
        rx
    }

    /// Wake `wake`'s poll loop for `client_id`'s envelopes from now on: an
    /// account switch moves a client between the on-screen loop and the
    /// background one, which then sleep through each other's updates.
    pub fn wake_with(&self, client_id: i32, wake: &'static crate::wake::Signal) {
        if let Some(route) = lock(&self.map).get_mut(&client_id) {
            route.wake = wake;
        }
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
        let Some(route) = map.get(&client_id) else {
            return Delivery::Unrouted;
        };
        if route.tx.send(owned).is_ok() {
            // Wake the client's poll loop, which sleeps while nothing arrives.
            route.wake.poke();
            Delivery::Delivered
        } else {
            map.remove(&client_id);
            Delivery::ReaderGone
        }
    }

    /// Block while no client is registered, so `td_receive` only runs after
    /// a client id was made, and with it the exit hook
    /// ([`stop_shared_before_tdlib_teardown`]). The shutdown flag is read
    /// under the lock [`TdHub::request_stop`] notifies under, so a stop is
    /// never missed.
    fn wait_for_client(&self, shutdown: &AtomicBool) {
        let mut map = lock(&self.map);
        while map.is_empty() && !shutdown.load(Ordering::SeqCst) {
            map = self
                .changed
                .wait(map)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
}

/// The single receive loop and its routing table.
pub struct TdHub {
    api: Option<Arc<TdJson>>,
    routes: Arc<Routes>,
    seq: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    wake: ReceiveWake,
    /// Set when the receive loop ends without a shutdown request (a panic):
    /// every bridge then reports itself stopped.
    stopped: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<Option<()>>>>,
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

/// What the shared slot holds, as far as [`TdHub::shared`] is concerned.
enum Reuse {
    None,
    Running(Arc<TdHub>),
    /// A hub whose receive thread ended unexpectedly, with its library.
    Stopped(Option<Arc<TdJson>>),
}

fn reusable(slot: Option<&Arc<TdHub>>) -> Reuse {
    match slot {
        None => Reuse::None,
        Some(hub) if hub.stopped_unexpectedly() => Reuse::Stopped(hub.api().cloned()),
        Some(hub) => Reuse::Running(hub.clone()),
    }
}

impl TdHub {
    /// The process-wide hub over `libtdjson`, loaded and secured on first
    /// use. A failed load is not remembered, so a later call can retry.
    pub fn shared(sink: Arc<dyn DiagnosticSink>) -> Result<Arc<TdHub>, TdJsonError> {
        let mut slot = lock(&SHARED);
        let api = match reusable(slot.as_ref()) {
            Reuse::Running(hub) => return Ok(hub),
            // The receive thread died: keep the loaded library (it is
            // already secured and must stay loaded for TDLib's worker
            // threads) and only start a new receive thread over it.
            Reuse::Stopped(Some(api)) => api,
            Reuse::Stopped(None) | Reuse::None => Self::load_secured_api()?,
        };
        let receive = api.clone();
        let source: ReceiveSource = Box::new(move |timeout| receive.receive(timeout));
        let waker = api.clone();
        let wake: ReceiveWake = Box::new(move || waker.wake_receive());
        let mut hub = TdHub::with_source(source, wake, sink).map_err(|_| TdJsonError::Load)?;
        hub.api = Some(api);
        let hub = Arc::new(hub);
        *slot = Some(hub.clone());
        Ok(hub)
    }

    /// Load `libtdjson` and close its native log stream.
    fn load_secured_api() -> Result<Arc<TdJson>, TdJsonError> {
        let api = Arc::new(TdJson::load_default()?);
        if let Some(root) = crate::settings::safe_app_root() {
            crate::crash_log::arm_native_fatal_breadcrumb(&root);
        }
        // Fail closed: no client until the native log stream is emptied.
        // install_redacted_log alone does not stop TDLib writing to stderr.
        api.secure_native_logging()?;
        api.check_pinned_version()?;
        api.install_redacted_log(MAX_NATIVE_LOG_VERBOSITY);
        // Debug mode (`advanced::debug_mode`) opens the native log file only
        // after the stream above is closed, so a failure leaves it closed.
        if crate::advanced::debug_mode::enabled()
            && let Some(root) = crate::settings::safe_app_root()
        {
            let _ = crate::advanced::debug_mode::start_tdlib_log(&api, &root);
        }
        Ok(api)
    }

    /// The loaded `libtdjson` of the process-wide hub, if one was made.
    pub fn shared_api() -> Option<Arc<TdJson>> {
        lock(&SHARED).as_ref().and_then(|hub| hub.api().cloned())
    }

    /// A hub reading from `source` on its own thread.
    pub fn with_source(
        source: ReceiveSource,
        wake: ReceiveWake,
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
            crate::thread_guard::spawn_guarded("quill-td-receive", move || {
                let _guard = guard;
                receive_loop(source, &routes, &seq, &shutdown, &sink);
            })?
        };
        Ok(Self {
            api: None,
            routes,
            seq,
            shutdown,
            wake,
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

    /// Stop the loop and join it. Test hubs are stopped here; the shared
    /// one only at process exit, with [`TdHub::stop_within`].
    pub fn shutdown(&self) {
        self.request_stop();
        if let Some(thread) = lock(&self.thread).take() {
            let _ = thread.join();
        }
    }

    /// Stop the loop and wait up to `max` for its thread to end. True once
    /// it has: it will never call `td_receive` again.
    pub fn stop_within(&self, max: Duration) -> bool {
        self.request_stop();
        let Some(thread) = lock(&self.thread).take() else {
            return true;
        };
        let deadline = std::time::Instant::now() + max;
        while !thread.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        if !thread.is_finished() {
            return false;
        }
        let _ = thread.join();
        true
    }

    fn request_stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let map = lock(&self.routes.map);
        self.routes.changed.notify_all();
        drop(map);
        (self.wake)();
    }
}

impl Drop for TdHub {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Consecutive panicked iterations after which the loop gives up and the
/// hub reports itself stopped (the shared hub is then replaced on the next
/// connect), instead of spinning on a source that keeps panicking.
const MAX_CONSECUTIVE_PANICS: u32 = 16;
/// Pause after a panicked iteration.
const PANIC_PAUSE: Duration = Duration::from_millis(10);

fn receive_loop(
    mut source: ReceiveSource,
    routes: &Routes,
    seq: &AtomicU64,
    shutdown: &AtomicBool,
    sink: &Arc<dyn DiagnosticSink>,
) {
    let mut panics = 0u32;
    while !shutdown.load(Ordering::SeqCst) {
        // One envelope's parse or delivery going wrong must not end the
        // receive thread: it serves every account in the process. The panic
        // hook has already filed the report (`crate::thread_guard`).
        let finished = crate::thread_guard::ffi_boundary(false, || {
            receive_one(&mut source, routes, seq, shutdown, sink);
            true
        });
        if finished {
            panics = 0;
            continue;
        }
        panics = panics.saturating_add(1);
        if panics >= MAX_CONSECUTIVE_PANICS {
            return;
        }
        std::thread::sleep(PANIC_PAUSE);
    }
}

/// One pass of the loop: wait for a client, read one envelope, route it.
fn receive_one(
    source: &mut ReceiveSource,
    routes: &Routes,
    seq: &AtomicU64,
    shutdown: &AtomicBool,
    sink: &Arc<dyn DiagnosticSink>,
) {
    routes.wait_for_client(shutdown);
    if shutdown.load(Ordering::SeqCst) {
        return;
    }
    let Some(json) = source(RECEIVE_TIMEOUT_SECS) else {
        return;
    };
    // What ended the call may be the stop's own wake.
    if shutdown.load(Ordering::SeqCst) {
        return;
    }
    // A conference call's frame answers go to the call thread waiting on
    // them, before anything else looks at the envelope.
    if crate::telegram::group_call_crypto::intercept(&json) {
        return;
    }
    // Answers to agent requests go to the agent, never to an account's session.
    if crate::agent::td::intercept(&json) {
        return;
    }
    crate::agent::events::observe(&json);
    let Some(owned) = copy_and_parse(&json, seq, sink) else {
        return;
    };
    exit::note_received(&owned);
    let note = match routes.deliver(owned) {
        Delivery::Delivered => return,
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

#[cfg(test)]
mod tests;
