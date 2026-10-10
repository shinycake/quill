//! A microphone level tap: the default input device opened through cpal
//! (the voice-note path, `crate::voice_input`), measured in tgcalls'
//! windows (`calls::audio_level`) on its own thread, with the latest
//! level readable from anywhere.
//!
//! ntgcalls opens the microphone itself during a call and hands Quill no
//! samples, so the tap opens the device a second time alongside it.
//! CoreAudio, WASAPI (shared mode) and PulseAudio / PipeWire share a
//! capture device between clients; a raw ALSA `hw:` device may refuse,
//! in which case the tap reports an error and the call carries on
//! without a level.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use super::audio_level::PeakWindow;

/// Where level readers get their values from. The driver's call audio
/// takes a boxed source so tests can feed levels without a microphone.
pub trait LevelSource {
    /// The highest level measured since the previous call, then reset;
    /// `None` when no window completed since.
    fn take_level(&self) -> Option<f32>;
    /// The error that stopped the source, if any.
    fn error(&self) -> Option<String>;
}

struct Shared {
    /// The highest level since the last `take_level`, as `f32` bits;
    /// `u32::MAX` means "no window completed".
    peak_since_read: AtomicU32,
    stop: AtomicBool,
}

const NO_LEVEL: u32 = u32::MAX;

/// An open microphone measured for levels. Dropping it closes the device.
pub struct LevelTap {
    mic: Option<crate::voice_input::MicStream>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl LevelTap {
    /// Open the default input device and start measuring.
    pub fn open_default() -> Result<Self, String> {
        let (sender, receiver) = std::sync::mpsc::channel::<Vec<f32>>();
        let mic = crate::voice_input::open_default(sender)?;
        let shared = Arc::new(Shared {
            peak_since_read: AtomicU32::new(NO_LEVEL),
            stop: AtomicBool::new(false),
        });
        let worker = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("quill-level-tap".into())
                .spawn(move || {
                    let mut window = PeakWindow::default();
                    while !shared.stop.load(Ordering::Acquire) {
                        let chunk = match receiver.recv_timeout(Duration::from_millis(50)) {
                            Ok(chunk) => chunk,
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        };
                        window.push(&chunk, |level| {
                            // Keep the highest level since the last read.
                            let mut seen = shared.peak_since_read.load(Ordering::Acquire);
                            loop {
                                let current = if seen == NO_LEVEL {
                                    0.0
                                } else {
                                    f32::from_bits(seen)
                                };
                                let next = level.max(current).to_bits();
                                match shared.peak_since_read.compare_exchange(
                                    seen,
                                    next,
                                    Ordering::AcqRel,
                                    Ordering::Acquire,
                                ) {
                                    Ok(_) => break,
                                    Err(actual) => seen = actual,
                                }
                            }
                        });
                    }
                })
                .map_err(|err| format!("Couldn't start the level thread: {err}"))?
        };
        Ok(Self {
            mic: Some(mic),
            shared,
            worker: Some(worker),
        })
    }
}

impl LevelSource for LevelTap {
    fn take_level(&self) -> Option<f32> {
        let bits = self.shared.peak_since_read.swap(NO_LEVEL, Ordering::AcqRel);
        (bits != NO_LEVEL).then(|| f32::from_bits(bits))
    }

    fn error(&self) -> Option<String> {
        self.mic
            .as_ref()
            .and_then(|mic| mic.error.lock().ok().and_then(|slot| slot.clone()))
    }
}

impl Drop for LevelTap {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        // Closing the stream drops the only sender; the worker then sees
        // the channel close (or the stop flag within 50 ms).
        drop(self.mic.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// A fixed level, for screenshot demos and tests.
pub struct FixedLevel(pub Mutex<f32>);

impl LevelSource for FixedLevel {
    fn take_level(&self) -> Option<f32> {
        self.0.lock().ok().map(|level| *level)
    }

    fn error(&self) -> Option<String> {
        None
    }
}
