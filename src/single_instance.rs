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
//! the header line `quill-ipc 1`, then one line per forwarded argument, then
//! an empty line; the owner answers `ok`. A connection that does not answer
//! in [`ACK_TIMEOUT`] means the owner is hung; the caller refuses to start
//! a second instance rather than risk the database.
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

use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HEADER: &str = "quill-ipc 1";
/// How long a launch waits for the owner to acknowledge before giving up.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound on one forwarded message.
const MAX_MESSAGE_BYTES: u64 = 1024 * 1024;
/// Upper bound on the header line that picks the protocol.
const MAX_HEADER_BYTES: usize = 64;
/// Upper bound on forwarded arguments honored per connection.
const MAX_ARGS: usize = 256;
/// How many launches the owner serves at once; further connections are
/// dropped (the sender sees a closed pipe and counts the owner as gone,
/// which `forward` handles).
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
}

/// Sends `args` to a running owner. `NoOwner` covers every way a connect can
/// fail (nothing listening, stale socket file); only a connection that opens
/// but never answers is `Unresponsive`.
fn forward(endpoint: &Endpoint, args: &[String]) -> Forward {
    let Ok(name) = endpoint.name() else {
        return Forward::NoOwner;
    };
    let Ok(stream) = Stream::connect(name) else {
        return Forward::NoOwner;
    };
    let message = encode_message(args);
    let (tx, rx) = mpsc::channel();
    // No thread: `tx` is dropped with the closure and the wait below ends.
    let _ = crate::thread_guard::spawn_guarded("quill-single-instance-send", move || {
        let result = (|| -> io::Result<bool> {
            let mut stream = stream;
            stream.write_all(message.as_bytes())?;
            stream.flush()?;
            let mut ack = String::new();
            BufReader::new(&stream).read_line(&mut ack)?;
            Ok(ack.trim() == "ok")
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(ACK_TIMEOUT) {
        Ok(Ok(true)) => Forward::Delivered,
        // Connected, then dropped or garbled: a dying owner counts as gone.
        Ok(Ok(false) | Err(_)) => Forward::NoOwner,
        Err(_) => Forward::Unresponsive,
    }
}

fn encode_message(args: &[String]) -> String {
    let mut message = format!("{HEADER}\n");
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

fn serve(stream: Stream, slot: ClientSlot, on_launch: &(dyn Fn(Vec<String>) + Send + Sync)) {
    // Best effort: Windows named pipes refuse timeouts.
    let _ = stream.set_recv_timeout(Some(CLIENT_READ_TIMEOUT));
    let Some(header) = read_header(&stream) else {
        return;
    };
    let header = header.trim_end_matches('\r');
    if header == crate::agent::server::HEADER {
        // Agent connections stay open; they have a cap of their own.
        drop(slot);
        crate::agent::server::serve(stream);
        return;
    }
    if header != HEADER {
        return;
    }
    let Some(args) = decode_args(&stream) else {
        return;
    };
    on_launch(args);
    let mut out = &stream;
    let _ = out.write_all(b"ok\n");
    let _ = out.flush();
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
    #[cfg(unix)]
    if let Some(parent) = endpoint.path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _lock = AcquireLock::take(endpoint)?;
    match forward(endpoint, args) {
        Forward::Delivered => return Ok(Acquired::Forwarded),
        Forward::Unresponsive => {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "another Quill instance is running but not responding",
            ));
        }
        Forward::NoOwner => {}
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
                Ok(stream) => stream,
                Err(_) => {
                    std::thread::sleep(ACCEPT_ERROR_PAUSE);
                    continue;
                }
            };
            // Over the cap the connection is dropped, not queued.
            let Some(slot) = slots.try_take(MAX_CLIENT_THREADS) else {
                continue;
            };
            let on_launch = Arc::clone(&on_launch);
            // A client that connects and stalls must not block others.
            let _ = crate::thread_guard::spawn_guarded("quill-single-instance-client", move || {
                serve(stream, slot, &*on_launch)
            });
        }
    })?;
    Ok(Acquired::Primary)
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_root_keys_ignore_case_slashes_and_verbatim_prefix() {
        let a = super::windows_root_key(r"C:\Users\Me\AppData\Quill");
        assert_eq!(a, super::windows_root_key(r"c:\users\me\appdata\quill\"));
        assert_eq!(a, super::windows_root_key("c:/users/me/appdata/quill"));
        assert_eq!(a, super::windows_root_key(r"\\?\C:\Users\Me\AppData\Quill"));
        assert_ne!(a, super::windows_root_key(r"C:\Users\Me\AppData\Other"));
    }

    use super::{Acquired, ClientSlots, Endpoint, acquire, root_digest};
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

    #[test]
    fn foreign_clients_are_ignored() {
        use super::decode_message;
        assert_eq!(decode_message(&b"GET / HTTP/1.1\r\n\r\n"[..]), None);
        assert_eq!(
            decode_message(&b"quill-ipc 1\ntg://a\n\nignored\n"[..]),
            Some(vec!["tg://a".to_string()])
        );
    }
}
