//! Whether the UI thread still answers. The listener runs on threads of
//! its own, so it would acknowledge a launch even while the window is
//! frozen, and the second launch would exit with nothing on screen.
//! tdesktop checks its main thread before it answers too (`sandbox.cpp`).

use std::sync::{Condvar, Mutex};
use std::time::Instant;

/// Where [`Heartbeat::answered_by`] reads the time, so a test can move it.
pub(super) trait Clock: Sync {
    fn now(&self) -> Instant;
}

pub(super) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub struct Heartbeat {
    beats: Mutex<Option<u64>>,
    beat: Condvar,
}

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

/// The UI thread that answers [`crate::wake::LINKS`].
pub static UI: Heartbeat = Heartbeat::new();

impl Heartbeat {
    pub const fn new() -> Self {
        Self {
            beats: Mutex::new(None),
            beat: Condvar::new(),
        }
    }

    /// The UI thread is up and beats each time it wakes. Until then, and
    /// after [`Self::stop`], every launch is answered at once.
    pub fn start(&self) {
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        beats.get_or_insert(0);
    }

    pub fn stop(&self) {
        *crate::locks::lock_or_recover(&self.beats) = None;
        self.beat.notify_all();
    }

    pub fn beat(&self) {
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        if let Some(count) = beats.as_mut() {
            *count = count.wrapping_add(1);
        }
        drop(beats);
        self.beat.notify_all();
    }

    /// The mark to pass to [`Self::answered_since`].
    pub(super) fn mark(&self) -> Option<u64> {
        *crate::locks::lock_or_recover(&self.beats)
    }

    /// Whether the UI beat after `mark` by `deadline` on `clock`, or does
    /// not beat at all.
    pub(super) fn answered_by(
        &self,
        mark: Option<u64>,
        deadline: Instant,
        clock: &dyn Clock,
    ) -> bool {
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        loop {
            if beats.is_none() || mark.is_none() || *beats != mark {
                return true;
            }
            let left = deadline.saturating_duration_since(clock.now());
            if left.is_zero() {
                return false;
            }
            beats = match self.beat.wait_timeout(beats, left) {
                Ok((beats, _)) => beats,
                Err(_) => return true,
            };
        }
    }

    /// Wakes [`Self::answered_by`] to read its clock again.
    #[cfg(test)]
    pub(super) fn recheck(&self) {
        let _beats = crate::locks::lock_or_recover(&self.beats);
        self.beat.notify_all();
    }
}
