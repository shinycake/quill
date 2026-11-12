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
        self.sleep_while(Some(deadline), |state| {
            state.info.is_none() && state.decoding.error().is_none()
        });
    }

    /// [`Player::wait_open`] with no deadline: it returns when the open
    /// settles, so a test waits on the decode thread, not on the clock.
    pub fn wait_open_settled(&self) {
        self.sleep_while(None, |state| {
            state.info.is_none() && state.decoding.error().is_none()
        });
    }

    /// Sleep until a picture is queued, decoding failed, ended (a seek at
    /// or past the last picture) or stopped, or `deadline` passes (tests
    /// and screenshot captures; never the UI thread).
    pub fn wait_frame(&self, deadline: Instant) {
        self.sleep_while(Some(deadline), Self::frame_pending);
    }

    /// [`Player::wait_frame`] with no deadline: it returns when a picture
    /// is queued or decoding ended, failed or stopped, so a test waits on
    /// the decode thread, not on the clock.
    pub fn wait_frame_settled(&self) {
        self.sleep_while(None, Self::frame_pending);
    }

    fn frame_pending(state: &State) -> bool {
        state.video.is_empty()
            && state.decoding.error().is_none()
            && !state.decoding.at_end()
            && !state.decoding.is_stopped()
    }

    /// Sleep while `waiting` holds: until `deadline`, or for as long as it
    /// takes when there is none.
    fn sleep_while(&self, deadline: Option<Instant>, waiting: impl Fn(&State) -> bool) {
        let mut state = lock(&self.shared.state);
        while waiting(&state) {
            state = match deadline {
                None => self
                    .shared
                    .progress
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return;
                    }
                    self.shared
                        .progress
                        .wait_timeout(state, left)
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .0
                }
            };
        }
    }
}
