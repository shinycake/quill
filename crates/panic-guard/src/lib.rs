//! The per-thread count of running panic guards. Quill's crash-log hook asks
//! [`panic_will_be_caught`] to tell a caught panic from a crash, so every
//! `catch_unwind` that keeps a thread alive goes through [`guarded`]: Quill's
//! own (`thread_guard`) and the vendored GPUI platform crates', which cannot
//! depend on the app.

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

thread_local! {
    static GUARDS: Cell<u32> = const { Cell::new(0) };
}

/// Whether a panic on this thread right now would be caught by a
/// [`guarded`] call instead of ending the thread or the process. Safe to
/// call from the panic hook (never panics itself).
pub fn panic_will_be_caught() -> bool {
    GUARDS.try_with(|guards| guards.get() > 0).unwrap_or(false)
}

/// Run `body` under `catch_unwind`, with the guard count raised.
pub fn guarded<R>(body: impl FnOnce() -> R) -> std::thread::Result<R> {
    let _ = GUARDS.try_with(|guards| guards.set(guards.get().saturating_add(1)));
    // `AssertUnwindSafe`: after a panic the body's captures are only
    // dropped, never observed again by this frame. State shared through
    // `Arc`s or locks is the owner's to check.
    let result = catch_unwind(AssertUnwindSafe(body));
    let _ = GUARDS.try_with(|guards| guards.set(guards.get().saturating_sub(1)));
    result
}

#[cfg(test)]
mod tests {
    use super::{guarded, panic_will_be_caught};
    use std::sync::atomic::{AtomicBool, Ordering};

    static SEEN_BY_HOOK: AtomicBool = AtomicBool::new(false);

    #[test]
    fn the_panic_hook_sees_a_guarded_panic_as_caught_and_the_count_drops_after() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {
            SEEN_BY_HOOK.store(panic_will_be_caught(), Ordering::SeqCst);
        }));
        let result = guarded(|| panic!("vsync failed"));
        std::panic::set_hook(previous);

        assert!(result.is_err());
        assert!(SEEN_BY_HOOK.load(Ordering::SeqCst));
        assert!(!panic_will_be_caught());
        assert_eq!(guarded(|| 7).ok(), Some(7));
    }
}
