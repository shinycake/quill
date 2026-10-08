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
//! Stale sockets (the owner crashed): connecting fails with "refused"/"not
//! found", so the new process unlinks the leftover file and binds a fresh
//! one. A file lock serializes that check-and-bind sequence so two racing
//! launches cannot both conclude they are the owner.

use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

const HEADER: &str = "quill-ipc 1";
/// How long a launch waits for the owner to acknowledge before giving up.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(3);
/// Upper bound on one forwarded message.
const MAX_MESSAGE_BYTES: u64 = 64 * 1024;
/// Upper bound on forwarded arguments honored per connection.
const MAX_ARGS: usize = 32;
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

fn root_digest(root: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
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
        Self {
            pipe: format!("quill-{}.ipc", root_digest(root)),
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
    fn take(_endpoint: &Endpoint) -> io::Result<Self> {
        // Named pipes: the first listener instance is created exclusively.
        Ok(Self {})
    }
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
    std::thread::spawn(move || {
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
fn decode_message(reader: impl Read) -> Option<Vec<String>> {
    let mut reader = BufReader::new(reader.take(MAX_MESSAGE_BYTES));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.trim_end() != HEADER {
        return None;
    }
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

fn serve(stream: Stream, on_launch: &(dyn Fn(Vec<String>) + Send + Sync)) {
    let Some(args) = decode_message(&stream) else {
        return;
    };
    on_launch(args);
    let mut out = &stream;
    let _ = out.write_all(b"ok\n");
    let _ = out.flush();
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
    let listener = match ListenerOptions::new().name(endpoint.name()?).create_sync() {
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
    let on_launch: Arc<dyn Fn(Vec<String>) + Send + Sync> = Arc::new(on_launch);
    std::thread::Builder::new()
        .name("quill-single-instance".into())
        .spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let on_launch = Arc::clone(&on_launch);
                // A client that connects and stalls must not block others.
                std::thread::spawn(move || serve(stream, &*on_launch));
            }
        })?;
    Ok(Acquired::Primary)
}

#[cfg(test)]
mod tests {
    use super::{Acquired, Endpoint, acquire};
    use std::sync::mpsc;
    use std::time::Duration;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-si-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
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
