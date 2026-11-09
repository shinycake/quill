//! settle-probe direct|settle LIBRARY
//!
//! Loads LIBRARY on a worker thread the way Quill's early-connect worker
//! loads TDLib, without (`direct`) or with (`settle`) `first_load::settle`
//! first, while the main thread takes the dynamic loader's lock once a
//! millisecond, as AppKit's start-up does. Prints how long the worker's two
//! steps took and the longest the main thread was held.
//!
//! The wait only shows when the system has forgotten LIBRARY and LIBRARY is
//! inside a notarized bundle: point it at a release Quill.app's
//! `Contents/Frameworks/libtdjson.dylib` after
//! `python3 -c 'import runpy; runpy.run_path("scripts/launch-bench.py")["recycle"]()'`,
//! and sign the program with the same team's identity and `--options runtime`
//! so it may load the library. docs/decisions/launch-first-load-check.md
#[path = "../../../../src/first_load.rs"]
mod first_load;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static LOADED: AtomicBool = AtomicBool::new(false);

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(mode), Some(library)) = (args.next(), args.next()) else {
        eprintln!("usage: settle-probe direct|settle LIBRARY");
        std::process::exit(2);
    };
    let library = std::path::PathBuf::from(library);
    let worker = std::thread::spawn(move || {
        let start = Instant::now();
        if mode == "settle" {
            first_load::settle(&library);
        }
        let settled = start.elapsed();
        // SAFETY: LIBRARY is the caller's choice of a library to load.
        let loaded = unsafe { libloading::Library::new(&library) };
        let opened = start.elapsed().saturating_sub(settled);
        LOADED.store(true, Ordering::SeqCst);
        // Never unloaded, as in Quill.
        std::mem::forget(loaded);
        (settled, opened)
    });
    let mut longest = Duration::ZERO;
    while !LOADED.load(Ordering::SeqCst) {
        let start = Instant::now();
        // SAFETY: a library every process already has: this takes the
        // loader's lock and loads nothing.
        let handle =
            unsafe { libc::dlopen(c"/usr/lib/libSystem.B.dylib".as_ptr(), libc::RTLD_LAZY) };
        if !handle.is_null() {
            // SAFETY: the handle just returned.
            unsafe { libc::dlclose(handle) };
        }
        longest = longest.max(start.elapsed());
        std::thread::sleep(Duration::from_millis(1));
    }
    let (settled, opened) = worker.join().unwrap_or_default();
    println!(
        "settle {:4.0} ms  dlopen {:4.0} ms  main thread held at most {:4.0} ms",
        settled.as_secs_f64() * 1e3,
        opened.as_secs_f64() * 1e3,
        longest.as_secs_f64() * 1e3
    );
}
