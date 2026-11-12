//! A capture device opened, run and closed on a thread of its own. The
//! device never leaves its thread, so it need not be `Send`.

use super::CaptureError;
use crate::thread_guard::spawn_guarded;
use crate::wake::Signal;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};

/// Where a device is: its open runs on `thread`, or it answered and `T` is
/// what runs on it. Either way the stage holds the thread (shared, so the
/// open's stage turns into the running one in place, with no stand-in thread
/// between them), and dropping the stage closes the device.
pub enum DeviceRun<T> {
    Opening(Arc<DeviceThread>),
    Running {
        thread: Arc<DeviceThread>,
        running: T,
    },
}

impl<T> DeviceRun<T> {
    /// The open of `thread` is the stage.
    pub fn opening(thread: DeviceThread) -> Self {
        Self::Opening(Arc::new(thread))
    }

    pub fn is_opening(&self) -> bool {
        matches!(self, Self::Opening(_))
    }

    pub fn running(&self) -> Option<&T> {
        match self {
            Self::Opening(_) => None,
            Self::Running { running, .. } => Some(running),
        }
    }

    /// How the open went while it is the stage; `None` while it runs (and
    /// once the device is up: nothing is left to ask).
    pub fn opened(&self) -> Option<Result<(), CaptureError>> {
        match self {
            Self::Opening(thread) => thread.opened(),
            Self::Running { .. } => None,
        }
    }

    /// The open answered and `running` runs on its device. Any other stage
    /// stays as it is.
    pub fn start(&mut self, running: T) {
        if let Self::Opening(thread) = self {
            let thread = Arc::clone(thread);
            *self = Self::Running { thread, running };
        }
    }
}

/// The open's answer, until it arrives and then as it came: asking again
/// gives the same answer.
enum Opened {
    Waiting(Receiver<Result<(), CaptureError>>),
    Answered(Result<(), CaptureError>),
}

pub struct DeviceThread {
    opened: Mutex<Opened>,
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
            opened: Mutex::new(Opened::Waiting(answer)),
            _closing: closing,
        }
    }

    /// How the open went; `None` while it runs. Once it has answered, every
    /// call returns that answer.
    pub fn opened(&self) -> Option<Result<(), CaptureError>> {
        let mut opened = crate::locks::lock_or_recover(&self.opened);
        let answer = match &*opened {
            Opened::Answered(answer) => return Some(answer.clone()),
            Opened::Waiting(answer) => match answer.try_recv() {
                Ok(answer) => answer,
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => {
                    Err(CaptureError::Device("The capture thread stopped.".into()))
                }
            },
        };
        *opened = Opened::Answered(answer.clone());
        Some(answer)
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
    fn the_answer_is_the_same_every_time_it_is_asked() {
        let device = DeviceThread::open("test-device", &crate::wake::UPDATES, || Ok(()), |()| {});
        assert_eq!(answer(&device), Ok(()));
        assert_eq!(device.opened(), Some(Ok(())));
        assert_eq!(device.opened(), Some(Ok(())));
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
