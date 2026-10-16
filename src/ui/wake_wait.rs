//! Sleep until a timeout or a [`quill::wake::Signal`] poke, whichever comes
//! first. Replaces fixed-interval polling in the UI's background tasks:
//! the task sleeps on a long fallback and wakes at once when the event it
//! waits for happens.
#![deny(clippy::indexing_slicing)]

use gpui_kit::BackgroundExecutor;
use quill::wake::Signal;
use std::future::Future;
use std::time::Duration;

/// Wait for `timeout`, or until `signal` moved past `seen`. Take `seen`
/// from [`Signal::epoch`] before the work whose input you are waiting for,
/// so a poke during that work ends the wait immediately.
pub async fn sleep_or_wake(
    executor: &BackgroundExecutor,
    signal: &'static Signal,
    seen: u64,
    timeout: Duration,
) {
    let mut timer = std::pin::pin!(executor.timer(timeout));
    let mut poked = std::pin::pin!(signal.wait_since(seen));
    std::future::poll_fn(|cx| {
        // Both are polled until one is ready, so each keeps its waker.
        let timed_out = timer.as_mut().poll(cx).is_ready();
        let woken = poked.as_mut().poll(cx).is_ready();
        if timed_out || woken {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
}
