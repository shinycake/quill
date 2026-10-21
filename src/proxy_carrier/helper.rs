//! The live [`CarrierHost`]: one `quill-webview --carrier` process per
//! carrier (`crates/webview-host/src/carrier.rs`).
//!
//! Each carrier gets a writer thread (owns the child and its stdin) and a
//! reader thread (owns its stdout). Neither ever blocks the caller: data
//! goes through a bounded queue whose byte count is capped, and a helper
//! that stops reading, stops answering or dies becomes a `Terminated`
//! event instead. Dropping the host, or a carrier's queue, kills the child.

use super::{CarrierError, CarrierHost, CarrierMessage, CarrierRequest, HostEvent};
use quill_webview_protocol::carrier::{
    CarrierCommand, CarrierEvent, CarrierFailure, MAX_LINE_BYTES, MAX_PENDING_BYTES, base64_len,
};
use quill_webview_protocol::lines::{Line, read_bounded_line};
use quill_webview_protocol::{decode, encode};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, channel};
use std::time::{Duration, Instant};

/// Commands that may wait for one carrier's writer thread.
const QUEUE_LEN: usize = 1024;
/// The helper sends `alive` every 3 s; this much silence means its loop is
/// stuck (or the whole process is), whatever the page does.
const HELPER_SILENCE: Duration = Duration::from_secs(15);
/// How often a writer thread looks at the silence clock.
const WATCH_TICK: Duration = Duration::from_secs(1);
/// How long a closed helper gets to send the page `{"t":"close"}` and go
/// before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(1);

/// `quill-webview` next to Quill's own executable, if it is there.
pub fn helper_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "quill-webview.exe"
    } else {
        "quill-webview"
    };
    let sibling = exe.with_file_name(name);
    sibling.is_file().then_some(sibling)
}

/// What the threads of one carrier share.
struct Shared {
    started: Instant,
    /// Milliseconds after `started` of the helper's last line.
    last_heard_ms: AtomicU64,
    /// Base64 bytes queued for the writer.
    queued: AtomicUsize,
    /// A failure was reported (only the first one counts).
    failed: AtomicBool,
    /// Quill closed it: the exit that follows is not a failure.
    closing: AtomicBool,
    /// The helper said `ready` (an exit before that means it cannot run).
    ready: AtomicBool,
}

impl Shared {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            last_heard_ms: AtomicU64::new(0),
            queued: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            ready: AtomicBool::new(false),
        }
    }

    fn heard(&self) {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_heard_ms.store(ms, Ordering::Release);
    }

    /// `size` queued bytes left for the helper (never below zero).
    fn dequeued(&self, size: usize) {
        let mut current = self.queued.load(Ordering::Acquire);
        loop {
            let next = current.saturating_sub(size);
            match self.queued.compare_exchange_weak(
                current,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return,
                Err(seen) => current = seen,
            }
        }
    }

    fn silent_for(&self) -> Duration {
        let heard = Duration::from_millis(self.last_heard_ms.load(Ordering::Acquire));
        self.started.elapsed().saturating_sub(heard)
    }

    /// Report `error` unless something was reported already or Quill is
    /// closing the carrier itself.
    fn fail(&self, carrier_id: i32, error: CarrierError, events: &Sender<(i32, HostEvent)>) {
        if self.closing.load(Ordering::Acquire) || self.failed.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = events.send((carrier_id, HostEvent::Failed(error)));
        crate::wake::UPDATES.poke();
    }
}

struct Running {
    commands: SyncSender<CarrierCommand>,
    shared: Arc<Shared>,
}

pub struct HelperCarrierHost {
    program: Option<PathBuf>,
    carriers: HashMap<i32, Running>,
    events_tx: Sender<(i32, HostEvent)>,
    events: Receiver<(i32, HostEvent)>,
}

impl HelperCarrierHost {
    /// A host running `program`; `None` hosts nothing ([`CarrierHost::available`]
    /// is false).
    pub fn new(program: Option<PathBuf>) -> Self {
        let (events_tx, events) = channel();
        Self {
            program,
            carriers: HashMap::new(),
            events_tx,
            events,
        }
    }

    fn spawn(&self, program: &PathBuf) -> std::io::Result<Child> {
        Command::new(program)
            .arg("--carrier")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
                Stdio::inherit()
            } else {
                Stdio::null()
            })
            .spawn()
    }
}

impl CarrierHost for HelperCarrierHost {
    fn available(&self) -> bool {
        self.program.is_some()
    }

    fn open(&mut self, request: &CarrierRequest) -> Result<(), CarrierError> {
        let program = self.program.as_ref().ok_or(CarrierError::Unsupported)?;
        let mut child = self.spawn(program).map_err(|_| CarrierError::Unsupported)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            crate::child_reap::kill_and_reap(child);
            return Err(CarrierError::Unsupported);
        };
        let carrier_id = request.id;
        let shared = Arc::new(Shared::new());
        let (commands, queue) = std::sync::mpsc::sync_channel(QUEUE_LEN);
        let open = CarrierCommand::Open {
            url: request.url.expose().to_string(),
            origin: request.origin.clone(),
            path_prefix: request.path_prefix.clone(),
            bridge_object_name: request.bridge_object_name.clone(),
        };
        let writer = {
            let shared = shared.clone();
            let events = self.events_tx.clone();
            crate::thread_guard::spawn_guarded("quill-carrier-writer", move || {
                write_commands(carrier_id, child, stdin, open, queue, &shared, &events)
            })
        };
        if writer.is_err() {
            return Err(CarrierError::Unsupported);
        }
        let reader = {
            let shared = shared.clone();
            let events = self.events_tx.clone();
            crate::thread_guard::spawn_guarded("quill-carrier-reader", move || {
                read_events(carrier_id, stdout, &shared, &events)
            })
        };
        if reader.is_err() {
            // The writer kills the child once its queue is gone.
            return Err(CarrierError::Unsupported);
        }
        self.carriers
            .insert(carrier_id, Running { commands, shared });
        Ok(())
    }

    fn deliver(&mut self, carrier_id: i32, data: String) {
        let Some(running) = self.carriers.get(&carrier_id) else {
            return;
        };
        let size = data.len();
        let queued = running.shared.queued.fetch_add(size, Ordering::AcqRel);
        let over = queued.saturating_add(size) > base64_len(MAX_PENDING_BYTES);
        let sent = !over
            && match running.commands.try_send(CarrierCommand::Deliver { data }) {
                Ok(()) => true,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
            };
        if !sent {
            // The helper is not keeping up: fail it, and dropping the queue
            // makes the writer kill it.
            running
                .shared
                .fail(carrier_id, CarrierError::Terminated, &self.events_tx);
            self.carriers.remove(&carrier_id);
        }
    }

    fn close(&mut self, carrier_id: i32, notify_page: bool) {
        if let Some(running) = self.carriers.remove(&carrier_id) {
            running.shared.closing.store(true, Ordering::Release);
            let _ = running
                .commands
                .try_send(CarrierCommand::Close { notify_page });
        }
    }

    fn poll(&mut self) -> Vec<(i32, HostEvent)> {
        self.events.try_iter().collect()
    }

    fn shutdown(&mut self) {
        for (_, running) in self.carriers.drain() {
            running.shared.closing.store(true, Ordering::Release);
        }
    }
}

impl Drop for HelperCarrierHost {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn write_line(stdin: &mut ChildStdin, command: &CarrierCommand) -> bool {
    stdin
        .write_all(encode(command).as_bytes())
        .and_then(|()| stdin.flush())
        .is_ok()
}

/// One carrier's writer: sends `open`, then the queue, and watches the
/// helper's silence clock. Owns the child; every way out ends it.
fn write_commands(
    carrier_id: i32,
    child: Child,
    mut stdin: ChildStdin,
    open: CarrierCommand,
    queue: Receiver<CarrierCommand>,
    shared: &Shared,
    events: &Sender<(i32, HostEvent)>,
) {
    shared.heard();
    if !write_line(&mut stdin, &open) {
        shared.fail(carrier_id, CarrierError::Unsupported, events);
        crate::child_reap::kill_and_reap(child);
        return;
    }
    drop(open);
    loop {
        match queue.recv_timeout(WATCH_TICK) {
            Ok(CarrierCommand::Close { notify_page }) => {
                let _ = write_line(&mut stdin, &CarrierCommand::Close { notify_page });
                drop(stdin);
                crate::child_reap::reap_after(child, CLOSE_GRACE);
                return;
            }
            Ok(command) => {
                if let CarrierCommand::Deliver { data } = &command {
                    shared.dequeued(data.len());
                }
                if !write_line(&mut stdin, &command) {
                    shared.fail(carrier_id, CarrierError::Terminated, events);
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if shared.silent_for() > HELPER_SILENCE {
                    shared.fail(carrier_id, CarrierError::Terminated, events);
                    break;
                }
            }
            // The host dropped the carrier (failure, shutdown, quit).
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(stdin);
    crate::child_reap::kill_and_reap(child);
}

/// One carrier's reader: the helper's lines until it exits.
fn read_events(
    carrier_id: i32,
    stdout: std::process::ChildStdout,
    shared: &Shared,
    events: &Sender<(i32, HostEvent)>,
) {
    let mut reader = std::io::BufReader::new(stdout);
    loop {
        let line = match read_bounded_line(&mut reader, MAX_LINE_BYTES) {
            Ok(Line::Text(line)) => line,
            // A line over the cap is a broken relay, not something to skip.
            Ok(Line::TooLong) => {
                shared.fail(carrier_id, CarrierError::Terminated, events);
                return;
            }
            Ok(Line::End) | Err(_) => break,
        };
        let Some(event) = std::str::from_utf8(&line)
            .ok()
            .and_then(|line| decode::<CarrierEvent>(line).ok())
        else {
            continue;
        };
        shared.heard();
        let message = match event {
            CarrierEvent::Ready => {
                shared.ready.store(true, Ordering::Release);
                continue;
            }
            CarrierEvent::Alive => continue,
            CarrierEvent::Binary { data } => CarrierMessage::Binary(data),
            CarrierEvent::Text { text } => CarrierMessage::Text(text),
            CarrierEvent::Failed { failure } => {
                shared.fail(carrier_id, carrier_error(failure), events);
                return;
            }
        };
        if shared.failed.load(Ordering::Acquire) || shared.closing.load(Ordering::Acquire) {
            continue;
        }
        if events
            .send((carrier_id, HostEvent::Message(message)))
            .is_err()
        {
            return;
        }
        crate::wake::UPDATES.poke();
    }
    // The helper exited without saying why. Before `ready` it could not
    // even start (no WebKitGTK, no WebView2): TDLib should stop asking.
    let error = if shared.ready.load(Ordering::Acquire) {
        CarrierError::Terminated
    } else {
        CarrierError::Unsupported
    };
    shared.fail(carrier_id, error, events);
}

fn carrier_error(failure: CarrierFailure) -> CarrierError {
    match failure {
        CarrierFailure::Unsupported => CarrierError::Unsupported,
        CarrierFailure::LoadFailed { message } => CarrierError::LoadFailed(message),
        CarrierFailure::Terminated => CarrierError::Terminated,
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    fn request(id: i32) -> CarrierRequest {
        CarrierRequest {
            id,
            proxy_id: 1,
            url: super::super::CarrierUrl::new("https://proxy.example.com/?bridge=x".into()),
            origin: "https://proxy.example.com".into(),
            path_prefix: "/".into(),
            bridge_object_name: "TelegramWebProxy".into(),
            temporary: false,
        }
    }

    fn wait_event(host: &mut HelperCarrierHost) -> Option<(i32, HostEvent)> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(event) = host.poll().into_iter().next() {
                return Some(event);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn no_program_hosts_nothing() {
        let mut host = HelperCarrierHost::new(None);
        assert!(!host.available());
        assert_eq!(host.open(&request(1)), Err(CarrierError::Unsupported));
    }

    #[test]
    fn a_helper_that_cannot_start_is_unsupported() {
        let mut host = HelperCarrierHost::new(Some("/definitely/not/quill-webview".into()));
        assert!(host.available());
        assert_eq!(host.open(&request(1)), Err(CarrierError::Unsupported));
    }

    #[test]
    fn a_helper_that_exits_before_ready_is_unsupported() {
        // `true` reads nothing and exits: like a helper whose web view
        // library is missing.
        let mut host = HelperCarrierHost::new(Some("/usr/bin/true".into()));
        assert_eq!(host.open(&request(4)), Ok(()));
        assert_eq!(
            wait_event(&mut host),
            Some((4, HostEvent::Failed(CarrierError::Unsupported)))
        );
    }

    #[test]
    fn a_closed_carrier_reports_no_failure() {
        let mut host = HelperCarrierHost::new(Some("/bin/cat".into()));
        assert_eq!(host.open(&request(5)), Ok(()));
        host.close(5, false);
        std::thread::sleep(Duration::from_millis(300));
        assert!(host.poll().is_empty());
    }
}
