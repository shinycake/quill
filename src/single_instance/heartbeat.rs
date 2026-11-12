//! Whether the UI thread still answers. The listener runs on threads of
//! its own, so it would acknowledge a launch even while the window is
//! frozen, and the second launch would exit with nothing on screen.
//! tdesktop checks its main thread before it answers too (`sandbox.cpp`).

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
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

/// Whether the UI thread is beating, and which launches wait for its next
/// beat.
enum Beats {
    /// Before [`Heartbeat::start`] and after [`Heartbeat::stop`]: every
    /// launch is answered at once.
    NotRunning,
    Running {
        /// The launches that wait for a beat; a beat answers all of them.
        waiting: Vec<Waiter>,
    },
}

/// A launch that waits for the next beat.
struct Waiter {
    wake: Sender<()>,
    /// When it gives up, so a launch that gave up is dropped from the list.
    until: Instant,
}

/// What a launch holds while it waits: the beat arrives on it, or the
/// sender goes away (the UI stopped), which answers it as well.
pub(super) enum Mark {
    /// The UI is not beating: answered at once.
    NotRunning,
    Waiting(Receiver<()>),
}

pub struct Heartbeat {
    beats: Mutex<Beats>,
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
            beats: Mutex::new(Beats::NotRunning),
            beat: Condvar::new(),
        }
    }

    /// The UI thread is up and beats each time it wakes. Until then, and
    /// after [`Self::stop`], every launch is answered at once.
    pub fn start(&self) {
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        if matches!(*beats, Beats::NotRunning) {
            *beats = Beats::Running {
                waiting: Vec::new(),
            };
        }
    }

    pub fn stop(&self) {
        *crate::locks::lock_or_recover(&self.beats) = Beats::NotRunning;
        self.beat.notify_all();
    }

    pub fn beat(&self) {
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        if let Beats::Running { waiting } = &mut *beats {
            for waiter in waiting.drain(..) {
                let _ = waiter.wake.send(());
            }
        }
        drop(beats);
        self.beat.notify_all();
    }

    /// A launch starts waiting for the next beat until `until`; `now` drops
    /// the launches whose time has passed.
    pub(super) fn mark(&self, now: Instant, until: Instant) -> Mark {
        match &mut *crate::locks::lock_or_recover(&self.beats) {
            Beats::NotRunning => Mark::NotRunning,
            Beats::Running { waiting } => {
                waiting.retain(|waiter| waiter.until > now);
                let (wake, beat) = channel();
                waiting.push(Waiter { wake, until });
                Mark::Waiting(beat)
            }
        }
    }

    /// Whether the UI beat after `mark` by `deadline` on `clock`, or does
    /// not beat at all.
    pub(super) fn answered_by(&self, mark: &Mark, deadline: Instant, clock: &dyn Clock) -> bool {
        let Mark::Waiting(beat) = mark else {
            return true;
        };
        let mut beats = crate::locks::lock_or_recover(&self.beats);
        loop {
            // A beat, or the UI stopping (the sender is dropped), answers.
            if !matches!(beat.try_recv(), Err(TryRecvError::Empty)) {
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
