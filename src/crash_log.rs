//! Crash log: a panic hook that writes a report to
//! `<data root>/logs/crash-<unix-ts>.txt`, and the clean-exit marker the next
//! launch compares those files against (tdesktop's `tdata/working` file and
//! `CrashReports::Start` / `Finish`).
//!
//! A report holds the panic message, where it happened, the thread, the app
//! version, the OS and a backtrace. Nothing else from the session goes in it.
//! Only the newest [`KEEP_CRASH_FILES`] reports are kept.

use std::backtrace::Backtrace;
use std::fs;
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many crash reports stay on disk; older ones are removed after each
/// new one is written.
pub const KEEP_CRASH_FILES: usize = 10;

const CRASH_PREFIX: &str = "crash-";
const CRASH_SUFFIX: &str = ".txt";
/// Unix seconds of the last clean quit, or of the last crash the user was
/// told about.
const CLEAN_EXIT_FILE: &str = "clean-exit";

/// Install the crash-log hook in front of the current panic hook. Call it
/// first thing in `main`; the previous hook (the standard stderr message)
/// still runs after the report is written.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic(info);
        previous(info);
    }));
}

/// The logs folder inside the app data root (shared with the debug logs).
pub fn log_dir(root: &Path) -> PathBuf {
    crate::advanced::debug_mode::log_dir(root)
}

/// Write the report for `info`. Every failure is ignored: a panic hook that
/// panics aborts the process.
fn record_panic(info: &PanicHookInfo<'_>) {
    let Some(root) = crate::settings::safe_app_root() else {
        return;
    };
    let current = std::thread::current();
    let now = unix_now();
    let report = format_report(&CrashDetails {
        message: info
            .payload_as_str()
            .unwrap_or("(panic payload is not text)"),
        location: info
            .location()
            .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column())),
        thread: current.name().unwrap_or("(unnamed)"),
        unix_secs: now,
        backtrace: &Backtrace::force_capture().to_string(),
    });
    let _ = write_crash_file(&log_dir(&root), now, &report, KEEP_CRASH_FILES);
}

/// What one report says.
struct CrashDetails<'a> {
    message: &'a str,
    location: Option<String>,
    thread: &'a str,
    unix_secs: u64,
    backtrace: &'a str,
}

fn format_report(details: &CrashDetails<'_>) -> String {
    format!(
        "Quill {version} panicked.\n\
         os: {os} ({arch})\n\
         time: {time} (unix seconds)\n\
         thread: {thread}\n\
         location: {location}\n\
         message: {message}\n\
         \n\
         backtrace:\n{backtrace}\n",
        version = crate::version::APP,
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        time = details.unix_secs,
        thread = details.thread,
        location = details.location.as_deref().unwrap_or("(unknown)"),
        message = details.message,
        backtrace = details.backtrace,
    )
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Write `report` as a new crash file in `dir`, then keep only the newest
/// `keep` files. Two panics in the same second get `crash-<ts>-<n>.txt`.
pub fn write_crash_file(
    dir: &Path,
    unix_secs: u64,
    report: &str,
    keep: usize,
) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let mut serial = 0u32;
    let (path, mut file) = loop {
        let name = if serial == 0 {
            format!("{CRASH_PREFIX}{unix_secs}{CRASH_SUFFIX}")
        } else {
            format!("{CRASH_PREFIX}{unix_secs}-{serial}{CRASH_SUFFIX}")
        };
        let path = dir.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => break (path, file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && serial < 100 => {
                serial = serial.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    };
    file.write_all(report.as_bytes())?;
    prune_crash_files(dir, keep);
    Ok(path)
}

/// `crash-<ts>.txt` or `crash-<ts>-<n>.txt` → `(ts, n)`.
fn parse_crash_name(name: &str) -> Option<(u64, u32)> {
    let stem = name
        .strip_prefix(CRASH_PREFIX)?
        .strip_suffix(CRASH_SUFFIX)?;
    match stem.split_once('-') {
        Some((secs, serial)) => Some((secs.parse().ok()?, serial.parse().ok()?)),
        None => Some((stem.parse().ok()?, 0)),
    }
}

/// The crash files in `dir`, newest first.
fn crash_files(dir: &Path) -> Vec<((u64, u32), PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let key = parse_crash_name(entry.file_name().to_str()?)?;
            Some((key, entry.path()))
        })
        .collect();
    files.sort_by_key(|(key, _)| std::cmp::Reverse(*key));
    files
}

fn prune_crash_files(dir: &Path, keep: usize) {
    for (_, path) in crash_files(dir).into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

/// A crash the user has not been told about yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCrash {
    pub unix_secs: u64,
    pub path: PathBuf,
}

/// The newest crash report written after the last clean exit (or after the
/// last crash already shown), if any.
pub fn pending_crash(root: &Path) -> Option<PendingCrash> {
    let dir = log_dir(root);
    let since = read_marker(&dir).unwrap_or(0);
    let ((unix_secs, _), path) = crash_files(&dir).into_iter().next()?;
    (unix_secs > since).then_some(PendingCrash { unix_secs, path })
}

/// Record a normal quit, so the next launch shows no crash notice.
pub fn mark_clean_exit(root: &Path) {
    let _ = write_marker(&log_dir(root), unix_now());
}

/// The user has been told about `crash`: do not show it again.
pub fn acknowledge(root: &Path, crash: &PendingCrash) {
    let _ = write_marker(&log_dir(root), crash.unix_secs);
}

fn read_marker(dir: &Path) -> Option<u64> {
    fs::read_to_string(dir.join(CLEAN_EXIT_FILE))
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn write_marker(dir: &Path, unix_secs: u64) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(CLEAN_EXIT_FILE), unix_secs.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        CrashDetails, KEEP_CRASH_FILES, acknowledge, crash_files, format_report, log_dir,
        mark_clean_exit, parse_crash_name, pending_crash, write_crash_file, write_marker,
    };
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("quill-crash-log-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn names_parse_with_and_without_serial() {
        assert_eq!(parse_crash_name("crash-1700.txt"), Some((1700, 0)));
        assert_eq!(parse_crash_name("crash-1700-2.txt"), Some((1700, 2)));
        assert_eq!(parse_crash_name("crash-x.txt"), None);
        assert_eq!(parse_crash_name("quill-debug.log"), None);
    }

    #[test]
    fn writer_keeps_only_the_newest_files() {
        let root = temp_root("cap");
        let dir = log_dir(&root);
        for secs in 0..(KEEP_CRASH_FILES as u64 + 5) {
            write_crash_file(&dir, 1_000 + secs, "report", KEEP_CRASH_FILES).unwrap();
        }
        // Same second twice: the second one gets a serial.
        let again = write_crash_file(&dir, 1_014, "again", KEEP_CRASH_FILES).unwrap();
        assert!(again.ends_with("crash-1014-1.txt"));
        let files = crash_files(&dir);
        assert_eq!(files.len(), KEEP_CRASH_FILES);
        assert_eq!(files.first().unwrap().0, (1_014, 1));
        assert_eq!(files.last().unwrap().0, (1_006, 0));
        assert_eq!(std::fs::read_to_string(&again).unwrap(), "again");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn crash_after_clean_exit_is_pending_once() {
        let root = temp_root("pending");
        let dir = log_dir(&root);
        assert_eq!(pending_crash(&root), None);
        write_marker(&dir, 2_000).unwrap();
        write_crash_file(&dir, 1_500, "old", 3).unwrap();
        assert_eq!(pending_crash(&root), None);
        write_crash_file(&dir, 2_500, "new", 3).unwrap();
        let crash = pending_crash(&root).unwrap();
        assert_eq!(crash.unix_secs, 2_500);
        acknowledge(&root, &crash);
        assert_eq!(pending_crash(&root), None);
        mark_clean_exit(&root);
        assert_eq!(pending_crash(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn report_has_the_fields() {
        let report = format_report(&CrashDetails {
            message: "boom",
            location: Some("src/x.rs:1:2".into()),
            thread: "main",
            unix_secs: 42,
            backtrace: "frames",
        });
        for needle in ["boom", "src/x.rs:1:2", "thread: main", "time: 42", "frames"] {
            assert!(report.contains(needle), "{needle} missing");
        }
        assert!(report.contains(std::env::consts::OS));
        assert!(report.contains(crate::version::APP));
    }
}
