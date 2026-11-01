//! Panic containment: `spawn_guarded`, the one way Quill starts a thread
//! (`docs/contributing/panic-safety.md`, rule 11), and `ffi_boundary`, the
//! wrapper around every Rust function C calls (rule 9).
//!
//! Both run their body under `catch_unwind`, so a panic there ends only that
//! thread, or only that callback. The crash-log hook (`crate::crash_log`)
//! has already written the report by the time the unwind is caught; it asks
//! [`panic_will_be_caught`] so it can file the report as a caught panic,
//! which does not bring up the "closed unexpectedly" notice next launch.
//! The count lives in `quill-panic-guard`, which the vendored Windows
//! platform crate's vsync thread uses too.
#![deny(clippy::indexing_slicing)]

use quill_panic_guard::guarded;
pub use quill_panic_guard::panic_will_be_caught;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{Builder, JoinHandle};

/// Spawn a named thread that runs `body`. `join` yields `Some(result)` when
/// `body` returned and `None` when it panicked. Spawning itself can fail
/// (out of threads or memory), which `std::thread::spawn` would turn into a
/// panic on the caller's thread.
pub fn spawn_guarded<T, F>(name: impl Into<String>, body: F) -> io::Result<JoinHandle<Option<T>>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Builder::new()
        .name(name.into())
        .spawn(move || guarded(body).ok())
}

/// Run the body of an `extern "C"` / `extern "system"` function Quill
/// implements. A panic must not unwind into the C caller (that aborts the
/// process), so it is caught here and the callback returns `default`. The
/// panic hook has already logged it.
pub fn ffi_boundary<R>(default: R, body: impl FnOnce() -> R) -> R {
    guarded(body).unwrap_or(default)
}

/// One of at most `max` places in a counted pool of worker threads (OS
/// notification helpers that wait for a click). The place is given back when
/// the guard drops: on return, on a panic that unwinds through the thread,
/// and when the thread never started and its closure is dropped.
pub struct InflightSlot(Arc<AtomicUsize>);

impl InflightSlot {
    /// Take a place, or `None` when `max` are in use.
    pub fn acquire(counter: &Arc<AtomicUsize>, max: usize) -> Option<Self> {
        if counter.fetch_add(1, Ordering::SeqCst) >= max {
            counter.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(Arc::clone(counter)))
    }
}

impl Drop for InflightSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::{InflightSlot, ffi_boundary, panic_will_be_caught, spawn_guarded};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn slots_are_capped_and_returned_on_drop() {
        let counter = Arc::new(AtomicUsize::new(0));
        let a = InflightSlot::acquire(&counter, 2);
        let b = InflightSlot::acquire(&counter, 2);
        assert!(a.is_some() && b.is_some());
        assert!(InflightSlot::acquire(&counter, 2).is_none());
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "a refusal gives back its count"
        );
        drop(a);
        assert!(InflightSlot::acquire(&counter, 2).is_some());
    }

    #[test]
    fn a_panicking_thread_gives_its_slot_back() {
        let counter = Arc::new(AtomicUsize::new(0));
        let slot = InflightSlot::acquire(&counter, 1);
        let handle = spawn_guarded("quill-test-slot-panic", move || {
            let _slot = slot;
            panic!("boom");
        })
        .unwrap();
        assert_eq!(handle.join().unwrap(), None::<()>);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn returns_the_value_and_the_name() {
        let handle = spawn_guarded("quill-test-ok", || {
            std::thread::current().name().map(str::to_owned)
        })
        .unwrap();
        assert_eq!(handle.join().unwrap(), Some(Some("quill-test-ok".into())));
    }

    #[test]
    fn a_panic_becomes_none() {
        let handle = spawn_guarded("quill-test-panic", || -> u32 { panic!("boom") }).unwrap();
        assert_eq!(handle.join().unwrap(), None);
    }

    #[test]
    fn ffi_boundary_returns_the_body_value() {
        assert_eq!(ffi_boundary(0, || 7), 7);
    }

    #[test]
    fn ffi_boundary_turns_a_panic_into_the_default() {
        let value = ffi_boundary(-1, || -> i32 { panic!("callback failed") });
        assert_eq!(value, -1);
        // Unit callbacks (most trampolines) just return.
        ffi_boundary((), || panic!("unit callback failed"));
    }

    #[test]
    fn guards_mark_panics_as_caught_only_inside() {
        assert!(!panic_will_be_caught());
        assert!(ffi_boundary(false, panic_will_be_caught));
        assert!(ffi_boundary(false, || ffi_boundary(
            false,
            panic_will_be_caught
        )));
        ffi_boundary((), || panic!("unwinds out of the guard"));
        assert!(!panic_will_be_caught(), "the count drops after a panic too");
        let handle = spawn_guarded("quill-test-caught", panic_will_be_caught).unwrap();
        assert_eq!(handle.join().unwrap(), Some(true));
    }
}
