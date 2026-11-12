//! One connection's `quill://unread` subscription: whether a watcher thread
//! runs for it, and which one. A watcher runs only while the subscription
//! holds its own token, so an unsubscribe followed by a subscribe retires
//! the old watcher instead of running two, and the connection ending
//! retires the last.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// Not subscribed: no watcher should run.
    None,
    /// The watcher that holds `token` runs.
    Watching { token: u64 },
}

pub(super) struct Subscription(Mutex<Stage>);

static NEXT_TOKEN: AtomicU64 = AtomicU64::new(0);

impl Subscription {
    pub(super) fn new() -> Self {
        Self(Mutex::new(Stage::None))
    }

    /// Subscribe: the token a new watcher runs under, or `None` when one
    /// already watches.
    pub(super) fn start(&self) -> Option<u64> {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        let mut stage = crate::locks::lock_or_recover(&self.0);
        match *stage {
            Stage::None => {
                *stage = Stage::Watching { token };
                Some(token)
            }
            Stage::Watching { .. } => None,
        }
    }

    /// Whether the watcher under `token` should keep running.
    pub(super) fn is_current(&self, token: u64) -> bool {
        *crate::locks::lock_or_recover(&self.0) == Stage::Watching { token }
    }

    /// Unsubscribe, or the connection ended: every watcher stops.
    pub(super) fn stop(&self) {
        *crate::locks::lock_or_recover(&self.0) = Stage::None;
        // A watcher parked while Quill is locked or signed out ends now.
        crate::agent::events::wake_waiters();
    }

    /// The watcher under `token` ended by itself.
    pub(super) fn finished(&self, token: u64) {
        let mut stage = crate::locks::lock_or_recover(&self.0);
        if *stage == (Stage::Watching { token }) {
            *stage = Stage::None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Subscription;

    #[test]
    fn one_watcher_at_a_time_and_a_stop_retires_it() {
        let subscription = Subscription::new();
        let first = subscription.start();
        assert!(first.is_some_and(|token| subscription.is_current(token)));
        assert_eq!(subscription.start(), None, "already watching");
        subscription.stop();
        assert!(!first.is_some_and(|token| subscription.is_current(token)));
        let second = subscription.start();
        assert!(second.is_some() && second != first);
        // The retired watcher ending does not stop the new one.
        if let Some(token) = first {
            subscription.finished(token);
        }
        assert!(second.is_some_and(|token| subscription.is_current(token)));
        if let Some(token) = second {
            subscription.finished(token);
            assert!(!subscription.is_current(token));
        }
    }

    #[test]
    fn a_stop_ends_a_watcher_parked_with_nothing_to_look_at() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let subscription = Arc::new(Subscription::new());
        let token = subscription.start().expect("first watcher");
        let ended = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (subscription, ended) = (subscription.clone(), ended.clone());
            std::thread::spawn(move || {
                while subscription.is_current(token) {
                    crate::agent::events::park(std::time::Duration::from_secs(600), || {
                        subscription.is_current(token)
                    });
                }
                ended.store(true, Ordering::SeqCst);
            })
        };
        // One stop, wherever the watcher is: before it parks, `park` sees
        // it under the lock; after, the notify wakes it.
        subscription.stop();
        assert!(watcher.join().is_ok());
        assert!(ended.load(Ordering::SeqCst));
    }
}
