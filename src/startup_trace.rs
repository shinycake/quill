//! Launch milestones for timing startup. With `QUILL_STARTUP_TRACE=<path>`
//! set, the first time each milestone is reached appends
//! `<label> <unix time in µs>` to `<path>`; a launch harness compares those
//! against the moment it started the process. Unset, `mark` is one atomic
//! load after the first call.
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

struct Trace {
    file: std::fs::File,
    seen: Vec<&'static str>,
}

static TRACE: OnceLock<Option<Mutex<Trace>>> = OnceLock::new();

fn trace() -> Option<&'static Mutex<Trace>> {
    TRACE
        .get_or_init(|| {
            let path = std::env::var_os("QUILL_STARTUP_TRACE")?;
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()?;
            let mut trace = Trace {
                file,
                seen: Vec::new(),
            };
            // The kernel's start time of this process, before dyld loaded
            // anything: `main` minus `exec` is the time spent before main.
            if let Some(micros) = process_start_micros() {
                let _ = writeln!(trace.file, "exec {micros}");
                trace.seen.push("exec");
            }
            Some(Mutex::new(trace))
        })
        .as_ref()
}

/// Whether milestones are being recorded, so a caller can skip the work
/// of deciding whether one was reached.
pub fn enabled() -> bool {
    trace().is_some()
}

/// Record `label` the first time it is reached in this process; true
/// that first time.
pub fn mark(label: &'static str) -> bool {
    let Some(trace) = trace() else {
        return false;
    };
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_micros());
    let mut trace = trace.lock_or_recover();
    if trace.seen.contains(&label) {
        return false;
    }
    trace.seen.push(label);
    let _ = writeln!(trace.file, "{label} {micros}");
    true
}

/// Whether `label` has been recorded.
pub fn reached(label: &'static str) -> bool {
    trace().is_some_and(|trace| trace.lock_or_recover().seen.contains(&label))
}

/// A frame went to the compositor: the first one is `presented`, and the
/// first after the chat list frame is `chat_list_presented`.
pub fn frame_presented() {
    if !enabled() {
        return;
    }
    mark("presented");
    if reached("chat_list") {
        mark("chat_list_presented");
    }
}

/// When the kernel started this process, in µs since the Unix epoch.
#[cfg(target_os = "macos")]
#[allow(unsafe_code, reason = "proc_pidinfo fills a plain C struct")]
fn process_start_micros() -> Option<u128> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: `info` is a writable buffer of exactly `size` bytes for the
    // PROC_PIDTBSDINFO flavor, and proc_pidinfo writes at most that many.
    let written = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: proc_pidinfo filled the whole struct (checked above).
    let info = unsafe { info.assume_init() };
    Some(u128::from(info.pbi_start_tvsec) * 1_000_000 + u128::from(info.pbi_start_tvusec))
}

#[cfg(not(target_os = "macos"))]
fn process_start_micros() -> Option<u128> {
    None
}
