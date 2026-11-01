//! Seeks on a playing track: the UI posts them, a worker waits for the audio
//! thread.
#![deny(clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

#[cfg(test)]
use std::sync::atomic::AtomicU64;

use super::OutputPlayer;

/// How long a posted seek may wait on the audio thread before the engine
/// reports that the device isn't responding.
pub(super) const SEEK_STALL_AFTER: Duration = Duration::from_millis(2500);

/// Seeks posted to one `Player`'s audio side and what came of them.
/// `Player::try_seek` waits for the audio thread, so the UI thread never
/// calls it: a worker does, and this is the only thing the two share.
#[derive(Debug, Default)]
pub(super) struct SeekBoard {
    inner: Mutex<SeekState>,
    /// The track is gone: a worker that wakes from a stalled device exits
    /// without reading or writing anything else.
    cancelled: AtomicBool,
    /// Live seek workers across the engine's tracks.
    workers: Arc<AtomicUsize>,
    /// Tests: how long (ms) the worker takes to notice the audio thread's
    /// answer, as on a loaded machine.
    #[cfg(test)]
    pub(super) answer_delay_ms: AtomicU64,
}

#[derive(Debug, Default)]
pub(super) struct SeekState {
    /// The latest seek not yet handed to the audio thread.
    queued: Option<Seek>,
    /// The seek the audio thread has been asked for.
    in_flight: Option<Seek>,
    /// Tests: seeks handed to the audio thread so far.
    #[cfg(test)]
    pub(super) handed: u64,
    /// A worker is draining `queued`.
    running: bool,
    /// The newest seek's outcome, until the engine takes it.
    pub(super) done: Option<SeekDone>,
}

/// A posted seek and when (engine clock) the user asked for it.
#[derive(Clone, Copy, Debug)]
struct Seek {
    target: f64,
    posted: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SeekDone {
    pub(super) target: f64,
    pub(super) moved: bool,
}

/// Counts a running seek worker for as long as it lives.
struct WorkerGuard(Arc<AtomicUsize>);

impl WorkerGuard {
    fn new(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl SeekBoard {
    pub(super) fn new(workers: Arc<AtomicUsize>) -> Self {
        Self {
            inner: Mutex::default(),
            cancelled: AtomicBool::new(false),
            workers,
            #[cfg(test)]
            answer_delay_ms: AtomicU64::new(0),
        }
    }

    /// Tell the worker, if one is parked on a stalled device, to leave as
    /// soon as the device answers.
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, SeekState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Where the sound is headed: a seek not yet confirmed, or one the
    /// decoder refused and the engine has not yet answered with a restart.
    pub(super) fn target(&self) -> Option<f64> {
        let state = self.lock();
        let pending = state.queued.or(state.in_flight).map(|seek| seek.target);
        pending.or_else(|| {
            state
                .done
                .filter(|done| !done.moved)
                .map(|done| done.target)
        })
    }

    /// The audio thread has held a seek for `SEEK_STALL_AFTER` without
    /// answering: the output isn't pulling.
    pub(super) fn stalled(&self, now: Duration) -> bool {
        self.lock()
            .in_flight
            .is_some_and(|seek| now.saturating_sub(seek.posted) >= SEEK_STALL_AFTER)
    }

    pub(super) fn take_done(&self) -> Option<SeekDone> {
        self.lock().done.take()
    }

    /// Queue `target` and make sure a worker will deliver it. Returns at once.
    pub(super) fn post(self: &Arc<Self>, player: &Arc<OutputPlayer>, target: f64, now: Duration) {
        let spawn = {
            let mut state = self.lock();
            state.queued = Some(Seek {
                target,
                posted: now,
            });
            state.done = None;
            !std::mem::replace(&mut state.running, true)
        };
        if !spawn {
            return;
        }
        let (board, player) = (self.clone(), player.clone());
        let guard = WorkerGuard::new(&self.workers);
        let worker = std::thread::Builder::new()
            .name("quill-audio-seek".into())
            .spawn(move || {
                let _guard = guard;
                board.drain(&player);
            });
        if worker.is_err() {
            let mut state = self.lock();
            state.running = false;
            state.queued = None;
            state.done = Some(SeekDone {
                target,
                moved: false,
            });
        }
    }

    /// The worker: hand the newest target to the audio thread, wait there,
    /// repeat until nothing newer was posted.
    fn drain(&self, player: &OutputPlayer) {
        loop {
            if self.is_cancelled() {
                return;
            }
            let target = {
                let mut state = self.lock();
                let Some(seek) = state.queued.take() else {
                    state.running = false;
                    return;
                };
                state.in_flight = Some(seek);
                #[cfg(test)]
                {
                    state.handed += 1;
                }
                seek.target
            };
            let moved = player.try_seek(Duration::from_secs_f64(target)).is_ok();
            #[cfg(test)]
            std::thread::sleep(Duration::from_millis(
                self.answer_delay_ms.load(Ordering::Relaxed),
            ));
            if self.is_cancelled() {
                return;
            }
            let mut state = self.lock();
            state.in_flight = None;
            if state.queued.is_none() {
                state.done = Some(SeekDone { target, moved });
            }
        }
    }
}
