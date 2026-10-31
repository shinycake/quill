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
    CLOSED_HELPER_DEADLINE, CarrierCommand, CarrierEvent, CarrierFailure, MAX_LINE_BYTES,
    MAX_PENDING_BYTES, SecretUrl, base64_len,
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
/// How long a closed helper gets to tell the page, drop its view and (the
/// last one, on Windows) wait for WebView2's browser process before it is
/// killed. The order of the close deadlines is checked where they are
/// defined (the protocol crate's `carrier.rs`).
const CLOSE_GRACE: Duration = CLOSED_HELPER_DEADLINE;
/// The most [`CarrierHost::shutdown`] waits for helpers still exiting, in
/// all. Shutdown tells every helper it is the last, so each may wait for
/// the shared browser process as a last close does: the same budget.
const SHUTDOWN_WAIT: Duration = CLOSE_GRACE;
/// How long a reader waits for the exit status of a helper whose output
/// ended, to tell a missing library from a crash.
const EXIT_WAIT: Duration = Duration::from_millis(500);
/// How long a host that could not claim a data folder waits before it
/// tries again; meanwhile every open is refused.
const CLAIM_BACKOFF: Duration = Duration::from_secs(10);
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
    /// This account's folder for the web views' files. Each host uses a
    /// folder of its own in there and removes it when it stops
    /// ([`data_folder`]).
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
    /// The account's folder the generations live in.
    root: Option<PathBuf>,
    /// This host's data folder, until it stops.
    generation: Option<data_folder::Generation>,
    /// No claim before this, after one failed.
    claim_after: Option<Instant>,
    relay: RelaySink,
    capability: Capability,
    carriers: HashMap<i32, Running>,
    failures_tx: Sender<(i32, CarrierError)>,
    failures: Receiver<(i32, CarrierError)>,
    /// Closed helpers still on their way out; each hears once its helper
    /// is gone.
    exiting: Vec<Receiver<()>>,
    /// [`CLOSE_GRACE`]; tests set their own.
    close_grace: Duration,
    /// [`SHUTDOWN_WAIT`]; tests set their own.
    shutdown_wait: Duration,
}

/// What [`HelperCarrierHost::stop`]'s wait for exiting helpers came to.
#[derive(Debug, PartialEq, Eq)]
enum Waited {
    /// None was exiting.
    NotAtAll,
    /// Each was gone before the ceiling.
    UntilAllGone,
    /// The ceiling came first, or the caller would not wait.
    GaveUp,
}

impl HelperCarrierHost {
    pub fn new(config: HelperConfig) -> Self {
        let generation = config.data_dir.as_deref().and_then(|root| {
            let generation = data_folder::Generation::claim(root);
            // What earlier hosts left (they may have been killed).
            data_folder::sweep_in_background(root.to_path_buf());
            generation
        });
        if let (Some(program), Some(cache)) = (&config.program, &config.probe_cache) {
            start_probe(program.clone(), cache.clone(), config.capability.clone());
        }
        let (failures_tx, failures) = channel();
        Self {
            program: config.program,
            claim_after: (config.data_dir.is_some() && generation.is_none())
                .then(|| Instant::now() + CLAIM_BACKOFF),
            root: config.data_dir,
            generation,
            relay: config.relay,
            capability: config.capability,
            carriers: HashMap::new(),
            failures_tx,
            failures,
            exiting: Vec::new(),
            close_grace: CLOSE_GRACE,
            shutdown_wait: SHUTDOWN_WAIT,
        }
    }

    /// The folder a new helper gets. With an account folder configured a
    /// helper never runs without one of its own: a claim that failed is
    /// tried again, at most every [`CLAIM_BACKOFF`], and until one works no
    /// carrier opens (TDLib may try again).
    fn data_dir_arg(&mut self) -> Result<Option<String>, CarrierError> {
        let Some(root) = &self.root else {
            return Ok(None);
        };
        let now = Instant::now();
        if self.generation.is_none() && self.claim_after.is_none_or(|after| now >= after) {
            self.generation = data_folder::Generation::claim(root);
            self.claim_after = self.generation.is_none().then(|| now + CLAIM_BACKOFF);
        }
        self.generation
            .as_ref()
            .and_then(|generation| generation.dir().to_str())
            .map(|dir| Some(dir.to_string()))
            .ok_or(CarrierError::Terminated)
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
        let data_dir = self.data_dir_arg()?;
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
            data_dir,
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
            let last = self.carriers.is_empty();
            let _ = running
                .commands
                .try_send(CarrierCommand::Close { notify_page, last });
            // It goes by itself after telling the page; if it does not (a
            // stuck loop, a writer stuck on a full pipe), it is killed.
            if let Some(child) = running.shared.take_child() {
                self.exiting.push(crate::child_reap::reap_after_notice(
                    child,
                    self.close_grace,
                ));
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
        self.exiting.retain(still_exiting);
        self.failures.try_iter().collect()
    }

    /// Closes every helper and waits, at most [`SHUTDOWN_WAIT`] in all, for
    /// the ones still exiting (on Windows a helper waits for WebView2's
    /// browser process before it goes), then removes this host's data
    /// folder, retrying for a while. Blocks: the driver runs it on a
    /// thread of its own.
    fn shutdown(&mut self) {
        self.stop(true);
    }
}

impl HelperCarrierHost {
    fn stop(&mut self, wait: bool) -> Waited {
        let deadline = Instant::now() + self.shutdown_wait;
        for (_, running) in self.carriers.drain() {
            running.shared.closing.store(true, Ordering::Release);
            if !wait {
                running.shared.kill();
                continue;
            }
            let _ = running.commands.try_send(CarrierCommand::Close {
                notify_page: false,
                last: true,
            });
            if let Some(child) = running.shared.take_child() {
                self.exiting.push(crate::child_reap::reap_after_notice(
                    child,
                    self.shutdown_wait,
                ));
            }
        }
        let mut waited = if self.exiting.is_empty() {
            Waited::NotAtAll
        } else {
            Waited::UntilAllGone
        };
        for exiting in self.exiting.drain(..) {
            let left = deadline.saturating_duration_since(Instant::now());
            if !wait || left.is_zero() {
                waited = Waited::GaveUp;
                break;
            }
            if let Err(RecvTimeoutError::Timeout) = exiting.recv_timeout(left) {
                waited = Waited::GaveUp;
            }
        }
        if let Some(generation) = self.generation.take() {
            if wait {
                generation.remove(data_folder::RETRY_FOR);
            } else {
                generation.remove_in_background(data_folder::RETRY_FOR);
            }
        }
        waited
    }

    /// Whether helpers are still running or on their way out.
    #[cfg(test)]
    fn exiting(&self) -> bool {
        !self.carriers.is_empty() || self.exiting.iter().any(still_exiting)
    }

    /// This host's data folder.
    #[cfg(test)]
    fn data_folder(&self) -> Option<&Path> {
        self.generation.as_ref().map(data_folder::Generation::dir)
    }
}

/// Whether the helper behind `notice` has not gone yet.
fn still_exiting(notice: &Receiver<()>) -> bool {
    matches!(notice.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty))
}

impl Drop for HelperCarrierHost {
    /// Never waits: a host dropped without `shutdown` (a CLI tool, a
    /// thread that could not start) kills its helpers and leaves the rest
    /// to the data folder's fallbacks.
    fn drop(&mut self) {
        self.stop(false);
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
            Ok(close @ CarrierCommand::Close { .. }) => {
                let _ = write_line(&mut stdin, &close);
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

mod data_folder;

#[cfg(test)]
#[cfg(unix)]
mod tests;
