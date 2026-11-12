//! One timer for the composer's debounced draft save. Each keystroke moves
//! the due time and token forward; a single task waits for it and commits
//! the latest token once the quiet window has passed, instead of one timer
//! task per keystroke.
#![deny(clippy::indexing_slicing)]

use gpui_kit::Task;
use std::time::{Duration, Instant};

/// What the timer task does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DraftTimerStep {
    /// Nothing is armed; the task ends.
    Idle,
    /// Not due yet; wait this long and ask again.
    Sleep(Duration),
    /// The quiet window passed; commit this token. The task ends.
    Fire(u64),
}

/// The timer is idle, or armed: a task waits on the latest keystroke's
/// token and the instant its quiet window ends. The task is held by the
/// stage, so a timer that is dropped or reset stops with it, and a stage
/// that says `Armed` always has a task behind it.
#[derive(Default)]
pub(super) enum DraftTimer {
    #[default]
    Idle,
    Armed {
        token: u64,
        due: Instant,
        _task: Task<()>,
    },
}

impl DraftTimer {
    /// A keystroke's `token` with its quiet window `delay`. When nothing is
    /// armed, `spawn` makes the task that waits (it ends by calling
    /// [`Self::step`] until that says `Idle` or `Fire`); when one is
    /// already waiting it sees the new due time and `spawn` is not called.
    pub(super) fn arm(
        &mut self,
        token: u64,
        delay: Duration,
        now: Instant,
        spawn: impl FnOnce() -> Task<()>,
    ) {
        let due = now.checked_add(delay).unwrap_or(now);
        match self {
            Self::Armed {
                token: armed,
                due: armed_due,
                ..
            } => {
                *armed = token;
                *armed_due = due;
            }
            Self::Idle => {
                *self = Self::Armed {
                    token,
                    due,
                    _task: spawn(),
                }
            }
        }
    }

    /// The task's next move at `now`. `Fire` and `Idle` leave the stage
    /// `Idle`, which drops the task: call it from inside the task, in the
    /// same update that acts on the answer.
    pub(super) fn step(&mut self, now: Instant) -> DraftTimerStep {
        let Self::Armed { token, due, .. } = self else {
            return DraftTimerStep::Idle;
        };
        if now < *due {
            return DraftTimerStep::Sleep(due.saturating_duration_since(now));
        }
        let token = *token;
        *self = Self::Idle;
        DraftTimerStep::Fire(token)
    }
}

#[cfg(test)]
mod tests {
    use super::{DraftTimer, DraftTimerStep};
    use gpui_kit::Task;
    use std::cell::Cell;
    use std::time::{Duration, Instant};

    const WINDOW: Duration = Duration::from_millis(1000);

    fn arm(timer: &mut DraftTimer, token: u64, at: Instant, spawned: &Cell<u32>) {
        timer.arm(token, WINDOW, at, || {
            spawned.set(spawned.get() + 1);
            Task::ready(())
        });
    }

    #[test]
    fn a_burst_of_keystrokes_spawns_one_task() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        let t0 = Instant::now();
        arm(&mut timer, 1, t0, &spawned);
        for (i, ms) in [100u64, 250, 400, 700].into_iter().enumerate() {
            arm(
                &mut timer,
                2 + i as u64,
                t0 + Duration::from_millis(ms),
                &spawned,
            );
        }
        assert_eq!(spawned.get(), 1, "later keystrokes reuse the task");
    }

    #[test]
    fn the_latest_keystroke_decides_when_and_what_commits() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        let t0 = Instant::now();
        arm(&mut timer, 1, t0, &spawned);
        let t1 = t0 + Duration::from_millis(600);
        arm(&mut timer, 2, t1, &spawned);
        // The first window has passed, the second has not.
        assert_eq!(
            timer.step(t0 + WINDOW),
            DraftTimerStep::Sleep(Duration::from_millis(600))
        );
        assert_eq!(timer.step(t1 + WINDOW), DraftTimerStep::Fire(2));
        assert_eq!(timer.step(t1 + WINDOW), DraftTimerStep::Idle);
    }

    #[test]
    fn a_new_window_after_a_commit_spawns_again() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        let t0 = Instant::now();
        arm(&mut timer, 1, t0, &spawned);
        assert_eq!(timer.step(t0 + WINDOW), DraftTimerStep::Fire(1));
        arm(&mut timer, 2, t0 + WINDOW, &spawned);
        assert_eq!(spawned.get(), 2);
    }

    #[test]
    fn firing_drops_the_task() {
        let mut timer = DraftTimer::default();
        let spawned = Cell::new(0);
        let t0 = Instant::now();
        arm(&mut timer, 1, t0, &spawned);
        timer.step(t0 + WINDOW);
        assert!(matches!(timer, DraftTimer::Idle));
    }
}
