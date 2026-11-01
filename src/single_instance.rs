//! Single running instance per account-data directory (tdesktop's
//! `Core::Sandbox` local-socket handshake, `core/sandbox.cpp`).
//!
//! Two Quill processes on one TDLib database would fight over its lock, so
//! the first process to start owns a local socket (a Unix domain socket in
//! the app data directory, a named pipe on Windows). A later launch connects,
//! forwards its argv, and exits; the owner raises its window and opens any
//! forwarded Telegram links.
//!
//! Wire protocol (one short-lived connection per launch): the client sends
//! the header line `quill-ipc 2`, then one line per forwarded argument, then
//! an empty line. The owner answers `queued` once it holds the arguments,
//! then `ok` when its UI thread beats, or `stuck` after [`UI_ANSWER`]. A
//! connection that does not say `queued` in [`ACK_TIMEOUT`], or says
//! `stuck`, means the owner is hung; the caller refuses to start a second
//! instance rather than risk the database. A `quill-ipc 1` owner rejects
//! the new header, so the client tries the old one, which an owner answers
//! with `ok` alone.
//!
//! The header `quill-agent 1` instead opens a long-lived agent connection
//! (see [`crate::agent::server`]); the rest of this file does not change for
//! it.
//!
//! Stale sockets (the owner crashed): connecting fails with "refused"/"not
//! found", so the new process unlinks the leftover file and binds a fresh
//! one. A file lock serializes that check-and-bind sequence so two racing
//! launches cannot both conclude they are the owner.
#![deny(clippy::indexing_slicing)]

mod heartbeat;

pub use heartbeat::{Heartbeat, UI};

use crate::lang::{keys, tr};
use heartbeat::{Clock, SystemClock};
use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HEADER: &str = "quill-ipc 2";
/// The protocol before the owner checked its UI thread: one `ok`.
const LEGACY_HEADER: &str = "quill-ipc 1";
/// How long a launch waits for the owner to take its arguments.
const ACK_TIMEOUT: Duration = Duration::from_secs(3);
/// How long the owner waits for its UI thread before it says it is stuck:
/// tdesktop's main-thread check (`sandbox.cpp`), so a UI that is only busy
/// is not called frozen.
const UI_ANSWER: Duration = Duration::from_secs(30);
/// Upper bound on one forwarded message.
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;
/// Upper bound on the header line that picks the protocol.
const MAX_HEADER_BYTES: usize = 64;
/// Upper bound on forwarded arguments honored per connection.
const MAX_ARGS: usize = 256;
/// How many launches the owner serves at once; further connections are
/// answered `busy` and closed. A bare close would read as a dead owner, and
/// that launch would take the endpoint over from this live one.
const MAX_CLIENT_THREADS: usize = 8;
/// How long a connected client may stay silent before its thread gives up.
/// Unix sockets only: Windows named pipes have no timeouts, so there the
/// cap above is the only bound.
const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Pause after a failed `accept`, so a persistent error (out of file
/// descriptors) does not turn the listener thread into a busy loop.
const ACCEPT_ERROR_PAUSE: Duration = Duration::from_millis(250);
/// `sun_path` is 104 bytes on macOS (108 on Linux), terminator included.
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 100;

/// Where the owner listens, derived from the app data directory so isolated
/// roots (demos, tests) never collide with the real account.
#[derive(Debug, Clone)]
pub struct Endpoint {
    #[cfg(unix)]
    path: PathBuf,
    #[cfg(windows)]
    pipe: String,
}

#[cfg(any(unix, test))]
fn root_digest(root: &Path) -> String {
    digest_of(&root.to_string_lossy())
}

fn digest_of(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(text.as_bytes());
    digest
        .get(..8)
        .unwrap_or_default()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Endpoint {
    #[cfg(unix)]
    pub fn for_root(root: &Path) -> Self {
        let preferred = root.join("quill.sock");
        let path = if preferred.as_os_str().len() <= MAX_SOCKET_PATH {
            preferred
        } else {
            // Long home directories overflow `sun_path`; the per-user temp
            // dir is short on macOS (`$TMPDIR`) and `/tmp` elsewhere.
            std::env::temp_dir().join(format!("quill-{}.sock", root_digest(root)))
        };
        Self { path }
    }

    #[cfg(windows)]
    pub fn for_root(root: &Path) -> Self {
        let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        Self {
            pipe: format!(
                "quill-{}.ipc",
                digest_of(&windows_root_key(&canonical.to_string_lossy()))
            ),
        }
    }

    fn name(&self) -> io::Result<interprocess::local_socket::Name<'static>> {
        #[cfg(unix)]
        {
            use interprocess::local_socket::GenericFilePath;
            self.path.clone().to_fs_name::<GenericFilePath>()
        }
        #[cfg(windows)]
        {
            use interprocess::local_socket::GenericNamespaced;
            self.pipe.clone().to_ns_name::<GenericNamespaced>()
        }
    }
}

/// Result of [`acquire`].
#[derive(Debug, PartialEq, Eq)]
pub enum Acquired {
    /// This process owns the endpoint; forwarded launches reach the callback.
    Primary,
    /// Another instance is running and now has this launch's arguments.
    Forwarded,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// Handed to the running Quill: exit.
    Forwarded,
    /// This process is the only Quill.
    Primary,
    /// A Quill is running and does not answer: say so and exit.
    OwnerStuck,
    /// The endpoint could not be used; Quill starts anyway.
    Unchecked,
}

impl Launch {
    pub fn after(acquired: &io::Result<Acquired>) -> Self {
        match acquired {
            Ok(Acquired::Forwarded) => Self::Forwarded,
            Ok(Acquired::Primary) => Self::Primary,
            Err(error) if error.kind() == io::ErrorKind::TimedOut => Self::OwnerStuck,
            Err(_) => Self::Unchecked,
        }
    }

    /// The title and body of what this launch must tell the user before it
    /// exits, when `--quit` did not ask it to stay quiet.
    pub fn alert(&self, quit: bool) -> Option<(&'static str, &'static str)> {
        (*self == Self::OwnerStuck && !quit).then(|| {
            (
                tr(keys::Q_STARTUP_OWNER_STUCK_TITLE),
                tr(keys::Q_STARTUP_OWNER_STUCK_BODY),
            )
        })
    }
}

/// Serializes the connect / unlink-stale / bind sequence across processes.
struct AcquireLock {
    #[cfg(unix)]
    _file: std::fs::File,
    #[cfg(windows)]
    mutex: windows_sys::Win32::Foundation::HANDLE,
}

impl AcquireLock {
    #[cfg(unix)]
    fn take(endpoint: &Endpoint) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let mut lock_path = endpoint.path.clone().into_os_string();
        lock_path.push(".lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(PathBuf::from(lock_path))?;
        // SAFETY: the descriptor is owned by `file`, valid for this call.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _file: file })
    }

    #[cfg(windows)]
    fn take(endpoint: &Endpoint) -> io::Result<Self> {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
        let name: Vec<u16> = format!("Local\\{}.lock", endpoint.pipe)
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `name` is a NUL-terminated UTF-16 string alive for the call;
        // a null security descriptor selects the default.
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if mutex.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `mutex` is the valid handle created above.
        let status = unsafe { WaitForSingleObject(mutex, 10_000) };
        if status == WAIT_OBJECT_0 || status == WAIT_ABANDONED {
            return Ok(Self { mutex });
        }
        // SAFETY: closes the handle created above, once; not owned by us.
        unsafe { CloseHandle(mutex) };
        Err(io::Error::other("single-instance lock timed out"))
    }
}

#[cfg(windows)]
impl Drop for AcquireLock {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::ReleaseMutex;
        // SAFETY: `mutex` was acquired on this thread in `take` (the lock is
        // taken and dropped within `acquire`) and is closed exactly once.
        unsafe {
            ReleaseMutex(self.mutex);
            CloseHandle(self.mutex);
        }
    }
}

/// The text a root path is hashed from on Windows: paths compare
/// case-insensitively, take either slash, and canonicalization may add the
/// verbatim `\\?\` prefix, so all of that is folded away first.
#[cfg(any(windows, test))]
fn windows_root_key(path: &str) -> String {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

enum Forward {
    Delivered,
    NoOwner,
    Unresponsive,
    /// The owner holds the arguments; its UI has yet to answer.
    Queued(mpsc::Receiver<io::Result<String>>),
}

/// The owner's answer once its UI did or did not beat.
enum Verdict {
    Delivered,
    Stuck,
    /// The owner went away after it queued the launch.
    Gone,
}

/// Sends `args` to a running owner. `NoOwner` covers every way a connect can
/// fail (nothing listening, stale socket file); only a connection that opens
/// but never answers is `Unresponsive`.
fn forward(endpoint: &Endpoint, args: &[String]) -> Forward {
    match forward_with(endpoint, HEADER, args) {
        // Closed unread: an owner that predates `HEADER`, or a dying one.
        Forward::NoOwner => forward_with(endpoint, LEGACY_HEADER, args),
        sent => sent,
    }
}

fn forward_with(endpoint: &Endpoint, header: &str, args: &[String]) -> Forward {
    let Ok(name) = endpoint.name() else {
        return Forward::NoOwner;
    };
    let Ok(stream) = Stream::connect(name) else {
        return Forward::NoOwner;
    };
    let message = encode_message(header, args);
    let (tx, rx) = mpsc::channel();
    // No thread: `tx` is dropped with the closure and the waits below end.
    let _ = crate::thread_guard::spawn_guarded("quill-single-instance-send", move || {
        let mut stream = stream;
        // An owner over its client cap says `busy` and closes before it
        // reads, so the write can fail with that answer waiting to be read.
        let _ = stream.write_all(message.as_bytes()).and(stream.flush());
        let mut reader = BufReader::new(&stream);
        loop {
            let mut line = String::new();
            let answer = match reader.read_line(&mut line) {
                Ok(0) => Ok(String::new()),
                Ok(_) => Ok(line.trim().to_string()),
                Err(error) => Err(error),
            };
            let last = !matches!(&answer, Ok(line) if line == "queued");
            if tx.send(answer).is_err() || last {
                return;
            }
        }
    });
    match rx.recv_timeout(ACK_TIMEOUT) {
        Ok(Ok(answer)) if answer == "ok" => Forward::Delivered,
        Ok(Ok(answer)) if answer == "queued" => Forward::Queued(rx),
        Ok(Ok(answer)) if answer == "busy" => Forward::Unresponsive,
        // Connected, then dropped or garbled: a dying owner counts as gone.
        Ok(_) => Forward::NoOwner,
        Err(_) => Forward::Unresponsive,
    }
}

/// Waits for the second answer of a queued launch.
fn verdict(answers: &mpsc::Receiver<io::Result<String>>) -> Verdict {
    // The owner's own wait, plus room for it to write the answer.
    match answers.recv_timeout(UI_ANSWER.saturating_add(ACK_TIMEOUT)) {
        Ok(Ok(answer)) if answer == "ok" => Verdict::Delivered,
        Ok(Ok(answer)) if answer == "stuck" => Verdict::Stuck,
        Err(mpsc::RecvTimeoutError::Timeout) => Verdict::Stuck,
        Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => Verdict::Gone,
    }
}

fn encode_message(header: &str, args: &[String]) -> String {
    let mut message = format!("{header}\n");
    for arg in args.iter().take(MAX_ARGS) {
        // One argument per line; embedded newlines cannot split an argument
        // into two (the owner validates each line again regardless).
        message.push_str(&arg.replace(['\n', '\r'], " "));
        message.push('\n');
    }
    message.push('\n');
    message
}

/// Parses one client message; `None` when it is not ours.
#[cfg(test)]
fn decode_message(reader: impl Read) -> Option<Vec<String>> {
    let mut reader = BufReader::new(reader.take(MAX_MESSAGE_BYTES));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.trim_end() != HEADER {
        return None;
    }
    decode_args(reader)
}

/// The forwarded arguments that follow the header line.
fn decode_args(reader: impl Read) -> Option<Vec<String>> {
    let mut reader = BufReader::new(reader.take(MAX_MESSAGE_BYTES));
    let mut line = String::new();
    let mut args = Vec::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let arg = line.trim_end_matches(['\r', '\n']);
        if arg.is_empty() || args.len() >= MAX_ARGS {
            break;
        }
        args.push(arg.to_string());
    }
    Some(args)
}

/// The header line, read a byte at a time so the protocol after it is left
/// unread for whoever handles it.
fn read_header(stream: &Stream) -> Option<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut reader = stream;
    while line.len() < MAX_HEADER_BYTES {
        match reader.read(&mut byte) {
            Ok(1) => {
                let [b] = byte;
                if b == b'\n' {
                    return String::from_utf8(line).ok();
                }
                line.push(b);
            }
            _ => return None,
        }
    }
    None
}

/// Counts the client threads in flight.
#[derive(Default)]
struct ClientSlots(Arc<Mutex<usize>>);

/// One taken slot; freed on drop, including when the thread panics.
struct ClientSlot(Arc<Mutex<usize>>);

impl ClientSlots {
    /// A slot, or `None` when `max` clients are already being served.
    fn try_take(&self, max: usize) -> Option<ClientSlot> {
        let mut count = crate::locks::lock_or_recover(&self.0);
        if *count >= max {
            return None;
        }
        *count = count.saturating_add(1);
        Some(ClientSlot(self.0.clone()))
    }
}

impl Drop for ClientSlot {
    fn drop(&mut self) {
        let mut count = crate::locks::lock_or_recover(&self.0);
        *count = count.saturating_sub(1);
    }
}

/// One end of a connection. On Windows, dropping it first waits, on the
/// dropping thread, until the peer has read everything written to it (or has
/// gone), and only then closes the pipe. Dropped as a bare `Stream`, a pipe
/// with unread data goes to interprocess' linger pool instead, whose one
/// thread flushes such pipes in turn: a peer that is itself waiting for this
/// end to close stalls every later close in the process, and every client
/// waiting on one of those closes hangs. A peer that never reads now holds
/// only the thread that dropped its connection. On Unix the drop does nothing
/// extra.
pub struct Connection(Stream);

impl Connection {
    pub fn new(stream: Stream) -> Self {
        Self(stream)
    }

    pub fn stream(&self) -> &Stream {
        &self.0
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // `&Stream`'s own flush is a no-op; the pipe's flush clears the
        // unread mark that sends a dropped pipe to the linger pool.
        #[cfg(windows)]
        {
            let Stream::NamedPipe(pipe) = &self.0;
            let _ = pipe.inner().flush();
        }
    }
}

/// The UI an owner checks before it acknowledges a launch.
#[derive(Clone, Copy)]
struct Owner {
    ui: &'static Heartbeat,
    clock: &'static dyn Clock,
    answer: Duration,
}

const APP_OWNER: Owner = Owner {
    ui: &UI,
    clock: &SystemClock,
    answer: UI_ANSWER,
};

/// An accepted connection no client thread has taken yet. Dropped untaken
/// (over the cap, or the thread could not start) it says `busy` first.
struct Unserved(Option<Stream>);

impl Unserved {
    fn take(mut self) -> Option<Stream> {
        self.0.take()
    }
}

impl Drop for Unserved {
    fn drop(&mut self) {
        if let Some(stream) = self.0.take() {
            let mut out = &stream;
            let _ = out.write_all(b"busy\n");
            let _ = out.flush();
        }
    }
}

fn serve(
    stream: Unserved,
    slot: ClientSlot,
    on_launch: &(dyn Fn(Vec<String>) + Send + Sync),
    owner: Owner,
) {
    let Some(stream) = stream.take() else {
        return;
    };
    let connection = Connection::new(stream);
    let stream = connection.stream();
    // Best effort: Windows named pipes refuse timeouts.
    let _ = stream.set_recv_timeout(Some(CLIENT_READ_TIMEOUT));
    let Some(header) = read_header(stream) else {
        return;
    };
    let header = header.trim_end_matches('\r');
    if header == crate::agent::server::HEADER {
        // Agent connections stay open; they have a cap of their own.
        drop(slot);
        crate::agent::server::serve(connection);
        return;
    }
    let legacy = header == LEGACY_HEADER;
    if header != HEADER && !legacy {
        return;
    }
    let Some(args) = decode_args(stream) else {
        return;
    };
    let mark = owner.ui.mark();
    let deadline = owner.clock.now().checked_add(owner.answer);
    on_launch(args);
    crate::wake::LINKS.poke();
    let mut out = stream;
    if legacy {
        // That launcher reads one line and gives up after a few seconds.
        let _ = out.write_all(b"ok\n");
        let _ = out.flush();
        return;
    }
    let _ = out.write_all(b"queued\n");
    let _ = out.flush();
    // Closing without an answer would read as a dead owner, and the launch
    // would start a second Quill on this profile.
    let answered =
        deadline.is_none_or(|deadline| owner.ui.answered_by(mark, deadline, owner.clock));
    let _ = out.write_all(if answered { b"ok\n" } else { b"stuck\n" });
    let _ = out.flush();
}

fn not_responding() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "another Quill instance is running but not responding",
    )
}

/// A connection to the instance listening on `endpoint`, for agent clients.
pub fn connect(endpoint: &Endpoint) -> io::Result<Stream> {
    Stream::connect(endpoint.name()?)
}

/// Becomes the owner of `endpoint`, or forwards `args` to the one that is.
///
/// `on_launch` runs on a background thread with the arguments of every
/// later launch; it must only validate and queue (see `deep_link_inbox`).
/// Errors mean the endpoint could not be used at all (callers may continue
/// without single-instance) except [`io::ErrorKind::TimedOut`], which means
/// a live owner is not answering.
pub fn acquire(
    endpoint: &Endpoint,
    args: &[String],
    on_launch: impl Fn(Vec<String>) + Send + Sync + 'static,
) -> io::Result<Acquired> {
    acquire_as(endpoint, args, on_launch, APP_OWNER)
}

fn acquire_as(
    endpoint: &Endpoint,
    args: &[String],
    on_launch: impl Fn(Vec<String>) + Send + Sync + 'static,
    owner: Owner,
) -> io::Result<Acquired> {
    #[cfg(unix)]
    if let Some(parent) = endpoint.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lock = AcquireLock::take(endpoint)?;
    // An owner that queues a launch and then closes, twice over, is broken
    // rather than gone.
    let mut tries = 2u8;
    loop {
        let answers = match forward(endpoint, args) {
            Forward::Delivered => return Ok(Acquired::Forwarded),
            Forward::Unresponsive => return Err(not_responding()),
            Forward::NoOwner => break,
            Forward::Queued(answers) => answers,
        };
        // The owner is alive and holds the launch: later launches need not
        // wait behind its UI for the lock.
        drop(lock);
        match verdict(&answers) {
            Verdict::Delivered => return Ok(Acquired::Forwarded),
            Verdict::Stuck => return Err(not_responding()),
            Verdict::Gone if tries > 1 => lock = AcquireLock::take(endpoint)?,
            Verdict::Gone => return Err(not_responding()),
        }
        tries = tries.saturating_sub(1);
    }
    // Nobody answered: any socket file left behind is a crashed owner's.
    #[cfg(unix)]
    match std::fs::remove_file(&endpoint.path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let options = ListenerOptions::new().name(endpoint.name()?);
    // Owner-only: other users must not inject links or reach agent access.
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        options.security_descriptor(crate::pipe_security::current_user_only()?)
    };
    let listener = match options.create_sync() {
        Ok(listener) => listener,
        Err(e) => {
            // Windows race: another launch created the pipe first.
            return match forward(endpoint, args) {
                Forward::Delivered => Ok(Acquired::Forwarded),
                Forward::Queued(answers) => {
                    drop(lock);
                    match verdict(&answers) {
                        Verdict::Delivered => Ok(Acquired::Forwarded),
                        Verdict::Stuck => Err(not_responding()),
                        Verdict::Gone => Err(e),
                    }
                }
                _ => Err(e),
            };
        }
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Owner-only: other local users must not be able to inject links.
        let _ = std::fs::set_permissions(&endpoint.path, std::fs::Permissions::from_mode(0o600));
    }
    // Agents read this launch's token once the socket can take them.
    crate::agent::access::publish_token();
    let on_launch: Arc<dyn Fn(Vec<String>) + Send + Sync> = Arc::new(on_launch);
    crate::thread_guard::spawn_guarded("quill-single-instance", move || {
        let slots = ClientSlots::default();
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => Unserved(Some(stream)),
                Err(_) => {
                    std::thread::sleep(ACCEPT_ERROR_PAUSE);
                    continue;
                }
            };
            let Some(slot) = slots.try_take(MAX_CLIENT_THREADS) else {
                continue;
            };
            let on_launch = Arc::clone(&on_launch);
            // A client that connects and stalls must not block others.
            let _ = crate::thread_guard::spawn_guarded("quill-single-instance-client", move || {
                serve(stream, slot, &*on_launch, owner)
            });
        }
    })?;
    Ok(Acquired::Primary)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_silent_owner_is_stuck_and_any_other_failure_starts_quill() {
        use std::io::{Error, ErrorKind};
        let after = |result: std::io::Result<Acquired>| Launch::after(&result);
        assert_eq!(
            after(Err(Error::from(ErrorKind::TimedOut))),
            Launch::OwnerStuck
        );
        assert_eq!(after(Ok(Acquired::Forwarded)), Launch::Forwarded);
        assert_eq!(after(Ok(Acquired::Primary)), Launch::Primary);
        assert_eq!(
            after(Err(Error::from(ErrorKind::PermissionDenied))),
            Launch::Unchecked
        );
    }

    #[test]
    fn a_stuck_owner_gets_an_alert_and_every_other_launch_stays_quiet() {
        let (title, body) = Launch::OwnerStuck.alert(false).expect("an alert");
        assert_eq!(title, "Quill isn't responding");
        assert!(body.contains("Force it to quit"), "{body}");
        assert_eq!(Launch::OwnerStuck.alert(true), None, "--quit");
        for launch in [Launch::Forwarded, Launch::Primary, Launch::Unchecked] {
            assert_eq!(launch.alert(false), None);
        }
    }

    #[test]
    fn windows_root_keys_ignore_case_slashes_and_verbatim_prefix() {
        let a = super::windows_root_key(r"C:\Users\Me\AppData\Quill");
        assert_eq!(a, super::windows_root_key(r"c:\users\me\appdata\quill\"));
        assert_eq!(a, super::windows_root_key("c:/users/me/appdata/quill"));
        assert_eq!(a, super::windows_root_key(r"\\?\C:\Users\Me\AppData\Quill"));
        assert_ne!(a, super::windows_root_key(r"C:\Users\Me\AppData\Other"));
    }

    use super::{Acquired, ClientSlots, Endpoint, Launch, acquire, root_digest};
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        crate::scratch::process_dir(&format!("quill-si-{tag}"))
    }

    #[test]
    fn client_slots_cap_concurrent_clients_and_free_on_drop() {
        let slots = ClientSlots::default();
        let held: Vec<_> = (0..3).filter_map(|_| slots.try_take(3)).collect();
        assert_eq!(held.len(), 3);
        assert!(slots.try_take(3).is_none(), "over the cap");
        drop(held);
        assert!(slots.try_take(3).is_some(), "slots come back");
    }

    #[test]
    fn root_digest_is_stable() {
        assert_eq!(root_digest(std::path::Path::new("abc")), "ba7816bf8f01cfea");
    }

    #[test]
    fn second_launch_forwards_args_and_exits() {
        let endpoint = Endpoint::for_root(&temp_root("fwd"));
        let (tx, rx) = mpsc::channel();
        let first = acquire(&endpoint, &[], move |args| {
            let _ = tx.send(args);
        })
        .unwrap();
        assert_eq!(first, Acquired::Primary);
        let args = vec!["tg://resolve?domain=durov".to_string(), "x y".to_string()];
        let second = acquire(&endpoint, &args, |_| panic!("not the owner")).unwrap();
        assert_eq!(second, Acquired::Forwarded);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), args);
        // A bare launch still raises the owner (empty arg list).
        assert_eq!(
            acquire(&endpoint, &[], |_| ()).unwrap(),
            Acquired::Forwarded
        );
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_empty());
    }

    #[test]
    fn racing_launches_elect_exactly_one_owner() {
        let endpoint = Endpoint::for_root(&temp_root("race"));
        let (tx, rx) = mpsc::channel();
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let endpoint = endpoint.clone();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    acquire(
                        &endpoint,
                        &[format!("tg://resolve?domain=u{i}")],
                        move |a| {
                            let _ = tx.send(a);
                        },
                    )
                    .unwrap()
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        assert_eq!(
            results.iter().filter(|r| **r == Acquired::Primary).count(),
            1
        );
        // The owner hears from every forwarded launch (all but its own).
        for _ in 0..7 {
            rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn stale_socket_file_from_a_crash_is_replaced() {
        let endpoint = Endpoint::for_root(&temp_root("stale"));
        {
            // Bind and drop without unlinking: what a crashed owner leaves.
            let _corpse = std::os::unix::net::UnixListener::bind(&endpoint.path).unwrap();
        }
        assert!(endpoint.path.exists());
        assert_eq!(acquire(&endpoint, &[], |_| ()).unwrap(), Acquired::Primary);
    }

    /// A clock that moves only when a test moves it.
    struct FakeClock {
        start: std::time::Instant,
        passed: std::sync::Mutex<Duration>,
    }

    impl super::Clock for FakeClock {
        fn now(&self) -> std::time::Instant {
            self.start + *self.passed.lock().unwrap()
        }
    }

    fn fake_clock() -> &'static FakeClock {
        Box::leak(Box::new(FakeClock {
            start: std::time::Instant::now(),
            passed: std::sync::Mutex::new(Duration::ZERO),
        }))
    }

    fn heartbeat() -> &'static super::Heartbeat {
        Box::leak(Box::new(super::Heartbeat::new()))
    }

    /// A second launch connected to an owner that serves it on a thread.
    struct Launched {
        reader: std::io::BufReader<interprocess::local_socket::Stream>,
        owner: std::thread::JoinHandle<()>,
    }

    impl Launched {
        fn line(&mut self) -> String {
            use std::io::BufRead;
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            line
        }

        fn rest(mut self) -> String {
            use std::io::Read;
            let mut rest = String::new();
            self.reader.read_to_string(&mut rest).unwrap();
            self.owner.join().unwrap();
            rest
        }
    }

    fn launch(
        tag: &str,
        header: &str,
        owner: super::Owner,
        on_launch: fn(&super::Heartbeat),
    ) -> Launched {
        use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
        use std::io::Write;
        let endpoint = Endpoint::for_root(&temp_root(tag));
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint.path);
        let listener = ListenerOptions::new()
            .name(endpoint.name().unwrap())
            .create_sync()
            .unwrap();
        let owner = std::thread::spawn(move || {
            let stream = listener.accept().unwrap();
            let slot = ClientSlots::default().try_take(1).unwrap();
            let ui = owner.ui;
            super::serve(
                super::Unserved(Some(stream)),
                slot,
                &move |_| on_launch(ui),
                owner,
            );
        });
        let mut client = Stream::connect(endpoint.name().unwrap()).unwrap();
        client
            .write_all(super::encode_message(header, &[]).as_bytes())
            .unwrap();
        Launched {
            reader: std::io::BufReader::new(client),
            owner,
        }
    }

    fn owner(ui: &'static super::Heartbeat, clock: &'static FakeClock) -> super::Owner {
        super::Owner {
            ui,
            clock,
            answer: super::UI_ANSWER,
        }
    }

    #[test]
    fn an_owner_whose_window_answers_at_29_seconds_acknowledges_the_launch() {
        let (ui, clock) = (heartbeat(), fake_clock());
        ui.start();
        let mut launched = launch("alive", super::HEADER, owner(ui, clock), |_| ());
        assert_eq!(launched.line(), "queued\n");
        *clock.passed.lock().unwrap() = Duration::from_secs(29);
        ui.recheck();
        ui.beat();
        assert_eq!(launched.rest(), "ok\n");
    }

    #[test]
    fn an_owner_whose_window_is_silent_for_30_seconds_says_it_is_stuck() {
        let (ui, clock) = (heartbeat(), fake_clock());
        ui.start();
        let mut launched = launch("frozen", super::HEADER, owner(ui, clock), |_| ());
        assert_eq!(launched.line(), "queued\n");
        *clock.passed.lock().unwrap() = Duration::from_secs(30);
        ui.recheck();
        assert_eq!(launched.rest(), "stuck\n");
    }

    #[test]
    fn an_owner_whose_window_beats_while_taking_the_launch_answers_at_once() {
        let (ui, clock) = (heartbeat(), fake_clock());
        ui.start();
        let launched = launch("beats", super::HEADER, owner(ui, clock), |ui| ui.beat());
        assert_eq!(launched.rest(), "queued\nok\n");
    }

    #[test]
    fn an_owner_with_no_window_yet_or_any_more_acknowledges_at_once() {
        let (ui, clock) = (heartbeat(), fake_clock());
        let starting = launch("starting", super::HEADER, owner(ui, clock), |_| ());
        assert_eq!(starting.rest(), "queued\nok\n");
        ui.start();
        ui.stop();
        let closed = launch("closed", super::HEADER, owner(ui, clock), |_| ());
        assert_eq!(closed.rest(), "queued\nok\n");
    }

    #[test]
    fn a_launch_against_a_frozen_owner_is_told_so_and_does_not_take_over() {
        let ui = heartbeat();
        ui.start();
        let owner = super::Owner {
            ui,
            clock: &super::SystemClock,
            answer: Duration::ZERO,
        };
        let endpoint = Endpoint::for_root(&temp_root("frozen-acquire"));
        let first = super::acquire_as(&endpoint, &[], |_| (), owner).unwrap();
        assert_eq!(first, Acquired::Primary);
        let second = acquire(&endpoint, &[], |_| panic!("not the owner"));
        assert_eq!(
            second.map_err(|error| error.kind()),
            Err(std::io::ErrorKind::TimedOut)
        );
        assert_eq!(
            Launch::after(&acquire(&endpoint, &[], |_| ())),
            Launch::OwnerStuck
        );
    }

    #[test]
    fn a_launch_past_the_client_cap_is_told_the_owner_is_busy_and_does_not_take_over() {
        use interprocess::local_socket::{Stream, prelude::*};
        use std::io::{BufRead, Write};
        let (ui, clock) = (heartbeat(), fake_clock());
        ui.start();
        let endpoint = Endpoint::for_root(&temp_root("over-cap"));
        let first = super::acquire_as(&endpoint, &[], |_| (), owner(ui, clock)).unwrap();
        assert_eq!(first, Acquired::Primary);
        // Launches the frozen window holds: each keeps its client thread.
        let held: Vec<_> = (0..super::MAX_CLIENT_THREADS)
            .map(|_| {
                let mut client = Stream::connect(endpoint.name().unwrap()).unwrap();
                client
                    .write_all(super::encode_message(super::HEADER, &[]).as_bytes())
                    .unwrap();
                let mut reader = std::io::BufReader::new(client);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                assert_eq!(line, "queued\n");
                reader
            })
            .collect();
        let over = acquire(&endpoint, &[], |_| panic!("not the owner"));
        assert_eq!(
            over.map_err(|error| error.kind()),
            Err(std::io::ErrorKind::TimedOut)
        );
        ui.stop();
        drop(held);
    }

    #[test]
    fn an_owner_that_keeps_closing_after_it_queued_is_called_stuck() {
        use interprocess::local_socket::{ListenerOptions, prelude::*};
        use std::io::Write;
        let endpoint = Endpoint::for_root(&temp_root("queue-and-close"));
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint.path);
        let listener = ListenerOptions::new()
            .name(endpoint.name().unwrap())
            .create_sync()
            .unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                let _ = super::read_header(&stream);
                let _ = super::decode_args(&stream);
                let mut out = &stream;
                let _ = out.write_all(b"queued\n");
            }
        });
        let launched = acquire(&endpoint, &[], |_| panic!("not the owner"));
        assert_eq!(
            launched.map_err(|error| error.kind()),
            Err(std::io::ErrorKind::TimedOut)
        );
    }

    #[test]
    fn a_launch_from_an_older_quill_gets_its_single_ok() {
        let (ui, clock) = (heartbeat(), fake_clock());
        ui.start();
        let launched = launch(
            "legacy-launch",
            super::LEGACY_HEADER,
            owner(ui, clock),
            |_| (),
        );
        assert_eq!(launched.rest(), "ok\n");
    }

    #[test]
    fn a_launch_reaches_an_older_quill_that_answers_ok_alone() {
        use interprocess::local_socket::{ListenerOptions, prelude::*};
        use std::io::Write;
        let endpoint = Endpoint::for_root(&temp_root("legacy-owner"));
        #[cfg(unix)]
        let _ = std::fs::remove_file(&endpoint.path);
        let listener = ListenerOptions::new()
            .name(endpoint.name().unwrap())
            .create_sync()
            .unwrap();
        let (tx, rx) = mpsc::channel();
        // An owner from before `quill-ipc 2`: it drops any other header.
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let stream = stream.unwrap();
                if super::read_header(&stream).as_deref() != Some(super::LEGACY_HEADER) {
                    continue;
                }
                let _ = tx.send(super::decode_args(&stream).unwrap());
                let mut out = &stream;
                out.write_all(b"ok\n").unwrap();
            }
        });
        let args = vec!["tg://resolve?domain=durov".to_string()];
        assert_eq!(
            acquire(&endpoint, &args, |_| panic!("not the owner")).unwrap(),
            Acquired::Forwarded
        );
        assert_eq!(rx.recv().unwrap(), args);
    }

    #[test]
    fn foreign_clients_are_ignored() {
        use super::decode_message;
        assert_eq!(decode_message(&b"GET / HTTP/1.1\r\n\r\n"[..]), None);
        assert_eq!(
            decode_message(&b"quill-ipc 2\ntg://a\n\nignored\n"[..]),
            Some(vec!["tg://a".to_string()])
        );
    }
}
