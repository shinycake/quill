//! `spawn_guarded`: the one way Quill starts a thread
//! (`docs/contributing/panic-safety.md`, rule 11).
//!
//! The thread is named, and its body runs under `catch_unwind`, so a panic
//! there ends only that thread. The crash-log hook (`crate::crash_log`) has
//! already written the report by the time the unwind is caught; the owner
//! learns the thread died from `join` returning `Ok(None)`.

use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
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
    // `AssertUnwindSafe`: after a panic the thread's captures are only
    // dropped, never observed again by this thread. State it shares through
    // `Arc`s is the owner's to check, as with any dead worker.
    Builder::new()
        .name(name.into())
        .spawn(move || catch_unwind(AssertUnwindSafe(body)).ok())
}

#[cfg(test)]
mod tests {
    use super::spawn_guarded;

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
}
