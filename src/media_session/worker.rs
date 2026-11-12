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
                    let Ok(thread) = crate::thread_guard::spawn_guarded(name, move || run(rx))
                    else {
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    static STARTS: AtomicUsize = AtomicUsize::new(0);

    fn dies(_rx: Receiver<Option<NowPlaying>>) {
        STARTS.fetch_add(1, Ordering::SeqCst);
    }

    fn serves(rx: Receiver<Option<NowPlaying>>) {
        STARTS.fetch_add(1, Ordering::SeqCst);
        while rx.recv().is_ok() {}
    }

    #[test]
    fn a_worker_that_is_gone_is_replaced_by_the_next_state() {
        let worker = Worker::new();
        STARTS.store(0, Ordering::SeqCst);
        worker.send("t-dies", dies, None);
        // The first worker returned at once and dropped its receiver; spin
        // until a send sees that, which starts the second.
        loop {
            worker.send("t-dies", dies, None);
            if STARTS.load(Ordering::SeqCst) >= 2 {
                break;
            }
            std::thread::yield_now();
        }
        // A worker that keeps running is not replaced.
        let steady = Worker::new();
        let before = STARTS.load(Ordering::SeqCst);
        steady.send("t-serves", serves, None);
        steady.send("t-serves", serves, None);
        steady.send("t-serves", serves, None);
        assert_eq!(STARTS.load(Ordering::SeqCst), before + 1);
    }
}
