//! Waiting on the decode thread without polling: a wake that runs when the
//! file opens, and blocking waits (for tests and screenshot captures) on
//! the condvar the decode thread notifies.
#![deny(clippy::indexing_slicing)]

use super::{Player, Shared, State, lock};
use std::time::Instant;

impl Shared {
    /// The open ended (the file is open, or opening failed or the thread
    /// died): run the wakes waiting for it, outside the lock.
    pub(super) fn open_settled(&self) {
        let wakes = std::mem::take(&mut lock(&self.state).open_wakes);
        self.progress.notify_all();
        for wake in wakes {
            wake();
        }
    }
}

impl Player {
    /// Run `wake` once the file is open or could not be opened (at once
    /// when that already happened). It runs on the decode thread, so it
    /// must only signal.
    pub fn on_open(&self, wake: impl FnOnce() + Send + 'static) {
        let mut state = lock(&self.shared.state);
        if state.info.is_none() && state.decoding.error().is_none() {
            state.open_wakes.push(Box::new(wake));
            return;
        }
        drop(state);
        wake();
    }

    /// Sleep until the file is open or failed to open, or `deadline`
    /// passes (tests and screenshot captures; never the UI thread).
    pub fn wait_open(&self, deadline: Instant) {
        self.sleep_while(deadline, |state| {
            state.info.is_none() && state.decoding.error().is_none()
        });
    }

    /// Sleep until a picture is queued or decoding failed, or `deadline`
    /// passes (tests and screenshot captures; never the UI thread).
    pub fn wait_frame(&self, deadline: Instant) {
        self.sleep_while(deadline, |state| {
            state.video.is_empty() && state.decoding.error().is_none()
        });
    }

    fn sleep_while(&self, deadline: Instant, mut waiting: impl FnMut(&State) -> bool) {
        let mut state = lock(&self.shared.state);
        while waiting(&state) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            state = self
                .shared
                .progress
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }
}
