//! The live [`CarrierHost`]: one `quill-webview --carrier` process per
//! carrier (`crates/webview-host/src/carrier`).
//!
//! Each carrier gets a writer thread (its stdin) and a reader thread (its
//! stdout); the child itself stays with the host, so every way out (a
//! failure, TDLib's close, logout, quit, dropping the host) can kill it
//! from the caller's thread without waiting on either. Data to the page
//! goes through a bounded queue whose byte count is capped. What the page
//! posts goes from the reader thread straight to TDLib ([`RelaySink`]),
//! never through the UI thread. A helper that stops reading, stops
//! answering or dies becomes a failure, classified as the schema wants:
//! `Unsupported` only when this system cannot run a web view at all (which
//! also turns hosting off for the whole app, see [`Capability`]), else
//! `Terminated` so TDLib tries again.

use super::{
    CARRIER_RELAY_EXTRA, CarrierError, CarrierHost, CarrierMessage, CarrierRequest, RelaySink,
};
use crate::media_tools::NoConsole;
use crate::telegram::requests::process_web_proxy_carrier_message;
use quill_webview_protocol::carrier::{
    CarrierCommand, CarrierEvent, CarrierFailure, MAX_LINE_BYTES, MAX_PENDING_BYTES, SecretUrl,
    base64_len,
};
use quill_webview_protocol::lines::{Line, read_bounded_line};
use quill_webview_protocol::{decode, encode};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Commands that may wait for one carrier's writer thread.
const QUEUE_LEN: usize = 1024;
/// How long a helper may take to build its view and lock it down before
/// it says `ready` (a first WebView2 start can take a while).
const START_TIMEOUT: Duration = Duration::from_secs(60);
/// After `ready` the helper sends `alive` every 3 s; this much silence
/// means its loop is stuck (or the whole process is), whatever the page
/// does.
const HELPER_SILENCE: Duration = Duration::from_secs(15);
/// How often a writer thread looks at the clocks when nothing is queued.
const WATCH_TICK: Duration = Duration::from_secs(1);
/// How long a closed helper gets to send the page `{"t":"close"}` and go
/// before it is killed.
const CLOSE_GRACE: Duration = Duration::from_secs(1);
/// How long a reader waits for the exit status of a helper whose output
/// ended, to tell a missing library from a crash.
const EXIT_WAIT: Duration = Duration::from_millis(500);
/// The capability check's whole budget.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

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

// Unknown is 0, the default.
const YES: u8 = 1;
const NO: u8 = 2;

/// Whether this system can run a carrier at all, shared by every host in
/// the process (all accounts): unknown until the capability check or a
/// carrier says. Only "no" stops hosting.
#[derive(Clone, Debug, Default)]
pub struct Capability(Arc<AtomicU8>);

impl Capability {
    /// The app's one capability.
    pub fn app() -> Self {
        static APP: OnceLock<Capability> = OnceLock::new();
        APP.get_or_init(Capability::default).clone()
    }

    pub fn ruled_out(&self) -> bool {
        self.0.load(Ordering::Acquire) == NO
    }

    pub fn confirmed(&self) -> bool {
        self.0.load(Ordering::Acquire) == YES
    }

    fn set(&self, can: bool) {
        self.0.store(if can { YES } else { NO }, Ordering::Release);
    }
}

/// Whether an exit before `ready` means the web view library is missing:
/// the dynamic loader's 127 on Unix, a DLL that is not there or would not
/// load on Windows.
fn loader_failure(status: ExitStatus) -> bool {
    #[cfg(windows)]
    {
        // STATUS_DLL_NOT_FOUND, STATUS_ENTRYPOINT_NOT_FOUND,
        // STATUS_DLL_INIT_FAILED.
        status
            .code()
            .map(|code| code as u32)
            .is_some_and(|code| matches!(code, 0xC000_0135 | 0xC000_0139 | 0xC000_0142))
    }
    #[cfg(not(windows))]
    {
        status.code() == Some(127)
    }
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
    /// Quill closed it: the exit that follows is not a failure, and the
    /// page's last words are not passed on.
    closing: AtomicBool,
    /// The helper said `ready`.
    ready: AtomicBool,
    /// The helper process, until something ends it.
    child: Mutex<Option<Child>>,
}

impl Shared {
    fn new(child: Child) -> Self {
        Self {
            started: Instant::now(),
            last_heard_ms: AtomicU64::new(0),
            queued: AtomicUsize::new(0),
            failed: AtomicBool::new(false),
            closing: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            child: Mutex::new(Some(child)),
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

    /// Whether the helper is past its start budget, or silent too long
    /// once started.
    fn overdue(&self) -> bool {
        if !self.ready.load(Ordering::Acquire) {
            return self.started.elapsed() > START_TIMEOUT;
        }
        let heard = Duration::from_millis(self.last_heard_ms.load(Ordering::Acquire));
        self.started.elapsed().saturating_sub(heard) > HELPER_SILENCE
    }

    fn take_child(&self) -> Option<Child> {
        self.child.lock().ok().and_then(|mut child| child.take())
    }

    /// End the helper now (reaped on a detached thread).
    fn kill(&self) {
        if let Some(child) = self.take_child() {
            crate::child_reap::kill_and_reap(child);
        }
    }

    /// The exit status of a helper whose output ended, waiting up to
    /// `wait` for it; `None` if it was ended here or is still going.
    fn exit_status(&self, wait: Duration) -> Option<ExitStatus> {
        let deadline = Instant::now() + wait;
        loop {
            {
                let mut slot = self.child.lock().ok()?;
                let status = slot.as_mut()?.try_wait().ok().flatten();
                if status.is_some() {
                    slot.take();
                    return status;
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Report `error` unless something was reported already or Quill is
    /// closing the carrier itself.
    fn fail(&self, carrier_id: i32, error: CarrierError, failures: &Sender<(i32, CarrierError)>) {
        if self.closing.load(Ordering::Acquire) || self.failed.swap(true, Ordering::AcqRel) {
            return;
        }
        let _ = failures.send((carrier_id, error));
        crate::wake::UPDATES.poke();
    }
}

struct Running {
    commands: SyncSender<CarrierCommand>,
    shared: Arc<Shared>,
}

/// How a [`HelperCarrierHost`] runs its helpers.
pub struct HelperConfig {
    /// The helper; `None` hosts nothing.
    pub program: Option<PathBuf>,
    /// This account's folder for the web views' files, emptied when the
    /// host starts and stops.
    pub data_dir: Option<PathBuf>,
    /// Where the capability check keeps its answer for this helper build;
    /// `None` runs no check.
    pub probe_cache: Option<PathBuf>,
    /// The page's messages to TDLib.
    pub relay: RelaySink,
    pub capability: Capability,
}

pub struct HelperCarrierHost {
    program: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    relay: RelaySink,
    capability: Capability,
    carriers: HashMap<i32, Running>,
    failures_tx: Sender<(i32, CarrierError)>,
    failures: Receiver<(i32, CarrierError)>,
}

impl HelperCarrierHost {
    pub fn new(config: HelperConfig) -> Self {
        if let Some(dir) = &config.data_dir {
            // Whatever the last run left (it may have been killed).
            let _ = std::fs::remove_dir_all(dir);
        }
        if let (Some(program), Some(cache)) = (&config.program, &config.probe_cache) {
            start_probe(program.clone(), cache.clone(), config.capability.clone());
        }
        let (failures_tx, failures) = channel();
        Self {
            program: config.program,
            data_dir: config.data_dir,
            relay: config.relay,
            capability: config.capability,
            carriers: HashMap::new(),
            failures_tx,
            failures,
        }
    }

    fn data_dir_arg(&self) -> Option<String> {
        let dir = self.data_dir.as_ref()?;
        let _ = std::fs::create_dir_all(dir);
        dir.to_str().map(String::from)
    }
}

fn spawn(program: &Path) -> std::io::Result<Child> {
    Command::new(program)
        .arg("--carrier")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .no_console()
        .spawn()
}

impl CarrierHost for HelperCarrierHost {
    fn available(&self) -> bool {
        self.program.is_some() && !self.capability.ruled_out()
    }

    /// Must run on a thread that lives as long as the carrier: on Linux the
    /// helper asks the kernel to kill it when the thread that started it
    /// ends (`PR_SET_PDEATHSIG`, `crates/webview-host/src/carrier/watchdog.rs`).
    /// Today that is always the UI thread (the poll loops and `ingest`); a
    /// short-lived or pooled thread here would kill every carrier it starts.
    fn open(&mut self, request: &CarrierRequest) -> Result<(), CarrierError> {
        if !self.available() {
            return Err(CarrierError::Unsupported);
        }
        let program = self.program.as_ref().ok_or(CarrierError::Unsupported)?;
        // Not starting is this process's trouble (no handles, no memory),
        // not proof the system has no web view: TDLib may try again.
        let mut child = spawn(program).map_err(|_| CarrierError::Terminated)?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            crate::child_reap::kill_and_reap(child);
            return Err(CarrierError::Terminated);
        };
        let carrier_id = request.id;
        let shared = Arc::new(Shared::new(child));
        let (commands, queue) = std::sync::mpsc::sync_channel(QUEUE_LEN);
        let open = CarrierCommand::Open {
            url: SecretUrl::new(request.url.expose().to_string()),
            origin: request.origin.clone(),
            path_prefix: request.path_prefix.clone(),
            bridge_object_name: request.bridge_object_name.clone(),
            data_dir: self.data_dir_arg(),
        };
        let writer = {
            let shared = shared.clone();
            let failures = self.failures_tx.clone();
            crate::thread_guard::spawn_guarded("quill-carrier-writer", move || {
                write_commands(carrier_id, stdin, open, queue, &shared, &failures)
            })
        };
        let reader = {
            let shared = shared.clone();
            let failures = self.failures_tx.clone();
            let relay = self.relay.clone();
            let capability = self.capability.clone();
            crate::thread_guard::spawn_guarded("quill-carrier-reader", move || {
                read_events(carrier_id, stdout, &shared, &failures, &relay, &capability)
            })
        };
        if writer.is_err() || reader.is_err() {
            shared.kill();
            return Err(CarrierError::Terminated);
        }
        self.carriers
            .insert(carrier_id, Running { commands, shared });
        Ok(())
    }

    fn deliver(&mut self, carrier_id: i32, data: String) {
        let Some(running) = self.carriers.get(&carrier_id) else {
            // The table says it is live but no helper runs it: never drop
            // TDLib's bytes silently.
            let _ = self
                .failures_tx
                .send((carrier_id, CarrierError::Terminated));
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
            // The helper is not keeping up.
            running
                .shared
                .fail(carrier_id, CarrierError::Terminated, &self.failures_tx);
            running.shared.kill();
            self.carriers.remove(&carrier_id);
        }
    }

    fn close(&mut self, carrier_id: i32, notify_page: bool) {
        if let Some(running) = self.carriers.remove(&carrier_id) {
            running.shared.closing.store(true, Ordering::Release);
            let _ = running
                .commands
                .try_send(CarrierCommand::Close { notify_page });
            // It goes by itself after telling the page; if it does not (a
            // stuck loop, a writer stuck on a full pipe), it is killed.
            if let Some(child) = running.shared.take_child() {
                crate::child_reap::reap_after(child, CLOSE_GRACE);
            }
        }
    }

    fn poll(&mut self) -> Vec<(i32, CarrierError)> {
        // The watchdog: a helper that went quiet is ended here, whatever
        // its threads are blocked on.
        let mut overdue = Vec::new();
        for (carrier_id, running) in &self.carriers {
            if running.shared.overdue() {
                running
                    .shared
                    .fail(*carrier_id, CarrierError::Terminated, &self.failures_tx);
                running.shared.kill();
                overdue.push(*carrier_id);
            }
        }
        for carrier_id in overdue {
            self.carriers.remove(&carrier_id);
        }
        self.failures.try_iter().collect()
    }

    fn shutdown(&mut self) {
        for (_, running) in self.carriers.drain() {
            running.shared.closing.store(true, Ordering::Release);
            running.shared.kill();
        }
        if let Some(dir) = &self.data_dir {
            // Best effort: a web view still exiting may hold files, and the
            // next start empties the folder anyway.
            let _ = std::fs::remove_dir_all(dir);
            if dir.exists() && std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
                for line in leftovers(dir, 50) {
                    eprintln!("carrier data left: {line}");
                }
            }
        }
    }
}

/// Up to `limit` entries still under `dir`, as relative paths with sizes,
/// for the debug trace of a folder that would not go away.
fn leftovers(dir: &Path, limit: usize) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            if found.len() >= limit {
                return found;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => {
                    found.push(format!("{relative}/"));
                    pending.push(path);
                }
                Ok(meta) => found.push(format!("{relative} ({} bytes)", meta.len())),
                Err(_) => found.push(format!("{relative} (unreadable)")),
            }
        }
    }
    found
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

/// One carrier's writer: sends `open`, then the queue. A write that fails
/// means the helper is gone, which its reader reports.
fn write_commands(
    carrier_id: i32,
    mut stdin: ChildStdin,
    open: CarrierCommand,
    queue: Receiver<CarrierCommand>,
    shared: &Shared,
    failures: &Sender<(i32, CarrierError)>,
) {
    shared.heard();
    if !write_line(&mut stdin, &open) {
        return;
    }
    drop(open);
    loop {
        // Checked on every turn, not only when idle: steady deliveries
        // must not hide a helper that stopped answering.
        if shared.overdue() {
            shared.fail(carrier_id, CarrierError::Terminated, failures);
            shared.kill();
            return;
        }
        match queue.recv_timeout(WATCH_TICK) {
            Ok(CarrierCommand::Close { notify_page }) => {
                let _ = write_line(&mut stdin, &CarrierCommand::Close { notify_page });
                return;
            }
            Ok(command) => {
                if let CarrierCommand::Deliver { data } = &command {
                    shared.dequeued(data.len());
                }
                if !write_line(&mut stdin, &command) {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The host dropped the carrier (failure, shutdown, quit).
            Err(RecvTimeoutError::Disconnected) => {
                shared.kill();
                return;
            }
        }
    }
}

/// One carrier's reader: the helper's lines until it exits. The page's
/// messages go straight to TDLib.
fn read_events(
    carrier_id: i32,
    stdout: std::process::ChildStdout,
    shared: &Shared,
    failures: &Sender<(i32, CarrierError)>,
    relay: &RelaySink,
    capability: &Capability,
) {
    let mut reader = std::io::BufReader::new(stdout);
    loop {
        let line = match read_bounded_line(&mut reader, MAX_LINE_BYTES) {
            Ok(Line::Text(line)) => line,
            // A line over the cap is a broken relay, not something to skip.
            Ok(Line::TooLong) => {
                shared.fail(carrier_id, CarrierError::Terminated, failures);
                shared.kill();
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
                if failure == CarrierFailure::Unsupported {
                    capability.set(false);
                }
                shared.fail(carrier_id, carrier_error(failure), failures);
                return;
            }
        };
        // After TDLib's close or a failure the page is not heard (the
        // schema's "must be ignored").
        if shared.failed.load(Ordering::Acquire) || shared.closing.load(Ordering::Acquire) {
            continue;
        }
        let request = process_web_proxy_carrier_message(CARRIER_RELAY_EXTRA, carrier_id, &message);
        if !relay(&request) {
            return;
        }
    }
    // The helper exited without saying why. Only a missing web view
    // library is `Unsupported` (TDLib then stops asking); a crash, a kill
    // or anything else is `Terminated`, and TDLib tries again.
    let missing_library = !shared.ready.load(Ordering::Acquire)
        && shared.exit_status(EXIT_WAIT).is_some_and(loader_failure);
    let error = if missing_library {
        capability.set(false);
        CarrierError::Unsupported
    } else {
        CarrierError::Terminated
    };
    shared.fail(carrier_id, error, failures);
}

fn carrier_error(failure: CarrierFailure) -> CarrierError {
    match failure {
        CarrierFailure::Unsupported => CarrierError::Unsupported,
        CarrierFailure::LoadFailed { message } => CarrierError::LoadFailed(message),
        CarrierFailure::Terminated => CarrierError::Terminated,
    }
}

/// The capability check, once per app run and on a thread of its own:
/// start the helper as a carrier would (`probe`) and see whether it gets
/// to `ready`. A pass is kept in `cache` for this helper build, so later
/// runs skip the check; a fail is not kept, so installing the missing
/// library is noticed on the next run.
fn start_probe(program: PathBuf, cache: PathBuf, capability: Capability) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    let _ = crate::thread_guard::spawn_guarded("quill-carrier-probe", move || {
        let build = helper_build(&program);
        let passed = build
            .as_ref()
            .is_some_and(|build| std::fs::read_to_string(&cache).ok().as_deref() == Some(build));
        if passed {
            capability.set(true);
            return;
        }
        let data_dir = cache.with_extension("data");
        let verdict = probe(&program, &data_dir);
        let _ = std::fs::remove_dir_all(&data_dir);
        match verdict {
            Some(true) => {
                capability.set(true);
                if let Some(build) = build {
                    let _ = std::fs::write(&cache, build);
                }
            }
            Some(false) => {
                capability.set(false);
                let _ = std::fs::remove_file(&cache);
            }
            None => {}
        }
    });
}

/// Names one build of the helper: its size and modification time.
fn helper_build(program: &Path) -> Option<String> {
    let meta = std::fs::metadata(program).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!("ok {} {}", meta.len(), modified.as_secs()))
}

/// `Some(true)` when the helper got to `ready`, `Some(false)` when it
/// showed it cannot run here, `None` when it could not tell.
pub(super) fn probe(program: &Path, data_dir: &Path) -> Option<bool> {
    let mut child = spawn(program).ok()?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        crate::child_reap::kill_and_reap(child);
        return None;
    };
    let _ = std::fs::create_dir_all(data_dir);
    let command = CarrierCommand::Probe {
        data_dir: data_dir.to_str().map(String::from),
    };
    let (lines_tx, lines) = channel();
    let reader = crate::thread_guard::spawn_guarded("quill-carrier-probe-reader", move || {
        let mut reader = std::io::BufReader::new(stdout);
        while let Ok(Line::Text(line)) = read_bounded_line(&mut reader, MAX_LINE_BYTES) {
            let event = std::str::from_utf8(&line)
                .ok()
                .and_then(|line| decode::<CarrierEvent>(line).ok());
            if let Some(event) = event
                && lines_tx.send(event).is_err()
            {
                return;
            }
        }
    });
    if reader.is_err() || !write_line(&mut stdin, &command) {
        drop(stdin);
        return finish_probe(child, None);
    }
    let deadline = Instant::now() + PROBE_TIMEOUT;
    let verdict = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(CarrierEvent::Ready) => break Some(true),
            Ok(CarrierEvent::Failed {
                failure: CarrierFailure::Unsupported,
            }) => break Some(false),
            Ok(CarrierEvent::Failed { .. }) => break None,
            Ok(_) => {}
            // Its output ended: what the exit says decides.
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) => break None,
        }
    };
    drop(stdin);
    finish_probe(child, verdict)
}

/// Reap the probe's helper; an exit that shows a missing library turns an
/// undecided probe into a fail.
fn finish_probe(mut child: Child, verdict: Option<bool>) -> Option<bool> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    verdict.or_else(|| {
        status
            .filter(|status| loader_failure(*status))
            .map(|_| false)
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests;
