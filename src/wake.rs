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
use std::thread::ThreadId;

thread_local! {
    static ISOLATED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Test apps only: from now on this thread's waiters are woken only by
/// pokes from this same thread, and its pokes wake only its own waiters.
///
/// The signals are process-wide statics shared by every app in a test
/// process. A GPUI test scheduler allows activity only from the thread that
/// owns it, so a poke from one test (a window activation) that woke
/// another test's task panicked that test as "not deterministic". Pokes
/// from threads that were never marked (helper threads) still wake every
/// waiter, marked or not. The app never calls this; its pokes from the
/// TDLib thread, the tray and the single-instance listener wake everything
/// as before.
pub fn isolate_current_thread() {
    let _ = ISOLATED.try_with(|isolated| isolated.set(true));
}

fn isolated_thread() -> Option<ThreadId> {
    ISOLATED
        .try_with(|isolated| isolated.get())
        .unwrap_or(false)
        .then(|| std::thread::current().id())
}

#[cfg(any(test, feature = "test-support"))]
mod poked_here {
    use super::Signal;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        static POKES: RefCell<HashMap<usize, u64>> = RefCell::new(HashMap::new());
    }

    fn key(signal: &Signal) -> usize {
        std::ptr::from_ref(signal) as usize
    }

    pub(super) fn count(signal: &Signal) {
        let _ = POKES.try_with(|pokes| {
            if let Ok(mut pokes) = pokes.try_borrow_mut() {
                *pokes.entry(key(signal)).or_default() += 1;
            }
        });
    }

    pub(super) fn get(signal: &Signal) -> u64 {
        POKES
            .try_with(|pokes| {
                pokes
                    .try_borrow()
                    .ok()
                    .and_then(|pokes| pokes.get(&key(signal)).copied())
            })
            .ok()
            .flatten()
            .unwrap_or(0)
    }
}

/// An epoch counter with a list of waiting tasks.
pub struct Signal {
    epoch: AtomicU64,
    wakers: Mutex<Vec<(Waker, Option<ThreadId>)>>,
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
        #[cfg(any(test, feature = "test-support"))]
        poked_here::count(self);
        let me = isolated_thread();
        let woken = {
            let mut wakers = lock_or_recover(&self.wakers);
            match me {
                // Production: nothing is marked, so everything wakes.
                None => std::mem::take(&mut *wakers),
                // Partition under one lock so no waiter is ever absent.
                Some(_) => {
                    let (mine, others): (Vec<_>, Vec<_>) = std::mem::take(&mut *wakers)
                        .into_iter()
                        .partition(|(_, owner)| *owner == me);
                    *wakers = others;
                    mine
                }
            }
        };
        for (waker, _) in woken {
            waker.wake();
        }
    }

    /// Tests: how many times this thread has poked the signal. The epoch
    /// also counts pokes from tests running in parallel, so only this
    /// count shows that the code under test poked.
    #[cfg(any(test, feature = "test-support"))]
    pub fn pokes_on_this_thread(&self) -> u64 {
        poked_here::get(self)
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
            if !wakers.iter().any(|(w, _)| w.will_wake(cx.waker())) {
                wakers.push((cx.waker().clone(), isolated_thread()));
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

/// TDLib updates for the account on screen (or any other input its poll
/// loop drains) arrived.
pub static UPDATES: Signal = Signal::new();
/// TDLib updates for an account in the background arrived.
pub static BACKGROUND_UPDATES: Signal = Signal::new();
/// A link, activation or quit request arrived from outside the window.
pub static LINKS: Signal = Signal::new();
/// Something the tray, the dock/app menus or the badge show may have
/// changed (applied updates, window activation, tray menu clicks).
pub static CHROME: Signal = Signal::new();
/// Passcode or auto-lock settings changed, or the app locked or unlocked.
pub static PASSCODE: Signal = Signal::new();
/// The "save power on low battery" switch changed.
pub static BATTERY: Signal = Signal::new();
/// Agent access changed: a grant, a connection, or a prompt for the user
/// (`crate::agent`).
pub static AGENT: Signal = Signal::new();

/// The main window came to the front (or the OS resumed): everything that
/// sleeps on a long timeout re-checks now, since monotonic timers pause in
/// system sleep and time-driven work (presence, auto-lock) may be overdue.
pub fn poke_on_activation() {
    UPDATES.poke();
    BACKGROUND_UPDATES.poke();
    PASSCODE.poke();
    crate::icon_badge::invalidate_icon_badge();
}

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

    #[test]
    fn a_thread_counts_only_its_own_pokes() {
        static SIGNAL: Signal = Signal::new();
        let other = std::thread::spawn(|| SIGNAL.poke());
        let _ = other.join();
        assert_eq!(SIGNAL.pokes_on_this_thread(), 0);
        SIGNAL.poke();
        assert_eq!(SIGNAL.pokes_on_this_thread(), 1);
        assert_eq!(SIGNAL.epoch(), 2);
    }

    #[test]
    fn activation_wakes_the_poll_loop_and_the_passcode_timer() {
        let (updates, passcode, chrome) = (UPDATES.epoch(), PASSCODE.epoch(), CHROME.epoch());
        poke_on_activation();
        assert!(UPDATES.epoch() != updates);
        assert!(PASSCODE.epoch() != passcode);
        assert!(CHROME.epoch() != chrome);
    }

    struct Flag(std::sync::atomic::AtomicBool);
    impl Wake for Flag {
        fn wake(self: Arc<Self>) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// Register a waiter on the current thread; returns its wake flag.
    fn park_waiter(signal: &Signal) -> Arc<Flag> {
        let flag = Arc::new(Flag(std::sync::atomic::AtomicBool::new(false)));
        let waker = Waker::from(flag.clone());
        let mut cx = Context::from_waker(&waker);
        let mut wait = Box::pin(signal.wait_since(signal.epoch()));
        assert!(wait.as_mut().poll(&mut cx).is_pending());
        flag
    }

    fn woken(flag: &Flag) -> bool {
        flag.0.load(Ordering::SeqCst)
    }

    #[test]
    fn a_marked_threads_poke_leaves_another_threads_waiter_pending() {
        static SIGNAL: Signal = Signal::new();
        let other = std::thread::spawn(|| park_waiter(&SIGNAL));
        let Ok(other_flag) = other.join() else { return };
        let marked = std::thread::spawn(|| {
            isolate_current_thread();
            let mine = park_waiter(&SIGNAL);
            SIGNAL.poke();
            woken(&mine)
        });
        assert_eq!(marked.join().ok(), Some(true), "its own waiter wakes");
        assert!(
            !woken(&other_flag),
            "the other thread's waiter stays asleep"
        );
        SIGNAL.poke();
        assert!(woken(&other_flag), "an unmarked poke still reaches it");
    }

    #[test]
    fn an_unmarked_poke_wakes_marked_and_unmarked_waiters() {
        static SIGNAL: Signal = Signal::new();
        let marked = std::thread::spawn(|| {
            isolate_current_thread();
            park_waiter(&SIGNAL)
        });
        let Ok(marked_flag) = marked.join() else {
            return;
        };
        let plain = park_waiter(&SIGNAL);
        SIGNAL.poke();
        assert!(woken(&marked_flag));
        assert!(woken(&plain));
    }
}
