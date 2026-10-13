//! Crash log: a panic hook that writes a report to
//! `<data root>/logs/crash-<unix-ts>.txt`, and the clean-exit marker the next
//! launch compares those files against (tdesktop's `tdata/working` file and
//! `CrashReports::Start` / `Finish`).
//!
//! A report holds the panic message, where it happened, the thread, the app
//! version, the OS and a backtrace. Nothing else from the session goes in it.
//! Only the newest [`KEEP_CRASH_FILES`] reports are kept.
//!
//! A panic that `spawn_guarded` or `ffi_boundary` will catch
//! (`crate::thread_guard`) is filed as `panic-caught-<unix-ts>.txt`
//! instead: Quill kept running, so the next launch does not say it closed.
//! Those reports are capped separately, so a burst of them never pushes
//! out the report of a real crash.

use std::backtrace::Backtrace;
use std::fs;
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// How many crash reports stay on disk; older ones are removed after each
/// new one is written.
pub const KEEP_CRASH_FILES: usize = 10;

const CRASH_PREFIX: &str = "crash-";
const CAUGHT_PREFIX: &str = "panic-caught-";
const CRASH_SUFFIX: &str = ".txt";
/// Unix seconds of the last clean quit, or of the last crash the user was
/// told about.
const CLEAN_EXIT_FILE: &str = "clean-exit";

/// What a report is about: a panic that ended the process, or one a
/// thread or FFI guard caught.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Crash,
    Caught,
}

impl ReportKind {
    /// The kind for a panic happening on this thread now.
    pub fn for_current_thread() -> Self {
        if crate::thread_guard::panic_will_be_caught() {
            Self::Caught
        } else {
            Self::Crash
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Crash => CRASH_PREFIX,
            Self::Caught => CAUGHT_PREFIX,
        }
    }
}

/// Install the crash-log hook in front of the current panic hook. Call it
/// first thing in `main`; the previous hook (the standard stderr message)
/// still runs after the report is written.
pub fn install_panic_hook() {
    session_start();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        record_panic(info);
        previous(info);
    }));
}

static SESSION_START: OnceLock<u64> = OnceLock::new();

/// Unix seconds when this run of Quill started (first asked for by
/// `install_panic_hook`, the first line of `main`). Crash reports from
/// before it are a previous run's; later ones were written while this run
/// kept going (a panic on a thread no guard covers).
pub fn session_start() -> u64 {
    *SESSION_START.get_or_init(unix_now)
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
    let kind = ReportKind::for_current_thread();
    let report = format_report(&CrashDetails {
        kind,
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
    let _ = write_report_file(&log_dir(&root), kind, now, &report, KEEP_CRASH_FILES);
}

/// What one report says.
struct CrashDetails<'a> {
    kind: ReportKind,
    message: &'a str,
    location: Option<String>,
    thread: &'a str,
    unix_secs: u64,
    backtrace: &'a str,
}

fn format_report(details: &CrashDetails<'_>) -> String {
    format!(
        "Quill {version} panicked{outcome}.\n\
         os: {os} ({arch})\n\
         time: {time} (unix seconds)\n\
         thread: {thread}\n\
         location: {location}\n\
         message: {message}\n\
         \n\
         backtrace:\n{backtrace}\n",
        version = crate::version::APP,
        outcome = match details.kind {
            ReportKind::Crash => "",
            ReportKind::Caught => " (caught; Quill kept running)",
        },
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
/// `keep` crash files. Two panics in the same second get `crash-<ts>-<n>.txt`.
pub fn write_crash_file(
    dir: &Path,
    unix_secs: u64,
    report: &str,
    keep: usize,
) -> io::Result<PathBuf> {
    write_report_file(dir, ReportKind::Crash, unix_secs, report, keep)
}

/// Write `report` as a new file of `kind` in `dir`, then keep only the
/// newest `keep` files of that kind.
pub fn write_report_file(
    dir: &Path,
    kind: ReportKind,
    unix_secs: u64,
    report: &str,
    keep: usize,
) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let prefix = kind.prefix();
    let mut serial = 0u32;
    let (path, mut file) = loop {
        let name = if serial == 0 {
            format!("{prefix}{unix_secs}{CRASH_SUFFIX}")
        } else {
            format!("{prefix}{unix_secs}-{serial}{CRASH_SUFFIX}")
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
    prune_report_files(dir, kind, keep);
    Ok(path)
}

/// `<prefix><ts>.txt` or `<prefix><ts>-<n>.txt` → `(ts, n)` for `kind`
/// (`crash-1700.txt`, `panic-caught-1700-2.txt`).
fn parse_report_name(name: &str, kind: ReportKind) -> Option<(u64, u32)> {
    let stem = name
        .strip_prefix(kind.prefix())?
        .strip_suffix(CRASH_SUFFIX)?;
    match stem.split_once('-') {
        Some((secs, serial)) => Some((secs.parse().ok()?, serial.parse().ok()?)),
        None => Some((stem.parse().ok()?, 0)),
    }
}

/// The crash files in `dir`, newest first.
fn crash_files(dir: &Path) -> Vec<((u64, u32), PathBuf)> {
    report_files(dir, ReportKind::Crash)
}

/// The report files of `kind` in `dir`, newest first.
fn report_files(dir: &Path, kind: ReportKind) -> Vec<((u64, u32), PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let key = parse_report_name(entry.file_name().to_str()?, kind)?;
            Some((key, entry.path()))
        })
        .collect();
    files.sort_by_key(|(key, _)| std::cmp::Reverse(*key));
    files
}

fn prune_report_files(dir: &Path, kind: ReportKind, keep: usize) {
    for (_, path) in report_files(dir, kind).into_iter().skip(keep) {
        let _ = fs::remove_file(path);
    }
}

/// A crash the user has not been told about yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCrash {
    pub unix_secs: u64,
    pub path: PathBuf,
}

/// The newest crash report from before this run that was written after the
/// last clean exit (or after the last crash already shown), if any.
pub fn pending_crash(root: &Path) -> Option<PendingCrash> {
    pending_crash_before(root, session_start())
}

/// [`pending_crash`] for a run that started at `session_start`: reports
/// from that second on are ignored.
pub fn pending_crash_before(root: &Path, session_start: u64) -> Option<PendingCrash> {
    let dir = log_dir(root);
    let since = read_marker(&dir).unwrap_or(0);
    let ((unix_secs, _), path) = crash_files(&dir)
        .into_iter()
        .find(|((unix_secs, _), _)| *unix_secs < session_start)?;
    (unix_secs > since).then_some(PendingCrash { unix_secs, path })
}

/// Record a normal quit, so the next launch shows no crash notice.
pub fn mark_clean_exit(root: &Path) {
    mark_clean_exit_for_session(root, session_start(), unix_now());
}

/// Quitting normally proves every crash report written since
/// `session_start` was a panic this run survived: those are renamed to
/// caught reports. A crash from before this run that the user was never
/// shown (Quill stayed hidden in the tray) keeps the old marker, so the next
/// launch still shows it; otherwise the marker moves to `now`.
fn mark_clean_exit_for_session(root: &Path, session_start: u64, now: u64) {
    let dir = log_dir(root);
    for ((unix_secs, _), path) in crash_files(&dir) {
        if unix_secs >= session_start {
            let _ = reclassify_as_caught(&dir, unix_secs, &path);
        }
    }
    prune_report_files(&dir, ReportKind::Caught, KEEP_CRASH_FILES);
    if pending_crash_before(root, session_start).is_some() {
        return;
    }
    let _ = write_marker(&dir, now);
}

/// Rename the crash report at `path` to a free `panic-caught-<ts>` name.
fn reclassify_as_caught(dir: &Path, unix_secs: u64, path: &Path) -> io::Result<()> {
    for serial in 0..100u32 {
        let name = if serial == 0 {
            format!("{CAUGHT_PREFIX}{unix_secs}{CRASH_SUFFIX}")
        } else {
            format!("{CAUGHT_PREFIX}{unix_secs}-{serial}{CRASH_SUFFIX}")
        };
        let target = dir.join(name);
        if !target.exists() {
            return fs::rename(path, target);
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
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
        CrashDetails, KEEP_CRASH_FILES, ReportKind, acknowledge, crash_files, format_report,
        log_dir, mark_clean_exit, mark_clean_exit_for_session, parse_report_name, pending_crash,
        pending_crash_before, report_files, write_crash_file, write_marker, write_report_file,
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
        let crash = |name| parse_report_name(name, ReportKind::Crash);
        let caught = |name| parse_report_name(name, ReportKind::Caught);
        assert_eq!(crash("crash-1700.txt"), Some((1700, 0)));
        assert_eq!(crash("crash-1700-2.txt"), Some((1700, 2)));
        assert_eq!(crash("crash-x.txt"), None);
        assert_eq!(crash("quill-debug.log"), None);
        assert_eq!(crash("panic-caught-1700.txt"), None);
        assert_eq!(caught("panic-caught-1700-1.txt"), Some((1700, 1)));
        assert_eq!(caught("crash-1700.txt"), None);
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
        // A session that never showed the notice (hidden in the tray) quits
        // cleanly: the crash is still pending next launch.
        mark_clean_exit(&root);
        assert_eq!(pending_crash(&root), Some(crash.clone()));
        acknowledge(&root, &crash);
        assert_eq!(pending_crash(&root), None);
        mark_clean_exit(&root);
        assert_eq!(pending_crash(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_from_this_run_never_become_the_notice() {
        let root = temp_root("session");
        let dir = log_dir(&root);
        write_marker(&dir, 1_000).unwrap();
        // A crash before this run (started at 3_000) and a panic on an
        // unguarded thread during it.
        write_crash_file(&dir, 2_000, "previous run", KEEP_CRASH_FILES).unwrap();
        write_crash_file(&dir, 3_500, "this run", KEEP_CRASH_FILES).unwrap();
        let before = pending_crash_before(&root, 3_000).unwrap();
        assert_eq!(before.unix_secs, 2_000, "the mid-run report is ignored");

        // Hidden in the tray all run, then a clean quit: the earlier crash
        // stays pending, the mid-run report becomes a caught one.
        mark_clean_exit_for_session(&root, 3_000, 4_000);
        assert_eq!(report_files(&dir, ReportKind::Caught).len(), 1);
        let next_run = pending_crash_before(&root, 5_000).unwrap();
        assert_eq!(next_run.unix_secs, 2_000);

        // Shown and acknowledged next run, then another mid-run report and
        // a clean quit: nothing is pending afterwards.
        acknowledge(&root, &next_run);
        write_crash_file(&dir, 5_500, "next run", KEEP_CRASH_FILES).unwrap();
        mark_clean_exit_for_session(&root, 5_000, 6_000);
        assert_eq!(pending_crash_before(&root, 7_000), None);
        assert_eq!(crash_files(&dir).len(), 1, "only the shown crash is left");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn caught_panics_are_never_pending_and_capped_on_their_own() {
        let root = temp_root("caught");
        let dir = log_dir(&root);
        write_marker(&dir, 100).unwrap();
        let crash = write_crash_file(&dir, 150, "real", KEEP_CRASH_FILES).unwrap();
        for secs in 0..(KEEP_CRASH_FILES as u64 + 3) {
            let path = write_report_file(
                &dir,
                ReportKind::Caught,
                200 + secs,
                "caught",
                KEEP_CRASH_FILES,
            )
            .unwrap();
            assert!(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("panic-caught-"))
            );
        }
        assert_eq!(
            report_files(&dir, ReportKind::Caught).len(),
            KEEP_CRASH_FILES
        );
        // Newer caught reports neither hide nor prune the real crash.
        assert_eq!(
            pending_crash(&root).map(|pending| pending.path),
            Some(crash)
        );
        acknowledge(&root, &pending_crash(&root).unwrap());
        write_report_file(&dir, ReportKind::Caught, 900, "caught", KEEP_CRASH_FILES).unwrap();
        assert_eq!(pending_crash(&root), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_kind_follows_the_guards() {
        assert_eq!(ReportKind::for_current_thread(), ReportKind::Crash);
        let inside =
            crate::thread_guard::ffi_boundary(ReportKind::Crash, ReportKind::for_current_thread);
        assert_eq!(inside, ReportKind::Caught);
        let caught = format_report(&CrashDetails {
            kind: ReportKind::Caught,
            message: "boom",
            location: None,
            thread: "worker",
            unix_secs: 1,
            backtrace: "",
        });
        assert!(caught.contains("caught; Quill kept running"));
    }

    #[test]
    fn report_has_the_fields() {
        let report = format_report(&CrashDetails {
            kind: ReportKind::Crash,
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
