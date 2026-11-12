//! One timer for the composer's debounced draft save. Each keystroke
//! replaces the armed task with a new one for its own quiet window, so the
//! old task (and its sleep) is dropped; the task that wakes commits the
//! token it was armed with.
#![deny(clippy::indexing_slicing)]

use gpui_kit::Task;

/// The timer is idle, or armed: a task waits out the quiet window of the
/// latest keystroke's token. The task is held by the stage, so a timer that
/// is dropped, reset or re-armed stops the old wait, and a stage that says
/// `Armed` always has a task behind it.
#[derive(Default)]
pub(super) enum DraftTimer {
    #[default]
    Idle,
    Armed {
        token: u64,
        _task: Task<()>,
    },
}

impl DraftTimer {
    /// A keystroke's `token`: `spawn` makes the task that waits `delay` and
    /// then calls [`Self::settle`] with the token. Any earlier wait is dropped.
    pub(super) fn arm(&mut self, token: u64, spawn: impl FnOnce() -> Task<()>) {
        *self = Self::Armed {
            token,
            _task: spawn(),
        };
    }

    /// The task woke with `token`. When it is still the armed one the stage
    /// goes `Idle` (dropping the task, so call it from inside the task in
    /// the same update that acts on the answer) and the token is returned
    /// to commit.
    pub(super) fn settle(&mut self, token: u64) -> Option<u64> {
        match self {
            Self::Armed { token: armed, .. } if *armed == token => {
                *self = Self::Idle;
                Some(token)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DraftTimer;
    use gpui_kit::Task;
    use std::cell::Cell;

    fn arm(timer: &mut DraftTimer, token: u64, spawned: &Cell<u32>) {
        timer.arm(token, || {
            spawned.set(spawned.get() + 1);
            Task::ready(())
        });
    }

    #[test]
    fn the_latest_keystroke_decides_what_commits() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        arm(&mut timer, 1, &spawned);
        arm(&mut timer, 2, &spawned);
        assert_eq!(spawned.get(), 2);
        // The first keystroke's wait was dropped with its task; if it still
        // woke it would find another token armed.
        assert_eq!(timer.settle(1), None);
        assert_eq!(timer.settle(2), Some(2));
        assert_eq!(timer.settle(2), None);
    }

    #[test]
    fn firing_drops_the_task() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        arm(&mut timer, 1, &spawned);
        timer.settle(1);
        assert!(matches!(timer, DraftTimer::Idle));
    }

    #[test]
    fn a_new_window_after_a_commit_spawns_again() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        arm(&mut timer, 1, &spawned);
        timer.settle(1);
        arm(&mut timer, 2, &spawned);
        assert_eq!(spawned.get(), 2);
        assert!(matches!(timer, DraftTimer::Armed { token: 2, .. }));
    }
}
