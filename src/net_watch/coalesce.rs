//! One more run of some work for any number of calls that arrive while it
//! runs. Windows sends one network change as dozens of callbacks across
//! its three registrations, on threads of its own.

use std::sync::atomic::{AtomicU8, Ordering};

/// No run is under way.
const IDLE: u8 = 0;
/// A run is under way and no call came after it began.
const RUNNING: u8 = 1;
/// A run is under way and a call came after it began: it runs once more.
const RUNNING_DIRTY: u8 = 2;

/// One value, so "a run is under way" and "one more is wanted" cannot
/// disagree.
pub(super) struct Coalesce {
    state: AtomicU8,
}

/// Ends the run when the work panics. A run that ends well hands the state
/// back itself (see [`Coalesce::run`]), so the guard is disarmed first.
struct Running<'a>(Option<&'a AtomicU8>);

impl Running<'_> {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.0 {
            state.store(IDLE, Ordering::Release);
        }
    }
}

impl Coalesce {
    pub(super) const fn new() -> Self {
        Self {
            state: AtomicU8::new(IDLE),
        }
    }

    /// Runs `work` now, or has the run under way run it once more when it
    /// is done.
    pub(super) fn run(&self, mut work: impl FnMut()) {
        loop {
            match self
                .state
                .compare_exchange(IDLE, RUNNING, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                // Under way: ask it for one more run. It may have ended
                // meanwhile, then this call runs it itself.
                Err(RUNNING) => {
                    if self
                        .state
                        .compare_exchange(
                            RUNNING,
                            RUNNING_DIRTY,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
        let mut running = Running(Some(&self.state));
        loop {
            work();
            // A call that came during the work wants one more run.
            if self
                .state
                .compare_exchange(RUNNING, IDLE, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                running.disarm();
                return;
            }
            self.state.store(RUNNING, Ordering::Release);
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
