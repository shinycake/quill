//! An account's carrier data folders: the web views' files, cookies the
//! pages set included, under `<account>/webview-carrier/`.
//!
//! Each host gets a generation of its own, `<pid>-<n>`, held by a lock on
//! `<pid>-<n>.lock` for as long as the host or its teardown lives. A host
//! deletes only its own generation, once its helpers are gone, and never
//! on the UI thread. A new host sweeps, on a thread of its own, every
//! generation no live process holds. So a teardown that runs late never
//! touches the folder a newer host of the same account uses, and a second
//! Quill on the same account root keeps its folder too.
//!
//! On Windows WebView2's browser process may still hold files for a
//! moment after its helper exits, so the teardown retries for a while;
//! whatever is still there is swept by the next start.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How long a teardown keeps trying to delete its generation.
pub(super) const RETRY_FOR: Duration = Duration::from_secs(2);
const RETRY_EVERY: Duration = Duration::from_millis(100);
const LOCK: &str = ".lock";
/// Names tried before a claim gives up (each one taken means another
/// generation of this pid is there, live or left over).
const MAX_TRIES: u32 = 256;

/// The next generation number this process tries.
static NEXT: AtomicU32 = AtomicU32::new(0);

/// One host's data folder and the lock that says it is in use.
#[derive(Debug)]
pub(super) struct Generation {
    dir: PathBuf,
    lock_path: PathBuf,
    _lock: File,
}

impl Generation {
    /// A new, empty generation under `root`. Cheap (a lock file and a
    /// folder), so it may run on any thread. `None` when `root` cannot
    /// hold one.
    pub(super) fn claim(root: &Path) -> Option<Self> {
        std::fs::create_dir_all(root).ok()?;
        let pid = std::process::id();
        for _ in 0..MAX_TRIES {
            let name = format!("{pid}-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            if let Some(generation) = Self::try_claim(root, &name) {
                return Some(generation);
            }
        }
        None
    }

    fn try_claim(root: &Path, name: &str) -> Option<Self> {
        let lock_path = root.join(format!("{name}{LOCK}"));
        // `create_new`: a name a leftover of an earlier process with this
        // pid still holds is never reused.
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
            .ok()?;
        lock.try_lock().ok()?;
        // A sweep may have taken and deleted the file between the two
        // calls above; this lock would then guard nothing.
        if !lock_path.exists() {
            return None;
        }
        let dir = root.join(name);
        if std::fs::create_dir(&dir).is_err() {
            // A leftover folder without its lock: the sweep's.
            return None;
        }
        Some(Self {
            dir,
            lock_path,
            _lock: lock,
        })
    }

    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    /// Delete the folder, retrying for up to `retry_for`, then its lock.
    /// Blocks: run it off the UI thread. What will not go is left to the
    /// next start's sweep.
    pub(super) fn remove(self, retry_for: Duration) {
        let started = Instant::now();
        while !remove_tree(&self.dir) && started.elapsed() < retry_for {
            std::thread::sleep(RETRY_EVERY);
        }
        if self.dir.exists() {
            if std::env::var_os("QUILL_WEBVIEW_DEBUG").is_some() {
                for line in listing(&self.dir, 50) {
                    eprintln!("carrier data left: {line}");
                }
            }
            return;
        }
        let _ = std::fs::remove_file(&self.lock_path);
    }

    /// [`Generation::remove`] on a thread of its own. Without a thread the
    /// folder waits for the next start.
    pub(super) fn remove_in_background(self, retry_for: Duration) {
        let _ = crate::thread_guard::spawn_guarded("quill-carrier-sweep", move || {
            self.remove(retry_for);
        });
    }
}

/// Whether `path` is gone.
fn remove_tree(path: &Path) -> bool {
    std::fs::remove_dir_all(path).is_ok() || !path.exists()
}

/// `<pid>-<n>`.
fn is_generation(name: &str) -> bool {
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
    name.split_once('-')
        .is_some_and(|(pid, n)| digits(pid) && digits(n))
}

/// Delete, on a thread of its own, every generation under `root` that no
/// live process holds, and anything else in there that is not a
/// generation.
pub(super) fn sweep_in_background(root: PathBuf) -> Option<JoinHandle<Option<()>>> {
    crate::thread_guard::spawn_guarded("quill-carrier-sweep", move || sweep(&root)).ok()
}

fn sweep(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(generation) = name.strip_suffix(LOCK)
            && is_generation(generation)
        {
            // A lock whose folder is gone (deleted before its lock was).
            if !root.join(generation).exists() {
                remove_if_unheld(root, generation);
            }
        } else if is_generation(name) {
            remove_if_unheld(root, name);
        } else {
            remove_tree(&path);
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Delete generation `name` and its lock, unless a live host holds it.
///
/// A sweep never creates a lock file: only [`Generation::try_claim`] does,
/// with `create_new`, so a claim that holds the lock and still finds its
/// file knows the file is its own. A folder without a lock file is never
/// in use (a claim makes the lock first, a teardown deletes it last).
fn remove_if_unheld(root: &Path, name: &str) {
    let lock_path = root.join(format!("{name}{LOCK}"));
    match OpenOptions::new().write(true).open(&lock_path) {
        Ok(lock) => {
            if lock.try_lock().is_err() {
                return;
            }
            if remove_tree(&root.join(name)) {
                let _ = std::fs::remove_file(&lock_path);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            remove_tree(&root.join(name));
        }
        Err(_) => {}
    }
}

/// Up to `limit` entries still under `dir`, as relative paths with sizes,
/// for the debug trace of a folder that would not go away.
fn listing(dir: &Path, limit: usize) -> Vec<String> {
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

#[cfg(test)]
mod tests;
