//! A capture device opened, run and closed on a thread of its own. The
//! device never leaves its thread, so it need not be `Send`.

use super::CaptureError;
use crate::thread_guard::spawn_guarded;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

pub struct DeviceThread {
    answer: Receiver<Result<(), CaptureError>>,
    _closing: Sender<()>,
}

impl DeviceThread {
    /// Run `open` on a new thread and return at once; its answer wakes the
    /// poll loop, which reads it with [`Self::opened`]. Dropping this
    /// closes the device on that thread with `close`, however late the
    /// open ends.
    pub fn open<D>(
        name: &str,
        open: impl FnOnce() -> Result<D, CaptureError> + Send + 'static,
        close: impl FnOnce(D) + Send + 'static,
    ) -> Self {
        let (answers, answer) = channel();
        let (closing, closed) = channel::<()>();
        let failed = answers.clone();
        let spawned = spawn_guarded(name, move || {
            let device = match open() {
                Ok(device) => {
                    let _ = answers.send(Ok(()));
                    device
                }
                Err(err) => {
                    let _ = answers.send(Err(err));
                    crate::wake::UPDATES.poke();
                    return;
                }
            };
            crate::wake::UPDATES.poke();
            let _ = closed.recv();
            close(device);
        });
        if let Err(err) = spawned {
            let _ = failed.send(Err(CaptureError::Device(err.to_string())));
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
    fn a_device_dropped_while_opening_closes_when_the_open_ends() {
        let (release, released) = channel::<()>();
        let (done, closed) = channel();
        let device = DeviceThread::open(
            "test-device",
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
