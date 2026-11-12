//! The worker thread of a media backend (MPRIS on Linux, the system
//! transport controls on Windows): a channel to a thread that talks to the
//! OS, so the UI never waits on D-Bus or WinRT.
//!
//! The stage holds the thread. A thread that panicked, or that never
//! started, left a channel with no receiver: the next state to publish sees
//! its send fail, and starts a fresh worker with it, so media keys come back
//! instead of staying dead for the session.

use super::NowPlaying;
use crate::locks::LockExt;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

enum Stage {
    Stopped,
    Running {
        tx: Sender<Option<NowPlaying>>,
        _thread: JoinHandle<Option<()>>,
    },
}

/// One backend's worker.
pub(super) struct Worker(Mutex<Stage>);

impl Worker {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(Stage::Stopped))
    }

    /// Hands `info` to the worker, starting it (named `name`, running `run`)
    /// when none runs or the last one is gone.
    pub(super) fn send(
        &self,
        name: &'static str,
        run: fn(Receiver<Option<NowPlaying>>),
        info: Option<&NowPlaying>,
    ) {
        self.send_with(
            |rx| crate::thread_guard::spawn_guarded(name, move || run(rx)),
            info,
        );
    }

    /// [`Self::send`] with `spawn` as the way a worker is started.
    fn send_with(
        &self,
        mut spawn: impl FnMut(Receiver<Option<NowPlaying>>) -> std::io::Result<JoinHandle<Option<()>>>,
        info: Option<&NowPlaying>,
    ) {
        let mut stage = self.0.lock_or_recover();
        let mut payload = info.cloned();
        // A dead worker is replaced once per state.
        for _ in 0..2 {
            match &*stage {
                Stage::Running { tx, .. } => match tx.send(payload) {
                    Ok(()) => return,
                    Err(returned) => {
                        payload = returned.0;
                        *stage = Stage::Stopped;
                    }
                },
                Stage::Stopped => {
                    let (tx, rx) = channel::<Option<NowPlaying>>();
                    let Ok(thread) = spawn(rx) else {
                        // No thread: the next state tries again.
                        return;
                    };
                    *stage = Stage::Running {
                        tx,
                        _thread: thread,
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn gone() -> Stage {
        let (tx, rx) = channel::<Option<NowPlaying>>();
        drop(rx);
        Stage::Running {
            tx,
            _thread: std::thread::spawn(|| Some(())),
        }
    }

    /// A spawn that counts its starts and keeps the receiver in its thread.
    fn counting(
        starts: &Cell<usize>,
    ) -> impl FnMut(Receiver<Option<NowPlaying>>) -> std::io::Result<JoinHandle<Option<()>>> + '_
    {
        |rx| {
            starts.set(starts.get() + 1);
            Ok(std::thread::spawn(move || {
                while rx.recv().is_ok() {}
                Some(())
            }))
        }
    }

    #[test]
    fn a_worker_that_is_gone_is_replaced_by_the_next_state() {
        let starts = Cell::new(0);
        let worker = Worker(Mutex::new(gone()));
        worker.send_with(counting(&starts), None);
        assert_eq!(starts.get(), 1, "the next state starts a fresh worker");
        // That worker keeps running: it is not replaced.
        worker.send_with(counting(&starts), None);
        worker.send_with(counting(&starts), None);
        assert_eq!(starts.get(), 1);
    }

    #[test]
    fn a_worker_that_cannot_start_is_tried_again_by_the_next_state() {
        let worker = Worker::new();
        let refused = Cell::new(0);
        worker.send_with(
            |_| {
                refused.set(refused.get() + 1);
                Err(std::io::Error::other("no thread"))
            },
            None,
        );
        assert_eq!(refused.get(), 1);
        let starts = Cell::new(0);
        worker.send_with(counting(&starts), None);
        assert_eq!(starts.get(), 1);
    }
}
