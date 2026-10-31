//! Sleep until a timeout or a [`quill::wake::Signal`] poke, whichever comes
//! first, and the pacing of the poll loops that do. Replaces fixed-interval
//! polling in the UI's background tasks: the task sleeps on a long fallback
//! and wakes at once when the event it waits for happens.
#![deny(clippy::indexing_slicing)]

use gpui_kit::{BackgroundExecutor, Task};
use quill::wake::Signal;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use std::time::{Duration, Instant};

/// Wait for `timeout`, or until `signal` moved past `seen`. Take `seen`
/// from [`Signal::epoch`] before the work whose input you are waiting for,
/// so a poke during that work ends the wait immediately.
pub async fn sleep_or_wake(
    executor: &BackgroundExecutor,
    signal: &'static Signal,
    seen: u64,
    timeout: Duration,
) {
    RestTimer::default()
        .rest(executor, signal, seen, timeout)
        .await;
}

/// The fallback timer of a repeated [`sleep_or_wake`]. A GPUI timer cannot
/// be cancelled: dropped, it still fires on a worker. So the timer a poke
/// left pending serves the next rest too, unless that rest must end sooner.
#[derive(Default)]
struct RestTimer {
    armed: Option<(Instant, Task<()>)>,
}

impl RestTimer {
    async fn rest(
        &mut self,
        executor: &BackgroundExecutor,
        signal: &'static Signal,
        seen: u64,
        timeout: Duration,
    ) {
        let deadline = executor.now() + timeout;
        if !self.armed.as_ref().is_some_and(|(at, _)| *at <= deadline) {
            self.armed = Some((deadline, executor.timer(timeout)));
        }
        let mut poked = std::pin::pin!(signal.wait_since(seen));
        std::future::poll_fn(|cx| {
            // Both are polled until one is ready, so each keeps its waker.
            let timed_out = self
                .armed
                .as_mut()
                .is_none_or(|(_, timer)| Pin::new(timer).poll(cx).is_ready());
            if timed_out {
                // A finished task must not be polled again.
                self.armed = None;
            }
            let woken = poked.as_mut().poll(cx).is_ready();
            if timed_out || woken {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
    }
}

/// What a pass of a poll loop left for the next one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PassEnd {
    /// More is known to be waiting (the pass stopped at its budget).
    pub(super) backlog: bool,
    /// When timed work next comes due, if any.
    pub(super) due: Option<Duration>,
}

/// When a poll loop runs its next pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NextPass {
    /// Work is waiting: give the thread back this long, whatever arrives.
    Yield(Duration),
    /// Nothing is waiting: sleep until a poke, or this long at most.
    Rest(Duration),
}

/// How a poll loop spaces its passes.
#[derive(Clone, Copy, Debug)]
pub(super) struct PollPace {
    /// The least the thread gets back between two passes.
    pub(super) gap: Duration,
    /// The longest rest: how late work with no poke of its own can run.
    pub(super) ceiling: Duration,
}

impl PollPace {
    /// The wait after a pass that ended as `end`. `poked`: something
    /// arrived while it ran.
    pub(super) fn next(self, end: PassEnd, poked: bool) -> NextPass {
        if end.backlog || poked {
            return NextPass::Yield(self.gap);
        }
        let due = end.due.map_or(self.ceiling, |due| due.max(self.gap));
        NextPass::Rest(due.min(self.ceiling))
    }

    /// What is left of the gap after a rest that lasted `rested`.
    fn gap_left(self, rested: Duration) -> Duration {
        self.gap.saturating_sub(rested)
    }
}

/// Run `pass` until it returns `None`, paced by `pace`. Every envelope
/// pokes `signal`, and a future holds the main thread until it returns
/// `Pending`, so a wait that a poke has already ended would run pass after
/// pass with no frame or input between them. The gap is a plain timer for
/// that reason.
pub(super) async fn run_poll_loop(
    executor: BackgroundExecutor,
    signal: &'static Signal,
    pace: PollPace,
    mut pass: impl FnMut() -> Option<PassEnd>,
) {
    let mut timer = RestTimer::default();
    loop {
        // Taken before the pass, so nothing that arrives during it is missed.
        let seen = signal.epoch();
        let Some(end) = pass() else {
            break;
        };
        match pace.next(end, signal.epoch() != seen) {
            NextPass::Yield(gap) => executor.timer(gap).await,
            NextPass::Rest(longest) => {
                let began = executor.now();
                timer.rest(&executor, signal, seen, longest).await;
                let rested = executor.now().saturating_duration_since(began);
                executor.timer(pace.gap_left(rested)).await;
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "wake_wait_tests.rs"]
mod tests;
