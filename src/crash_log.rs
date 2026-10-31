//! Crash log: a panic hook that writes a report to
//! `<data root>/logs/crash-<unix-ts>.txt`, and the clean-exit marker the next
//! launch compares those files against (tdesktop's `tdata/working` file and
//! `CrashReports::Start` / `Finish`).
//!
//! A report holds the panic message, where it happened, the thread, the app
//! version, the OS and a backtrace. Nothing else from the session goes in it.
//! Only the newest [`KEEP_CRASH_FILES`] reports are kept.
//!
//! A crash that never reaches the panic hook (a native fault, the process
//! killed, a power cut) is caught the way tdesktop catches it with its
//! `tdata/working` file (`CrashReports::Start` / `Finish`): `logs/running`
//! is written at startup and removed on a clean quit. A file still there at
//! the next launch, with no report newer than that run, becomes a report of
//! its own. TDLib's fatal log line (verbosity 0, right before it aborts)
//! leaves `logs/tdlib-fatal` with no message text, which that report
//! mentions.
//!
//! A panic that `spawn_guarded` or `ffi_boundary` will catch
//! (`crate::thread_guard`) is filed as `panic-caught-<unix-ts>.txt`
//! instead: Quill kept running, so the next launch does not say it closed.
//! Those reports are capped separately, so a burst of them never pushes
//! out the report of a real crash.
#![deny(clippy::indexing_slicing)]

use std::backtrace::Backtrace;
use std::fs;
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
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

/// Present while a Quill process runs: process id, version, start time.
const RUNNING_FILE: &str = "running";
const NATIVE_FATAL_FILE: &str = "tdlib-fatal";
/// [`NATIVE_FATAL_FILE`]'s size, written while there is room. A full disk
/// refuses a new file (and a rename, on APFS) but not an overwrite.
const NATIVE_FATAL_SLOT: usize = 128;
const DISK_FULL_LINE: &str = "The disk was full.";
/// A caught panic from one code location is filed at most this often, so a
/// hot callback that panics on every call does not write a report per call.
const CAUGHT_REPORT_INTERVAL_SECS: u64 = 10;
/// How many panic sites the rate limiter remembers.
const MAX_CAUGHT_SITES: usize = 32;

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
    let location = info
        .location()
        .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column()));
    if kind == ReportKind::Caught {
        let site = location
            .as_deref()
            .or(current.name())
            .unwrap_or("(unknown)");
        // Checked before the backtrace is captured: that is the costly part.
        if !caught_report_due(site, now) {
            return;
        }
    }
    let report = format_report(&CrashDetails {
        kind,
        message: info
            .payload_as_str()
            .unwrap_or("(panic payload is not text)"),
        location,
        thread: current.name().unwrap_or("(unnamed)"),
        unix_secs: now,
        backtrace: &Backtrace::force_capture().to_string(),
    });
    let _ = write_report_file(&log_dir(&root), kind, now, &report, KEEP_CRASH_FILES);
}

static CAUGHT_SITES: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());

/// Whether a caught panic at `site` gets a report now (the global limiter).
/// Never blocks: the panic hook must not wait on a lock.
fn caught_report_due(site: &str, now: u64) -> bool {
    match crate::locks::try_lock_or_recover(&CAUGHT_SITES) {
        Some(mut sites) => caught_site_due(&mut sites, site, now),
        None => false,
    }
}

/// The limiter itself: true (and the site noted) when `site` has no report
/// from the last [`CAUGHT_REPORT_INTERVAL_SECS`].
fn caught_site_due(sites: &mut Vec<(String, u64)>, site: &str, now: u64) -> bool {
    if let Some((_, last)) = sites.iter_mut().find(|(known, _)| known == site) {
        if now.saturating_sub(*last) < CAUGHT_REPORT_INTERVAL_SECS {
            return false;
        }
        *last = now;
        return true;
    }
    if sites.len() >= MAX_CAUGHT_SITES {
        // Forget the site that reported longest ago.
        if let Some(oldest) = sites
            .iter()
            .enumerate()
            .min_by_key(|(_, (_, last))| *last)
            .map(|(index, _)| index)
        {
            sites.swap_remove(oldest);
        }
    }
    sites.push((site.to_owned(), now));
    true
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
    /// TDLib aborted with the disk full.
    pub disk_full: bool,
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
    (unix_secs > since).then(|| PendingCrash {
        unix_secs,
        disk_full: fs::read_to_string(&path).is_ok_and(|report| report.contains(DISK_FULL_LINE)),
        path,
    })
}

/// The previous run of Quill, as its `running` file recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RunningSentinel {
    pid: u32,
    version: String,
    started: u64,
}

impl RunningSentinel {
    fn render(&self) -> String {
        format!("{}\n{}\n{}\n", self.pid, self.version, self.started)
    }

    fn parse(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        Some(Self {
            pid: lines.next()?.trim().parse().ok()?,
            version: lines.next()?.trim().to_owned(),
            started: lines.next()?.trim().parse().ok()?,
        })
    }
}

/// Call once at startup, before [`pending_crash`]: turns the previous run's
/// leftover `running` file (and TDLib fatal breadcrumb) into a crash report
/// when that run ended without writing one, then marks this run as running.
/// Every failure is ignored.
pub fn begin_run(root: &Path) {
    begin_run_for_session(root, session_start(), std::process::id());
}

pub(crate) fn begin_run_for_session(root: &Path, session_start: u64, pid: u32) {
    let dir = log_dir(root);
    let previous = fs::read_to_string(dir.join(RUNNING_FILE))
        .ok()
        .and_then(|text| RunningSentinel::parse(&text));
    // A sentinel that cannot be parsed still proves the run did not finish.
    let unfinished = previous.is_some() || dir.join(RUNNING_FILE).exists();
    let fatal = take_native_fatal(&dir);
    // A native crash handler's report becomes a crash file of its own; the
    // unfinished-run report below then sees it and stays out of the way.
    let started = previous.as_ref().map_or(0, |run| run.started);
    let _ = crate::native_crash::promote_previous(
        &dir,
        report_date(&dir, started, session_start),
        KEEP_CRASH_FILES,
    );
    if unfinished || fatal.is_some() {
        let _ = report_unfinished_run(&dir, previous.as_ref(), fatal.as_ref(), session_start);
    }
    let sentinel = RunningSentinel {
        pid,
        version: crate::version::APP.to_owned(),
        started: session_start,
    };
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(RUNNING_FILE), sentinel.render());
}

/// Start of the UI process, after the single-instance check and before
/// GPUI, AppKit, TDLib and the video/call libraries start, so a fault in
/// their setup is covered too. A second launch never gets here, so it never
/// touches the running Quill's files. It also files the previous run's crash
/// report before the "closed unexpectedly" notice looks for it.
pub fn begin_app_run() {
    if let Some(root) = crate::settings::safe_app_root() {
        begin_run_with_native_handlers(&root);
    }
}

/// [`begin_run`], then the native crash handlers for this process. Kept
/// apart so tests can start a run without installing signal handlers.
pub fn begin_run_with_native_handlers(root: &Path) {
    begin_run(root);
    crate::native_crash::install(&log_dir(root));
}

/// File a crash report for a run that left its `running` file behind,
/// unless the run already wrote one (a panic: its report is newer than the
/// run's start).
fn report_unfinished_run(
    dir: &Path,
    previous: Option<&RunningSentinel>,
    fatal: Option<&NativeFatal>,
    session_start: u64,
) -> io::Result<()> {
    let started = previous
        .map(|run| run.started)
        .or(fatal.map(|fatal| fatal.unix_secs))
        .unwrap_or(0);
    if crash_files(dir)
        .iter()
        .any(|((unix_secs, _), _)| *unix_secs >= started)
    {
        return Ok(());
    }
    let unix_secs = report_date(dir, started, session_start);
    let report = format_unfinished_report(previous, fatal, unix_secs);
    write_report_file(dir, ReportKind::Crash, unix_secs, &report, KEEP_CRASH_FILES).map(|_| ())
}

/// Dated inside (marker, this run): pending_crash shows reports newer than
/// the last clean exit and older than this run.
fn report_date(dir: &Path, started: u64, session_start: u64) -> u64 {
    let since = read_marker(dir).unwrap_or(0);
    started
        .max(since.saturating_add(1))
        .min(session_start.saturating_sub(1))
}

fn format_unfinished_report(
    previous: Option<&RunningSentinel>,
    fatal: Option<&NativeFatal>,
    unix_secs: u64,
) -> String {
    let (version, pid, started) = match previous {
        Some(run) => (
            run.version.as_str(),
            run.pid.to_string(),
            run.started.to_string(),
        ),
        None => ("(unknown)", "(unknown)".to_owned(), "(unknown)".to_owned()),
    };
    let cause = match fatal {
        Some(fatal) => {
            use crate::disk_guard::{FLOORS, was_full};
            let disk = if was_full(fatal.free_bytes, &FLOORS) {
                format!("\n{DISK_FULL_LINE}")
            } else {
                String::new()
            };
            format!(
                "TDLib reported a fatal error at {} (unix seconds) in Quill {}, then the process ended.{disk}",
                fatal.unix_secs, fatal.version,
            )
        }
        None => "No panic report exists for that run: the process was killed, lost power, or \
                 failed in native code."
            .to_owned(),
    };
    format!(
        "Quill closed without finishing.\n\
         previous run: Quill {version}, pid {pid}, started {started} (unix seconds)\n\
         report dated: {unix_secs}\n\
         os: {os} ({arch})\n\
         \n\
         {cause}\n",
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
    )
}

/// What TDLib's fatal log callback recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeFatal {
    unix_secs: u64,
    version: String,
    free_bytes: Option<u64>,
}

fn take_native_fatal(dir: &Path) -> Option<NativeFatal> {
    let path = dir.join(NATIVE_FATAL_FILE);
    let text = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(&path);
    let mut lines = text.lines();
    Some(NativeFatal {
        unix_secs: lines.next()?.trim().parse().ok()?,
        version: lines.next()?.trim().to_owned(),
        free_bytes: lines.next().and_then(|free| free.trim().parse().ok()),
    })
}

static NATIVE_FATAL_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Where TDLib's fatal log callback leaves its breadcrumb. Call before the
/// first TDLib client is made.
pub fn arm_native_fatal_breadcrumb(root: &Path) {
    let path = log_dir(root).join(NATIVE_FATAL_FILE);
    let _ = fs::create_dir_all(log_dir(root));
    let _ = fs::write(&path, [b' '; NATIVE_FATAL_SLOT]);
    let _ = NATIVE_FATAL_PATH.set(path);
}

/// TDLib logged at verbosity 0 (fatal): it aborts right after the callback
/// returns. Leave a breadcrumb for the next launch. The message text is
/// never read: native logs can contain account data. Failures are ignored.
pub fn note_native_fatal() {
    if let Some(path) = NATIVE_FATAL_PATH.get() {
        let _ = write_native_fatal(path, unix_now());
    }
}

fn write_native_fatal(path: &Path, unix_secs: u64) -> io::Result<()> {
    let free = path.parent().and_then(crate::disk_guard::available_bytes);
    let free = free.map(|free| free.to_string()).unwrap_or_default();
    let line = format!("{unix_secs}\n{}\n{free}\n", crate::version::APP);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.write_all(format!("{line:<NATIVE_FATAL_SLOT$}").as_bytes())
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
    // The quit is clean: the next launch must not read this run as unfinished.
    let _ = fs::remove_file(dir.join(RUNNING_FILE));
    crate::native_crash::discard_active(&dir);
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
        CAUGHT_REPORT_INTERVAL_SECS, CrashDetails, KEEP_CRASH_FILES, MAX_CAUGHT_SITES,
        NATIVE_FATAL_FILE, NATIVE_FATAL_SLOT, RUNNING_FILE, ReportKind, acknowledge,
        begin_run_for_session, caught_site_due, crash_files, format_report, log_dir,
        mark_clean_exit, mark_clean_exit_for_session, parse_report_name, pending_crash,
        pending_crash_before, report_files, take_native_fatal, write_crash_file, write_marker,
        write_native_fatal, write_report_file,
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

    #[test]
    fn a_hot_panic_site_reports_once_per_interval() {
        let mut sites = Vec::new();
        assert!(caught_site_due(&mut sites, "a.rs:1:1", 100));
        for now in 100..(100 + CAUGHT_REPORT_INTERVAL_SECS) {
            assert!(!caught_site_due(&mut sites, "a.rs:1:1", now));
        }
        // Another site is independent; the first is due again after the pause.
        assert!(caught_site_due(&mut sites, "b.rs:2:2", 101));
        assert!(caught_site_due(
            &mut sites,
            "a.rs:1:1",
            100 + CAUGHT_REPORT_INTERVAL_SECS
        ));
        // The table stays bounded.
        for index in 0..(MAX_CAUGHT_SITES * 2) {
            caught_site_due(&mut sites, &format!("s{index}"), 200);
        }
        assert!(sites.len() <= MAX_CAUGHT_SITES);
    }

    #[test]
    fn a_leftover_running_file_becomes_a_pending_crash_once() {
        let root = temp_root("sentinel");
        let dir = log_dir(&root);
        write_marker(&dir, 1_000).unwrap();
        // Run A starts and is killed: its sentinel stays.
        begin_run_for_session(&root, 2_000, 11);
        assert!(dir.join(RUNNING_FILE).exists());
        assert_eq!(
            pending_crash_before(&root, 2_000),
            None,
            "first run: nothing"
        );

        // Run B finds it.
        begin_run_for_session(&root, 3_000, 12);
        let crash = pending_crash_before(&root, 3_000).expect("unfinished run reported");
        assert_eq!(crash.unix_secs, 2_000);
        let text = std::fs::read_to_string(&crash.path).unwrap();
        assert!(text.contains("pid 11"));
        acknowledge(&root, &crash);

        // Run B quits cleanly: run C sees nothing.
        mark_clean_exit_for_session(&root, 3_000, 3_500);
        assert!(!dir.join(RUNNING_FILE).exists());
        begin_run_for_session(&root, 4_000, 13);
        assert_eq!(pending_crash_before(&root, 4_000), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_panic_report_from_the_run_is_not_doubled_by_its_sentinel() {
        let root = temp_root("sentinel-panic");
        let dir = log_dir(&root);
        write_marker(&dir, 1_000).unwrap();
        begin_run_for_session(&root, 2_000, 11);
        write_crash_file(&dir, 2_400, "panic", KEEP_CRASH_FILES).unwrap();
        begin_run_for_session(&root, 3_000, 12);
        assert_eq!(crash_files(&dir).len(), 1, "no second report");
        assert_eq!(pending_crash_before(&root, 3_000).unwrap().unix_secs, 2_400);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_blank_slot_is_no_breadcrumb() {
        let root = temp_root("blank-slot");
        let dir = log_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NATIVE_FATAL_FILE), [b' '; NATIVE_FATAL_SLOT]).unwrap();
        begin_run_for_session(&root, 3_000, 12);
        assert!(crash_files(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_breadcrumb_stays_inside_its_slot() {
        let root = temp_root("slot");
        let dir = log_dir(&root);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(NATIVE_FATAL_FILE);
        std::fs::write(&path, [b' '; NATIVE_FATAL_SLOT]).unwrap();
        write_native_fatal(&path, 2_100).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            NATIVE_FATAL_SLOT as u64,
            "the overwrite allocates nothing"
        );
        let fatal = take_native_fatal(&dir).unwrap();
        assert_eq!(fatal.unix_secs, 2_100);
        assert_eq!(fatal.version, crate::version::APP);
        assert!(fatal.free_bytes.is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_fatal_on_a_full_disk_names_the_disk_and_others_do_not() {
        for (free, named) in [("1024", true), ("99999999999", false), ("", false)] {
            let root = temp_root("fatal-disk");
            let dir = log_dir(&root);
            write_marker(&dir, 1_000).unwrap();
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(NATIVE_FATAL_FILE),
                format!("2100\n1.2.3\n{free}\n"),
            )
            .unwrap();
            begin_run_for_session(&root, 3_000, 12);
            let crash = pending_crash_before(&root, 3_000).unwrap();
            assert_eq!(crash.disk_full, named, "free: {free:?}");
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn the_tdlib_fatal_breadcrumb_is_reported_and_consumed() {
        let root = temp_root("fatal");
        let dir = log_dir(&root);
        write_marker(&dir, 1_000).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(NATIVE_FATAL_FILE), "2100\n1.2.3\n").unwrap();
        begin_run_for_session(&root, 3_000, 12);
        assert!(!dir.join(NATIVE_FATAL_FILE).exists(), "consumed");
        let crash = pending_crash_before(&root, 3_000).unwrap();
        let text = std::fs::read_to_string(crash.path).unwrap();
        assert!(text.contains("TDLib reported a fatal error"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
