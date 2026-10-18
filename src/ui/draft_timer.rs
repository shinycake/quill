//! One timer for the composer's debounced draft save. Each keystroke moves
//! the due time and token forward; a single task waits for it and commits
//! the latest token once the quiet window has passed, instead of one timer
//! task per keystroke.
#![deny(clippy::indexing_slicing)]

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

#[derive(Debug, Default)]
pub(super) struct DraftTimer {
    /// The latest keystroke's token and the instant its window ends.
    due: Option<(u64, Instant)>,
    /// Whether a task is waiting on `due`.
    running: bool,
}

impl DraftTimer {
    /// A keystroke's `token` with its quiet window `delay`. True when the
    /// caller must spawn the task; false when one is already waiting and
    /// will see the new due time.
    pub(super) fn arm(&mut self, token: u64, delay: Duration, now: Instant) -> bool {
        self.due = Some((token, now.checked_add(delay).unwrap_or(now)));
        !std::mem::replace(&mut self.running, true)
    }

    /// The task's next move at `now`.
    pub(super) fn step(&mut self, now: Instant) -> DraftTimerStep {
        let Some((token, due)) = self.due else {
            self.running = false;
            return DraftTimerStep::Idle;
        };
        if now < due {
            return DraftTimerStep::Sleep(due.saturating_duration_since(now));
        }
        self.due = None;
        self.running = false;
        DraftTimerStep::Fire(token)
    }
}

#[cfg(test)]
mod tests {
    use super::{DraftTimer, DraftTimerStep};
    use std::time::{Duration, Instant};

    const WINDOW: Duration = Duration::from_millis(1000);

    #[test]
    fn a_burst_of_keystrokes_spawns_one_task() {
        let mut timer = DraftTimer::default();
        let t0 = Instant::now();
        assert!(timer.arm(1, WINDOW, t0), "the first keystroke spawns");
        for (i, ms) in [100u64, 250, 400, 700].into_iter().enumerate() {
            let at = t0 + Duration::from_millis(ms);
            assert!(!timer.arm(2 + i as u64, WINDOW, at), "later ones reuse it");
        }
    }

    #[test]
    fn the_latest_keystroke_decides_when_and_what_commits() {
        let mut timer = DraftTimer::default();
        let t0 = Instant::now();
        timer.arm(1, WINDOW, t0);
        let t1 = t0 + Duration::from_millis(600);
        timer.arm(2, WINDOW, t1);
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
        let t0 = Instant::now();
        assert!(timer.arm(1, WINDOW, t0));
        assert_eq!(timer.step(t0 + WINDOW), DraftTimerStep::Fire(1));
        assert!(timer.arm(2, WINDOW, t0 + WINDOW));
    }
}
