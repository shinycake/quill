//! Scratch directories for tests that are removed when the test process exits.
//!
//! Tests used to build `temp_dir()/quill-<name>-<pid>` paths and never remove
//! them, so every run left a directory behind. Directories handed out here are
//! recorded and deleted by an `atexit` hook, which also runs when a test fails
//! or panics (libtest still exits through `exit`). A hard kill (SIGKILL) skips
//! it, as it would any destructor.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

static DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static HOOK: Once = Once::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

fn dirs() -> MutexGuard<'static, Vec<PathBuf>> {
    DIRS.lock().unwrap_or_else(PoisonError::into_inner)
}

extern "C" fn remove_all() {
    // A panic here would abort the exiting process; only non-panicking calls.
    let list = std::mem::take(&mut *dirs());
    for dir in list {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn check_prefix(prefix: &str) {
    debug_assert!(
        !prefix.contains(['/', '\\']) && !prefix.contains(".."),
        "scratch prefix must be a plain name: {prefix:?}"
    );
}

fn register_hook() {
    HOOK.call_once(|| {
        // SAFETY: `remove_all` is a plain `extern "C" fn()` that never unwinds.
        // A failed registration only means the directory is left behind.
        let _ = unsafe { libc::atexit(remove_all) };
    });
}

/// Records `dir` for removal at process exit.
pub fn remove_at_exit(dir: PathBuf) {
    register_hook();
    dirs().push(dir);
}

/// A freshly created directory `<temp>/<prefix>-<pid>-<n>`, removed at exit.
/// Every call returns a different directory.
pub fn unique_dir(prefix: &str) -> PathBuf {
    check_prefix(prefix);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    remove_at_exit(dir.clone());
    dir
}

/// The directory `<temp>/<prefix>-<pid>` shared by the whole test process,
/// created on first use and removed at exit. Repeated calls return the same
/// path, so it can hold fixtures that several tests reuse.
pub fn process_dir(prefix: &str) -> PathBuf {
    check_prefix(prefix);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    register_hook();
    let mut list = dirs();
    if !list.contains(&dir) {
        list.push(dir.clone());
    }
    dir
}

/// `<temp>/<name>`, a working directory the app itself creates (edited
/// photos, sticker clips). Release builds return exactly that path and never
/// remove anything. Test builds return `<temp>/<name>-<pid>` instead and remove
/// it at exit, so a test can never delete the directory of a running app.
pub fn temp_work_dir(name: &str) -> PathBuf {
    check_prefix(name);
    #[cfg(any(test, feature = "test-support"))]
    {
        process_dir(name)
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        std::env::temp_dir().join(name)
    }
}

#[cfg(test)]
mod tests {
    use super::{process_dir, unique_dir};

    #[test]
    fn unique_dirs_differ_and_exist() {
        let a = unique_dir("quill-scratch-test");
        let b = unique_dir("quill-scratch-test");
        assert_ne!(a, b);
        assert!(a.is_dir() && b.is_dir());
    }

    #[test]
    fn process_dir_is_stable() {
        let a = process_dir("quill-scratch-proc");
        assert_eq!(a, process_dir("quill-scratch-proc"));
        assert!(a.is_dir());
    }
}
