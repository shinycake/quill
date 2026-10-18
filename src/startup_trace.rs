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
            Some(Mutex::new(Trace {
                file,
                seen: Vec::new(),
            }))
        })
        .as_ref()
}

/// Whether milestones are being recorded, so a caller can skip the work
/// of deciding whether one was reached.
pub fn enabled() -> bool {
    trace().is_some()
}

/// Record `label` the first time it is reached in this process.
pub fn mark(label: &'static str) {
    let Some(trace) = trace() else {
        return;
    };
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_micros());
    let mut trace = trace.lock_or_recover();
    if trace.seen.contains(&label) {
        return;
    }
    trace.seen.push(label);
    let _ = writeln!(trace.file, "{label} {micros}");
}
