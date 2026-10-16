//! Cross-thread wake-ups for the UI's background tasks.
//!
//! Quill's periodic tasks (the TDLib poll loop, the deep-link inbox, the
//! tray, the OS menus, the passcode timer, the battery probe) used to wake
//! the main thread on a fixed tick even when nothing happened. A
//! [`Signal`] lets the thread that produces an event (the TDLib receive
//! thread, the single-instance listener, a tray callback) poke the task that
//! cares, so the task can sleep on a long fallback timeout instead.
//!
//! A signal is an epoch counter plus the wakers of whoever waits. A waiter
//! remembers the epoch it last saw and asks for [`Signal::wait_since`]: the
//! future is ready as soon as the epoch moved, including a poke that landed
//! between two waits, so no wake-up is lost. Spurious wake-ups are harmless:
//! every waiter re-checks its own state.
#![deny(clippy::indexing_slicing)]

use crate::locks::lock_or_recover;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll, Waker};

/// An epoch counter with a list of waiting tasks.
pub struct Signal {
    epoch: AtomicU64,
    wakers: Mutex<Vec<Waker>>,
}

impl Signal {
    pub const fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            wakers: Mutex::new(Vec::new()),
        }
    }

    /// The current epoch; pass it to [`Signal::wait_since`] later.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Wake every waiter. Cheap and safe from any thread.
    pub fn poke(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let wakers = std::mem::take(&mut *lock_or_recover(&self.wakers));
        for waker in wakers {
            waker.wake();
        }
    }

    /// Ready once the epoch differs from `seen`; yields the new epoch.
    pub fn wait_since(&self, seen: u64) -> WaitSince<'_> {
        WaitSince { signal: self, seen }
    }
}

impl Default for Signal {
    fn default() -> Self {
        Self::new()
    }
}

pub struct WaitSince<'a> {
    signal: &'a Signal,
    seen: u64,
}

impl Future for WaitSince<'_> {
    type Output = u64;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u64> {
        let now = self.signal.epoch();
        if now != self.seen {
            return Poll::Ready(now);
        }
        {
            let mut wakers = lock_or_recover(&self.signal.wakers);
            if !wakers.iter().any(|w| w.will_wake(cx.waker())) {
                wakers.push(cx.waker().clone());
            }
        }
        // A poke between the check above and the registration would have
        // drained a list without us in it.
        let now = self.signal.epoch();
        if now != self.seen {
            Poll::Ready(now)
        } else {
            Poll::Pending
        }
    }
}

/// TDLib updates (or any other input the poll loop drains) arrived.
pub static UPDATES: Signal = Signal::new();
/// A link, activation or quit request arrived from outside the window.
pub static LINKS: Signal = Signal::new();
/// Something the tray, the dock/app menus or the badge show may have
/// changed (applied updates, window activation, tray menu clicks).
pub static CHROME: Signal = Signal::new();
/// Passcode or auto-lock settings changed, or the app locked or unlocked.
pub static PASSCODE: Signal = Signal::new();
/// The "save power on low battery" switch changed.
pub static BATTERY: Signal = Signal::new();

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::task::Wake;
    use std::time::{Duration, Instant};

    struct Unpark(std::thread::Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    /// Block on a wait with a deadline; returns how long it took.
    fn block_until_ready(signal: &Signal, seen: u64) -> Option<Duration> {
        let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut wait = Box::pin(signal.wait_since(seen));
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if wait.as_mut().poll(&mut cx).is_ready() {
                return Some(start.elapsed());
            }
            std::thread::park_timeout(Duration::from_secs(5));
        }
        None
    }

    #[test]
    fn a_poke_before_the_wait_is_not_lost() {
        let signal = Signal::new();
        let seen = signal.epoch();
        signal.poke();
        assert!(block_until_ready(&signal, seen).is_some());
    }

    #[test]
    fn a_poke_from_another_thread_wakes_a_sleeping_waiter_at_once() {
        static SIGNAL: Signal = Signal::new();
        let seen = SIGNAL.epoch();
        let poker = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            SIGNAL.poke();
        });
        let waited = block_until_ready(&SIGNAL, seen);
        let _ = poker.join();
        let waited = waited.unwrap_or(Duration::from_secs(99));
        assert!(waited < Duration::from_millis(1000), "waited {waited:?}");
    }
}
