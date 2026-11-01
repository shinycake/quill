//! Settings writes that keep the disk off the UI thread.
//!
//! The atomic write syncs the file, which can stall for tens of
//! milliseconds on a slow or scanned disk. [`write_json_deferred`]
//! serializes on the caller's thread (the value is small) and hands the
//! bytes to a short-lived writer thread. Pending writes are coalesced by
//! path, so the newest bytes for a path win and earlier ones are dropped.
//! Each write is still the atomic temp-file-and-rename, and
//! [`flush_deferred_writes`] waits for everything queued, for quitting.
//! A write that fails stays queued, so reads still see it, and is tried
//! again with the next submit or the flush.

use super::{json_bytes, write_bytes_atomic};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

type Sink = dyn Fn(&Path, &[u8]) -> std::io::Result<()> + Send + Sync;

#[derive(Default)]
struct State {
    /// Kept until the bytes are on disk, so a read during the write sees them.
    pending: HashMap<PathBuf, Queued>,
    writer_running: bool,
    failure: Option<std::io::Error>,
}

struct Queued {
    bytes: Arc<Vec<u8>>,
    /// The last try failed: the writer leaves it until a submit or a
    /// flush asks for another.
    failed: bool,
}

impl State {
    fn has_work(&self) -> bool {
        self.pending.values().any(|queued| !queued.failed)
    }

    fn retry_failed(&mut self) {
        for queued in self.pending.values_mut() {
            queued.failed = false;
        }
    }
}

struct Shared {
    state: Mutex<State>,
    idle: Condvar,
    sink: Box<Sink>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Coalescing, latest-wins background writer.
#[derive(Clone)]
pub(crate) struct DeferredWriter {
    shared: Arc<Shared>,
}

impl DeferredWriter {
    pub(crate) fn new(sink: Box<Sink>) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                idle: Condvar::new(),
                sink,
            }),
        }
    }

    /// Queue `bytes` for `path`, replacing anything still queued for it.
    pub(crate) fn submit(&self, path: PathBuf, bytes: Vec<u8>) {
        let start = {
            let mut state = self.shared.lock();
            state.retry_failed();
            state.pending.insert(
                path,
                Queued {
                    bytes: Arc::new(bytes),
                    failed: false,
                },
            );
            !std::mem::replace(&mut state.writer_running, true)
        };
        if start && !self.spawn_writer() {
            // No thread to write on: keep the bytes queued (reads still see
            // them), so the UI thread never waits for the disk. The next
            // submit tries a thread again, and `flush` writes them at quit.
            self.shared.lock().writer_running = false;
        }
    }

    /// The bytes still queued for `path`, so a read sees a write that has
    /// not reached the disk yet.
    pub(crate) fn pending(&self, path: &Path) -> Option<Vec<u8>> {
        self.shared
            .lock()
            .pending
            .get(path)
            .map(|queued| queued.bytes.as_ref().clone())
    }

    /// The latest write that failed since the last call, for the status note.
    pub(crate) fn take_failure(&self) -> Option<std::io::Error> {
        self.shared.lock().failure.take()
    }

    fn spawn_writer(&self) -> bool {
        let this = self.clone();
        std::thread::Builder::new()
            .name("quill-settings-writer".into())
            .spawn(move || this.drain())
            .is_ok()
    }

    /// Write what is queued until nothing is left, then mark the writer
    /// stopped. Runs on the writer thread, or inside `flush` if none could
    /// start.
    fn drain(&self) {
        loop {
            let batch = {
                let mut state = self.shared.lock();
                if !state.has_work() {
                    state.writer_running = false;
                    self.shared.idle.notify_all();
                    return;
                }
                state
                    .pending
                    .iter()
                    .filter(|(_, queued)| !queued.failed)
                    .map(|(path, queued)| (path.clone(), queued.bytes.clone()))
                    .collect::<Vec<_>>()
            };
            for (path, bytes) in batch {
                let written = (self.shared.sink)(&path, &bytes);
                let mut state = self.shared.lock();
                let current = state
                    .pending
                    .get_mut(&path)
                    .filter(|queued| Arc::ptr_eq(&queued.bytes, &bytes));
                match written {
                    Ok(()) => {
                        if current.is_some() {
                            state.pending.remove(&path);
                        }
                    }
                    Err(err) => {
                        if let Some(queued) = current {
                            queued.failed = true;
                        }
                        state.failure = Some(err);
                    }
                }
            }
        }
    }

    /// Wait until everything queued is written, or `timeout` passes.
    pub(crate) fn flush(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let mut state = self.shared.lock();
        state.retry_failed();
        while state.writer_running || state.has_work() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            if !state.writer_running {
                // Queued with no writer (its thread never started): write
                // here, the one place that is allowed to wait for the disk.
                state.writer_running = true;
                drop(state);
                self.drain();
                state = self.shared.lock();
                continue;
            }
            state = match self.shared.idle.wait_timeout(state, left) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }
}

fn global() -> &'static DeferredWriter {
    static WRITER: OnceLock<DeferredWriter> = OnceLock::new();
    WRITER.get_or_init(|| {
        DeferredWriter::new(Box::new(|path, bytes| {
            #[cfg(test)]
            test_gate::wait(path);
            write_bytes_atomic(path, bytes)
        }))
    })
}

/// A settings write that failed on the writer thread since the last call.
pub fn take_deferred_write_failure() -> Option<std::io::Error> {
    global().take_failure()
}

/// Holds the shared writer before it writes one path, so a test can look
/// at the state while that write is still waiting.
#[cfg(test)]
pub(crate) mod test_gate {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::sync::mpsc::{Receiver, Sender, channel};

    static GATES: Mutex<Option<HashMap<PathBuf, Receiver<()>>>> = Mutex::new(None);

    /// Block writes of `path` until the returned sender sends or drops.
    pub(crate) fn hold(path: &Path) -> Sender<()> {
        let (open, gate) = channel();
        if let Ok(mut gates) = GATES.lock() {
            gates
                .get_or_insert_default()
                .insert(path.to_path_buf(), gate);
        }
        open
    }

    pub(super) fn wait(path: &Path) {
        let gate = GATES
            .lock()
            .ok()
            .and_then(|mut gates| gates.as_mut()?.remove(path));
        if let Some(gate) = gate {
            let _ = gate.recv();
        }
    }
}

/// Save `value` as JSON at `path` without waiting for the disk. A newer
/// call for the same path replaces one still waiting. A failure, and a
/// value that cannot be serialized, reach the user as the one status note
/// read through [`take_deferred_write_failure`].
pub(crate) fn write_json_deferred<T: Serialize>(path: &Path, value: &T) {
    match json_bytes(value) {
        Ok(bytes) => global().submit(path.to_path_buf(), bytes),
        Err(err) => global().shared.lock().failure = Some(err),
    }
}

/// The bytes of `path`, from the queue when a write is still waiting.
pub(crate) fn read_with_pending(path: &Path) -> std::io::Result<Vec<u8>> {
    match global().pending(path) {
        Some(bytes) => Ok(bytes),
        None => std::fs::read(path),
    }
}

/// Wait for every deferred write; call when the app quits.
pub fn flush_deferred_writes() {
    global().flush(Duration::from_secs(5));
}

#[cfg(test)]
mod tests {
    use super::DeferredWriter;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    type Log = Arc<Mutex<Vec<(PathBuf, Vec<u8>)>>>;

    /// A writer whose sink tells `entered` it started, then waits on `gate`.
    fn recording(gate: Option<(mpsc::Receiver<()>, mpsc::Sender<()>)>) -> (DeferredWriter, Log) {
        let log: Log = Arc::default();
        let seen = log.clone();
        let gate = Mutex::new(gate);
        let writer = DeferredWriter::new(Box::new(move |path, bytes| {
            if let Ok(gate) = gate.lock()
                && let Some((gate, entered)) = gate.as_ref()
            {
                let _ = entered.send(());
                let _ = gate.recv_timeout(Duration::from_secs(5));
            }
            if let Ok(mut log) = seen.lock() {
                log.push((path.to_path_buf(), bytes.to_vec()));
            }
            Ok(())
        }));
        (writer, log)
    }

    #[test]
    fn last_write_wins_while_a_write_is_in_flight() {
        let (open, gate) = mpsc::channel();
        let (entered_tx, entered) = mpsc::channel();
        let (writer, log) = recording(Some((gate, entered_tx)));
        let path = PathBuf::from("/x/prefs.json");
        writer.submit(path.clone(), b"a".to_vec());
        assert!(entered.recv_timeout(Duration::from_secs(5)).is_ok());
        // The first write is stuck in the sink; these queue behind it.
        writer.submit(path.clone(), b"b".to_vec());
        writer.submit(path.clone(), b"c".to_vec());
        assert_eq!(writer.pending(&path), Some(b"c".to_vec()));
        for _ in 0..3 {
            let _ = open.send(());
        }
        writer.flush(Duration::from_secs(5));
        let written: Vec<Vec<u8>> = log
            .lock()
            .map(|l| l.iter().map(|(_, b)| b.clone()).collect())
            .unwrap_or_default();
        assert_eq!(written, vec![b"a".to_vec(), b"c".to_vec()]);
        assert_eq!(writer.pending(&path), None);
    }

    #[test]
    fn different_paths_are_each_written_and_flush_waits() {
        let (writer, log) = recording(None);
        for name in ["a.json", "b.json", "c.json"] {
            writer.submit(Path::new("/x").join(name), name.as_bytes().to_vec());
        }
        writer.flush(Duration::from_secs(5));
        let count = log.lock().map(|l| l.len()).unwrap_or(0);
        assert_eq!(count, 3);
    }

    #[test]
    fn writes_reach_disk_atomically_and_flush_on_quit() {
        let dir = std::env::temp_dir().join(format!("quill-deferred-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("prefs.json");
        let writer = DeferredWriter::new(Box::new(|path, bytes| {
            super::write_bytes_atomic(path, bytes)
        }));
        writer.submit(path.clone(), b"{\"a\":1}".to_vec());
        writer.submit(path.clone(), b"{\"a\":2}".to_vec());
        writer.flush(Duration::from_secs(5));
        assert_eq!(std::fs::read(&path).ok(), Some(b"{\"a\":2}".to_vec()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bytes_queued_without_a_writer_stay_readable_and_flush_writes_them() {
        let (writer, log) = recording(None);
        let path = PathBuf::from("/x/stuck.json");
        // The state `submit` leaves when its writer thread cannot start.
        writer.shared.lock().pending.insert(
            path.clone(),
            super::Queued {
                bytes: Arc::new(b"q".to_vec()),
                failed: false,
            },
        );
        assert_eq!(writer.pending(&path), Some(b"q".to_vec()));
        assert_eq!(log.lock().map(|l| l.len()).unwrap_or(9), 0);
        writer.flush(Duration::from_secs(5));
        assert_eq!(log.lock().map(|l| l.len()).unwrap_or(0), 1);
        assert_eq!(writer.pending(&path), None);
    }

    #[test]
    fn a_failed_write_is_reported_once() {
        let writer = DeferredWriter::new(Box::new(|_, _| Err(std::io::Error::other("disk full"))));
        assert!(writer.take_failure().is_none());
        writer.submit(PathBuf::from("/x/a.json"), b"a".to_vec());
        writer.flush(Duration::from_secs(5));
        let failure = writer.take_failure().map(|err| err.to_string());
        assert_eq!(failure.as_deref(), Some("disk full"));
        assert!(writer.take_failure().is_none());
    }

    #[test]
    fn a_failed_write_stays_queued_and_the_next_submit_tries_it_again() {
        let full = Arc::new(Mutex::new(true));
        let (seen, disk) = (Log::default(), full.clone());
        let written = seen.clone();
        let writer = DeferredWriter::new(Box::new(move |path, bytes| {
            if disk.lock().map(|full| *full).unwrap_or(false) {
                return Err(std::io::Error::other("disk full"));
            }
            if let Ok(mut log) = written.lock() {
                log.push((path.to_path_buf(), bytes.to_vec()));
            }
            Ok(())
        }));
        let (prefs, other) = (PathBuf::from("/x/prefs.json"), PathBuf::from("/x/b.json"));
        writer.submit(prefs.clone(), b"kept".to_vec());
        writer.flush(Duration::from_secs(5));
        assert_eq!(writer.pending(&prefs), Some(b"kept".to_vec()));
        assert!(writer.take_failure().is_some());
        if let Ok(mut full) = full.lock() {
            *full = false;
        }
        writer.submit(other.clone(), b"b".to_vec());
        writer.flush(Duration::from_secs(5));
        assert_eq!(writer.pending(&prefs), None);
        let mut on_disk = seen.lock().map(|l| l.clone()).unwrap_or_default();
        on_disk.sort();
        assert_eq!(
            on_disk,
            vec![(other, b"b".to_vec()), (prefs, b"kept".to_vec())]
        );
    }

    #[test]
    fn a_write_on_its_way_to_disk_stays_readable() {
        let (open, gate) = mpsc::channel();
        let (entered_tx, entered) = mpsc::channel();
        let (writer, _log) = recording(Some((gate, entered_tx)));
        let path = PathBuf::from("/x/prefs.json");
        writer.submit(path.clone(), b"a".to_vec());
        assert!(entered.recv_timeout(Duration::from_secs(5)).is_ok());
        assert_eq!(writer.pending(&path), Some(b"a".to_vec()));
        let _ = open.send(());
        writer.flush(Duration::from_secs(5));
        assert_eq!(writer.pending(&path), None);
    }
}
