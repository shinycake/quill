//! Panic containment: `spawn_guarded`, the one way Quill starts a thread
//! (`docs/contributing/panic-safety.md`, rule 11), and `ffi_boundary`, the
//! wrapper around every Rust function C calls (rule 9).
//!
//! Both run their body under `catch_unwind`, so a panic there ends only that
//! thread, or only that callback. The crash-log hook (`crate::crash_log`)
//! has already written the report by the time the unwind is caught; it asks
//! [`panic_will_be_caught`] so it can file the report as a caught panic,
//! which does not bring up the "closed unexpectedly" notice next launch.

use std::cell::Cell;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::{Builder, JoinHandle};

thread_local! {
    /// How many guards (`spawn_guarded` bodies, `ffi_boundary` calls) are
    /// running on this thread.
    static GUARDS: Cell<u32> = const { Cell::new(0) };
}

/// Whether a panic on this thread right now would be caught by
/// `spawn_guarded` or `ffi_boundary` instead of ending the process. Safe to
/// call from the panic hook (never panics itself).
pub fn panic_will_be_caught() -> bool {
    GUARDS.try_with(|guards| guards.get() > 0).unwrap_or(false)
}

/// Run `body` under `catch_unwind`, with the guard count raised.
fn guarded<R>(body: impl FnOnce() -> R) -> std::thread::Result<R> {
    let _ = GUARDS.try_with(|guards| guards.set(guards.get().saturating_add(1)));
    // `AssertUnwindSafe`: after a panic the body's captures are only
    // dropped, never observed again by this frame. State shared through
    // `Arc`s or locks is the owner's to check, and locks recover from
    // poisoning (`crate::locks`).
    let result = catch_unwind(AssertUnwindSafe(body));
    let _ = GUARDS.try_with(|guards| guards.set(guards.get().saturating_sub(1)));
    result
}

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

#[cfg(test)]
mod tests {
    use super::{ffi_boundary, panic_will_be_caught, spawn_guarded};

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
