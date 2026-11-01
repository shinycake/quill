//! A capture device opened, run and closed on a thread of its own. The
//! device never leaves its thread, so it need not be `Send`.

use super::CaptureError;
use crate::thread_guard::spawn_guarded;
use crate::wake::Signal;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub struct DeviceThread {
    answer: Receiver<Result<(), CaptureError>>,
    _closing: Sender<()>,
}

impl DeviceThread {
    /// Run `open` on a new thread and return at once; its answer pokes
    /// `wake`, and the poll loop reads it with [`Self::opened`]. Dropping
    /// this closes the device on that thread with `close`, however late
    /// the open ends.
    pub fn open<D>(
        name: &str,
        wake: &'static Signal,
        open: impl FnOnce() -> Result<D, CaptureError> + Send + 'static,
        close: impl FnOnce(D) + Send + 'static,
    ) -> Self {
        let (answers, answer) = channel();
        let (closing, closed) = channel::<()>();
        let failed = answers.clone();
        let spawned = spawn_guarded(name, move || {
            let answering = Answering {
                to: Some(answers),
                wake,
            };
            let device = match open() {
                Ok(device) => {
                    answering.send(Ok(()));
                    device
                }
                Err(err) => {
                    answering.send(Err(err));
                    return;
                }
            };
            let _ = closed.recv();
            close(device);
        });
        if let Err(err) = spawned {
            let _ = failed.send(Err(CaptureError::Device(err.to_string())));
            wake.poke();
        }
        Self {
            answer,
            _closing: closing,
        }
    }

    /// How the open went, once; `None` while it runs.
    pub fn opened(&self) -> Option<Result<(), CaptureError>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(CaptureError::Device(
                "The capture thread stopped.".into(),
            ))),
        }
    }
}

/// The open's answer. Whether the open answers or panics, the poll loop
/// is poked after the answer or the hang-up is there to read.
struct Answering {
    to: Option<Sender<Result<(), CaptureError>>>,
    wake: &'static Signal,
}

impl Answering {
    fn send(mut self, answer: Result<(), CaptureError>) {
        if let Some(to) = self.to.take() {
            let _ = to.send(answer);
        }
    }
}

impl Drop for Answering {
    fn drop(&mut self) {
        drop(self.to.take());
        self.wake.poke();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn answer(device: &DeviceThread) -> Result<(), CaptureError> {
        loop {
            if let Some(answer) = device.opened() {
                return answer;
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_blocked_open_holds_nobody_and_answers_when_it_ends() {
        let (release, released) = channel::<()>();
        let device = DeviceThread::open(
            "test-device",
            &crate::wake::UPDATES,
            move || released.recv().map_err(|_| CaptureError::NoDevice),
            |()| {},
        );
        assert_eq!(device.opened(), None);
        release.send(()).unwrap();
        assert_eq!(answer(&device), Ok(()));
    }

    #[test]
    fn a_failed_open_answers_with_its_error_and_closes_nothing() {
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let device = DeviceThread::open(
            "test-device",
            &crate::wake::UPDATES,
            || Err::<(), _>(CaptureError::NoDevice),
            move |()| flag.store(true, Ordering::SeqCst),
        );
        assert_eq!(answer(&device), Err(CaptureError::NoDevice));
        drop(device);
        assert!(!closed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_device_closes_on_its_own_thread_when_dropped() {
        let (done, closed_on) = channel();
        let device = DeviceThread::open(
            "test-device",
            &crate::wake::UPDATES,
            || Ok(std::thread::current().id()),
            move |opened_on| {
                let _ = done.send(opened_on == std::thread::current().id());
            },
        );
        assert_eq!(answer(&device), Ok(()));
        drop(device);
        assert_eq!(closed_on.recv(), Ok(true));
    }

    #[test]
    fn a_panicked_open_answers_and_wakes_the_poll_loop() {
        static WAKE: Signal = Signal::new();
        let seen = WAKE.epoch();
        let thread_alive = Arc::new(());
        let held = thread_alive.clone();
        let device = DeviceThread::open(
            "test-device",
            &WAKE,
            || -> Result<(), CaptureError> { panic!("a driver bug") },
            move |()| drop(held),
        );
        assert!(matches!(answer(&device), Err(CaptureError::Device(_))));
        // The thread's last act drops what it holds.
        while Arc::strong_count(&thread_alive) > 1 {
            std::thread::yield_now();
        }
        assert_ne!(WAKE.epoch(), seen, "the poll loop hears it");
    }

    #[test]
    fn a_device_dropped_while_opening_closes_when_the_open_ends() {
        let (release, released) = channel::<()>();
        let (done, closed) = channel();
        let device = DeviceThread::open(
            "test-device",
            &crate::wake::UPDATES,
            move || released.recv().map_err(|_| CaptureError::NoDevice),
            move |()| {
                let _ = done.send(());
            },
        );
        drop(device);
        release.send(()).unwrap();
        assert_eq!(closed.recv(), Ok(()));
    }
}
