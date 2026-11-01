//! Scratch directories for tests that are removed when the test process exits.
//!
//! Tests used to build `temp_dir()/quill-<name>-<pid>` paths and never remove
//! them, so every run left a directory behind. Directories handed out here are
//! recorded and deleted by an `atexit` hook, which also runs when a test fails
//! or panics (libtest still exits through `exit`). A hard kill (SIGKILL) skips
//! it, as it would any destructor.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};
use std::time::{Duration, SystemTime};

static DIRS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static HOOK: Once = Once::new();
static NEXT: AtomicU64 = AtomicU64::new(0);

fn dirs() -> MutexGuard<'static, Vec<PathBuf>> {
    DIRS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Also the `atexit` hook; an exit that skips the hooks calls it first.
pub(crate) extern "C" fn remove_all() {
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

/// Subfolders of the work root that the startup sweep empties of stale entries.
#[cfg(not(any(test, feature = "test-support")))]
const WORK_NAMES: [&str; 6] = [
    "quill-send-photos",
    "quill-edits",
    "quill-video-edits",
    "quill-profile-photo",
    "quill-sticker-video",
    "quill-editor-sticker",
];

/// How old an entry must be before the sweep removes it. A send or edit
/// uses a file within minutes of making it, so a day leaves a wide margin.
pub const WORK_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// How deep the sweep looks for the newest change inside one entry.
const SWEEP_DEPTH: u32 = 4;

/// The per-user private folder under the temp dir that holds Quill's working
/// files (edited photos, scaled copies, sticker clips). Owner-only (0700 on
/// unix) and named after the user, so another local user on a shared `/tmp`
/// can neither read it nor pre-create the name. Test builds return the temp
/// dir itself, so `<temp>/<name>-<pid>` stays unique to the test process.
pub fn work_root() -> PathBuf {
    #[cfg(any(test, feature = "test-support"))]
    {
        std::env::temp_dir()
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        ROOT.get_or_init(private_root).clone()
    }
}

#[cfg(not(any(test, feature = "test-support")))]
fn private_root() -> PathBuf {
    let tag = user_tag();
    let first = format!("quill-work{tag}");
    if let Some(root) = create_private_root(&first) {
        return root;
    }
    // Someone else owns the name (or it is unusable): take a private name
    // of our own for this run.
    let second = format!("quill-work{tag}-{}", std::process::id());
    create_private_root(&second).unwrap_or_else(|| std::env::temp_dir().join(second))
}

/// `<temp>/<name>` as an owner-only directory, or `None` when that cannot be
/// made. A symlink in its place is replaced, never followed.
#[cfg(any(test, not(feature = "test-support")))]
fn create_private_root(name: &str) -> Option<PathBuf> {
    let root = std::env::temp_dir().join(name);
    crate::local_path::secure_create_dir(&root).ok()?;
    Some(root)
}

/// `-<uid>` on unix. Windows' temp dir is already per user.
#[cfg(all(unix, not(any(test, feature = "test-support"))))]
fn user_tag() -> String {
    // SAFETY: `geteuid` takes no arguments, cannot fail and touches no memory.
    format!("-{}", unsafe { libc::geteuid() })
}

#[cfg(all(not(unix), not(any(test, feature = "test-support"))))]
fn user_tag() -> String {
    String::new()
}

/// `<work root>/<name>`, a working directory the app itself creates (edited
/// photos, sticker clips). The startup sweep ([`sweep_work_dirs_once`])
/// removes what it holds after [`WORK_MAX_AGE`]. Test builds return
/// `<temp>/<name>-<pid>` instead and remove it at exit, so a test can never
/// delete the directory of a running app.
pub fn temp_work_dir(name: &str) -> PathBuf {
    check_prefix(name);
    #[cfg(any(test, feature = "test-support"))]
    {
        process_dir(name)
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        work_root().join(name)
    }
}

/// Remove stale working files once per process, on a background thread:
/// entries of the [`WORK_NAMES`] folders in the private work root not touched
/// for [`WORK_MAX_AGE`]. Called only from real app startup, never from a
/// connection restart or a demo run, so a file a tray or editor holds open
/// in a long session is not swept from under it. Later calls do nothing. A
/// no-op in test builds.
pub fn sweep_work_dirs_once() {
    #[cfg(not(any(test, feature = "test-support")))]
    {
        static SWEPT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        run_once(&SWEPT, || {
            let _ = crate::thread_guard::spawn_guarded("quill-work-sweep", || {
                sweep_stale(&work_root(), &WORK_NAMES, WORK_MAX_AGE, SystemTime::now());
            });
        });
    }
}

/// Run `job` the first time `guard` is seen; returns whether it ran.
#[cfg(any(test, not(feature = "test-support")))]
fn run_once(guard: &std::sync::OnceLock<()>, job: impl FnOnce()) -> bool {
    let mut ran = false;
    guard.get_or_init(|| {
        ran = true;
        job();
    });
    ran
}

/// Remove entries of `root/<name>` (for each name) whose newest change is
/// older than `max_age` before `now`; returns how many were removed. Only the
/// age decides. `root` and each folder must be real directories: a symlink in
/// either place is skipped, so the sweep never leaves the tree it was given.
/// Symlinks inside are unlinked, never followed.
pub fn sweep_stale(root: &Path, names: &[&str], max_age: Duration, now: SystemTime) -> usize {
    if !is_real_dir(root) {
        return 0;
    }
    let mut removed = 0;
    for name in names {
        let folder = root.join(name);
        if !is_real_dir(&folder) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let stale = newest_change(&path, SWEEP_DEPTH)
                .and_then(|changed| now.duration_since(changed).ok())
                .is_some_and(|age| age > max_age);
            if stale && remove_entry(&path) {
                removed += 1;
            }
        }
    }
    removed
}

fn is_real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir())
}

fn remove_entry(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path).is_ok(),
        Ok(_) => std::fs::remove_file(path).is_ok(),
        Err(_) => false,
    }
}

/// The latest modification time of `path` and, to `depth` levels, what is
/// inside it. `None` when it cannot be read (then it is kept).
fn newest_change(path: &Path, depth: u32) -> Option<SystemTime> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let mut newest = meta.modified().ok()?;
    if meta.is_dir() {
        let Some(deeper) = depth.checked_sub(1) else {
            // Too deep to judge: treat as just changed, keep it.
            return Some(SystemTime::now());
        };
        for entry in std::fs::read_dir(path).ok()?.flatten() {
            newest = newest.max(newest_change(&entry.path(), deeper)?);
        }
    }
    Some(newest)
}

#[cfg(test)]
mod tests {
    use super::{create_private_root, process_dir, run_once, sweep_stale, unique_dir};
    use std::time::{Duration, SystemTime};

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn set_age(path: &std::path::Path, age: Duration) {
        let file = std::fs::OpenOptions::new().write(true).open(path);
        if let Ok(file) = file {
            let _ = file.set_modified(SystemTime::now() - age);
        }
    }

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

    #[test]
    fn sweep_removes_only_entries_older_than_the_limit() {
        let root = unique_dir("quill-sweep-age");
        let folder = root.join("quill-edits");
        let nested = folder.join("clip");
        std::fs::create_dir_all(&nested).unwrap();
        let old = folder.join("old.png");
        let fresh = folder.join("fresh.png");
        let busy_old = nested.join("part.mp4");
        let busy_new = nested.join("part2.mp4");
        for path in [&old, &fresh, &busy_old, &busy_new] {
            std::fs::write(path, b"x").unwrap();
        }
        set_age(&old, DAY * 2);
        set_age(&busy_old, DAY * 2);
        let removed = sweep_stale(
            &root,
            &["quill-edits", "quill-missing"],
            DAY,
            SystemTime::now(),
        );
        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(
            busy_old.exists() && busy_new.exists(),
            "a folder with a recent file inside is in use"
        );
    }

    #[test]
    fn the_sweep_guard_runs_the_job_once() {
        let guard = std::sync::OnceLock::new();
        let mut count = 0;
        assert!(run_once(&guard, || count += 1));
        assert!(!run_once(&guard, || count += 1));
        assert_eq!(count, 1);
    }

    #[test]
    fn sweep_removes_a_wholly_stale_folder() {
        let root = unique_dir("quill-sweep-tree");
        let nested = root.join("quill-video-edits").join("job");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("out.mp4");
        std::fs::write(&file, b"x").unwrap();
        let later = SystemTime::now() + DAY * 2;
        assert_eq!(sweep_stale(&root, &["quill-video-edits"], DAY, later), 1);
        assert!(!nested.exists());
    }

    #[cfg(unix)]
    #[test]
    fn sweep_does_not_follow_a_symlinked_root_or_folder() {
        let outside = unique_dir("quill-sweep-outside");
        let victim = outside.join("quill-edits");
        std::fs::create_dir_all(&victim).unwrap();
        let file = victim.join("keep.png");
        std::fs::write(&file, b"x").unwrap();
        let later = SystemTime::now() + DAY * 5;

        let holder = unique_dir("quill-sweep-link");
        let linked_root = holder.join("root");
        std::os::unix::fs::symlink(&outside, &linked_root).unwrap();
        assert_eq!(sweep_stale(&linked_root, &["quill-edits"], DAY, later), 0);

        let real_root = holder.join("real");
        std::fs::create_dir_all(&real_root).unwrap();
        std::os::unix::fs::symlink(&victim, real_root.join("quill-edits")).unwrap();
        assert_eq!(sweep_stale(&real_root, &["quill-edits"], DAY, later), 0);
        assert!(file.exists(), "nothing outside the tree is touched");
    }

    #[cfg(unix)]
    #[test]
    fn private_root_is_owner_only_and_replaces_a_planted_symlink() {
        use std::os::unix::fs::PermissionsExt;
        let name = format!("quill-work-test-{}", std::process::id());
        let path = std::env::temp_dir().join(&name);
        let _ = std::fs::remove_dir_all(&path);
        let _ = std::fs::remove_file(&path);
        let target = unique_dir("quill-work-target");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let root = create_private_root(&name);
        assert_eq!(root.as_deref(), Some(path.as_path()));
        let meta = std::fs::symlink_metadata(&path).unwrap();
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        assert!(target.is_dir(), "the symlink's target is left alone");
        super::remove_at_exit(path);
    }
}
