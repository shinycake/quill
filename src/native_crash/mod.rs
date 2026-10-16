//! Native crash handlers, like tdesktop's `core/crash_reports.cpp`: a crash
//! in native code (TDLib, ntgcalls, FFmpeg, AppKit, a GPU driver) never
//! reaches the Rust panic hook, so these handlers leave a report of their
//! own.
//!
//! * Unix (`unix.rs`): `sigaction` for SIGSEGV, SIGBUS, SIGILL, SIGFPE and
//!   SIGABRT on an alternate signal stack. The handler only calls `write(2)`
//!   and `backtrace(3)` on a file opened at startup, restores the default
//!   action and re-raises, so the OS crash reporter still runs.
//! * Windows (`windows.rs`): an unhandled-exception filter writing the
//!   exception code, its address and a minidump, then chaining to the
//!   previous filter.
//!
//! Both write `native-crash-active.txt` (and `.dmp` on Windows) in the logs
//! folder. A clean quit removes them ([`discard_active`]). At the next start
//! [`promote_previous`] turns what is left into a regular `crash-<ts>.txt`
//! (plus `crash-<ts>.dmp`), which the "closed unexpectedly" notice already
//! points at. Only the newest [`crate::crash_log::KEEP_CRASH_FILES`] reports
//! stay, with their dumps.
//!
//! Limits: `sigaltstack` is per thread, so only the installing thread (the
//! main thread) survives a stack overflow; a fault on another thread runs
//! the handler on that thread's own stack. The Rust runtime's stack-overflow
//! message is replaced by the report.
#![deny(clippy::indexing_slicing)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;

/// The text report of the running process.
pub const ACTIVE_REPORT_FILE: &str = "native-crash-active.txt";
/// The Windows minidump of the running process.
pub const ACTIVE_DUMP_FILE: &str = "native-crash-active.dmp";
const DUMP_SUFFIX: &str = ".dmp";

/// Install the handlers for this process; reports go into `dir`. Call it
/// after [`promote_previous`] has dealt with the previous run's files. Every
/// failure is ignored: running without handlers is the old behavior.
pub fn install(dir: &Path) {
    let _ = fs::create_dir_all(dir);
    #[cfg(unix)]
    unix::install(dir);
    #[cfg(windows)]
    windows::install(dir);
    #[cfg(not(any(unix, windows)))]
    let _ = dir;
}

/// A clean quit: the handlers' files are not a crash.
pub fn discard_active(dir: &Path) {
    let _ = fs::remove_file(dir.join(ACTIVE_REPORT_FILE));
    let _ = fs::remove_file(dir.join(ACTIVE_DUMP_FILE));
}

/// File the previous run's native crash report, if it left one, as
/// `crash-<unix_secs>.txt` (its minidump becomes `crash-<unix_secs>.dmp`),
/// and drop dumps whose report was pruned. Returns the report's path.
/// Empty files (a run that did not crash) are removed.
pub fn promote_previous(dir: &Path, unix_secs: u64, keep: usize) -> io::Result<Option<PathBuf>> {
    let text_path = dir.join(ACTIVE_REPORT_FILE);
    let dump_path = dir.join(ACTIVE_DUMP_FILE);
    let report = match fs::read(&text_path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    };
    if report.trim().is_empty() {
        discard_active(dir);
        return Ok(None);
    }
    let report_path = crate::crash_log::write_crash_file(dir, unix_secs, &report, keep)?;
    if dump_path.exists() {
        let _ = fs::rename(&dump_path, report_path.with_extension("dmp"));
    }
    discard_active(dir);
    prune_orphan_dumps(dir);
    Ok(Some(report_path))
}

/// Remove `crash-*.dmp` files whose `crash-*.txt` report is gone.
fn prune_orphan_dumps(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let is_dump = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("crash-") && name.ends_with(DUMP_SUFFIX));
        if is_dump && !path.with_extension("txt").exists() {
            let _ = fs::remove_file(path);
        }
    }
}
