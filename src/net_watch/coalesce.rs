//! One more run of some work for any number of calls that arrive while it
//! runs. Windows sends one network change as dozens of callbacks across
//! its three registrations, on threads of its own.

use std::sync::atomic::{AtomicBool, Ordering};

pub(super) struct Coalesce {
    dirty: AtomicBool,
    running: AtomicBool,
}

/// Clears `running` even when the work panics.
struct Running<'a>(&'a AtomicBool);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl Coalesce {
    pub(super) const fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            running: AtomicBool::new(false),
        }
    }

    /// Runs `work` now, or has the run under way run it once more when it
    /// is done.
    pub(super) fn run(&self, mut work: impl FnMut()) {
        self.dirty.store(true, Ordering::Release);
        loop {
            if self.running.swap(true, Ordering::AcqRel) {
                return;
            }
            {
                let _running = Running(&self.running);
                while self.dirty.swap(false, Ordering::AcqRel) {
                    work();
                }
            }
            // A call that found the run under way after its last look.
            if !self.dirty.load(Ordering::Acquire) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Coalesce;
    use std::cell::Cell;

    #[test]
    fn calls_during_a_run_make_one_more_run_not_one_each() {
        let changes = Coalesce::new();
        let runs = Cell::new(0);
        let depth = Cell::new(0);
        let work = || {
            depth.set(depth.get() + 1);
            assert_eq!(depth.get(), 1, "never inside another run");
            runs.set(runs.get() + 1);
            if runs.get() == 1 {
                // Callbacks from other threads land while this run looks.
                for _ in 0..30 {
                    changes.run(|| panic!("the run under way takes this"));
                }
            }
            depth.set(depth.get() - 1);
        };
        changes.run(work);
        assert_eq!(runs.get(), 2);
        changes.run(|| runs.set(runs.get() + 1));
        assert_eq!(runs.get(), 3, "a later call runs again");
    }

    #[test]
    fn a_run_that_panics_does_not_stop_later_ones() {
        let changes = Coalesce::new();
        let failed = std::panic::catch_unwind(|| changes.run(|| panic!("route lookup")));
        assert!(failed.is_err());
        let ran = Cell::new(false);
        changes.run(|| ran.set(true));
        assert!(ran.get());
    }
}
