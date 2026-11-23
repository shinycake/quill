// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.

//! What the timer thread (`timer_thread.rs`) does next, without the Win32 calls.

use std::time::{Duration, Instant};

use crate::timer_queue::{TimerQueue, Wait};

/// Without a high-resolution timer, waits shorter than this hold the system
/// timer at 1 ms; longer (idle) waits leave it alone.
const SHORT_WAIT: Duration = Duration::from_millis(100);

/// `WaitForSingleObject`'s `INFINITE`.
const INFINITE: u32 = u32::MAX;

pub(crate) struct TimerState<T> {
    pub(crate) timers: TimerQueue<T>,
    pub(crate) stopped: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step<T> {
    Stop,
    Fire(Vec<T>),
    /// Sleep until a timer is added.
    WaitForWake,
    WaitFor {
        duration: Duration,
        hold_system_resolution: bool,
    },
}

impl<T> TimerState<T> {
    pub(crate) fn new() -> Self {
        Self {
            timers: TimerQueue::new(),
            stopped: false,
        }
    }

    pub(crate) fn next_step(&mut self, now: Instant, high_resolution: bool) -> Step<T> {
        if self.stopped {
            return Step::Stop;
        }
        let due = self.timers.pop_due(now);
        if !due.is_empty() {
            return Step::Fire(due);
        }
        match self.timers.wait(now) {
            Wait::Forever => Step::WaitForWake,
            Wait::For(duration) => Step::WaitFor {
                duration,
                hold_system_resolution: !high_resolution && duration < SHORT_WAIT,
            },
        }
    }
}

/// `SetWaitableTimer`'s due time for `duration` from now: negative means
/// relative, in 100 ns units, and at least one unit so it never fires late
/// by being read as an absolute time.
pub(crate) fn relative_due_time(duration: Duration) -> i64 {
    let ticks = i64::try_from(duration.as_nanos() / 100).unwrap_or(i64::MAX);
    -ticks.max(1)
}

/// `WaitForSingleObject`'s timeout when the waitable timer can't be armed:
/// at least 1 ms, and never `INFINITE`.
pub(crate) fn fallback_wait_millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis())
        .unwrap_or(INFINITE - 1)
        .clamp(1, INFINITE - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_stopped_thread_stops_even_with_timers_due() {
        let now = Instant::now();
        let mut state = TimerState::new();
        state.timers.push(now, 1);
        state.stopped = true;

        assert_eq!(state.next_step(now, true), Step::Stop);
    }

    #[test]
    fn due_timers_fire_before_the_next_wait() {
        let now = Instant::now();
        let mut state = TimerState::new();
        state.timers.push(now + ms(5), 2);
        state.timers.push(now, 1);
        state.timers.push(now + ms(40), 3);

        assert_eq!(state.next_step(now + ms(5), true), Step::Fire(vec![1, 2]));
        assert_eq!(
            state.next_step(now + ms(5), true),
            Step::WaitFor {
                duration: ms(35),
                hold_system_resolution: false
            }
        );
    }

    #[test]
    fn with_nothing_pending_it_sleeps_until_woken() {
        let mut state: TimerState<u32> = TimerState::new();

        assert_eq!(state.next_step(Instant::now(), false), Step::WaitForWake);
    }

    #[test]
    fn only_a_short_wait_without_high_resolution_holds_the_system_timer() {
        let now = Instant::now();
        let waiting = |deadline, high_resolution| {
            let mut state = TimerState::new();
            state.timers.push(now + deadline, 1);
            state.next_step(now, high_resolution)
        };

        assert_eq!(
            waiting(ms(16), false),
            Step::WaitFor {
                duration: ms(16),
                hold_system_resolution: true
            }
        );
        assert_eq!(
            waiting(ms(16), true),
            Step::WaitFor {
                duration: ms(16),
                hold_system_resolution: false
            }
        );
        assert_eq!(
            waiting(ms(500), false),
            Step::WaitFor {
                duration: ms(500),
                hold_system_resolution: false
            }
        );
    }

    #[test]
    fn the_due_time_is_relative_in_100_ns_units() {
        assert_eq!(relative_due_time(ms(16)), -160_000);
        assert_eq!(relative_due_time(Duration::from_nanos(250)), -2);
    }

    #[test]
    fn a_zero_wait_still_arms_a_relative_timer() {
        assert_eq!(relative_due_time(Duration::ZERO), -1);
    }

    #[test]
    fn a_huge_wait_saturates_instead_of_wrapping() {
        assert_eq!(relative_due_time(Duration::MAX), -i64::MAX);
        assert_eq!(fallback_wait_millis(Duration::MAX), INFINITE - 1);
    }

    #[test]
    fn the_fallback_wait_is_at_least_a_millisecond() {
        assert_eq!(fallback_wait_millis(Duration::from_micros(200)), 1);
        assert_eq!(fallback_wait_millis(ms(16)), 16);
    }
}
