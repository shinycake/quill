//! Call media capture owned by Quill: the microphone and the screen are
//! captured here and handed to the call engine as external frames, so Quill can process it first (noise
//! suppression) and read its level without a second device open.
//!
//! - `denoise`: 10 ms framing, RNNoise (`nnnoiseless`), mute, level.
//! - `feeder`: the bounded drop-oldest sink and the thread that pushes
//!   frames into the engine (`CallEngine::external_frame_sender`).
//! - `mic_cpal` (UI builds): the microphone through cpal.
//! - `screen`: screen sources, the I420 conversions and the per-OS
//!   [`screen::ScreenBackend`]; `screen_mac` is ScreenCaptureKit,
//!   `screen_win` Windows.Graphics.Capture (`screen_bgra` converts its
//!   BGRA frames), `screen_linux` the screen cast portal and PipeWire on
//!   Wayland (`pipewire_dl`, `spa_pod`, `pipewire_frames`, `screen_yuv`)
//!   and X11 capture elsewhere (`screen_x11`).
//! - `system_audio`: "Share System Audio", mixed into the microphone
//!   frames; `system_audio_cpal` is WASAPI loopback (Windows).
//!
//! Design: `docs/decisions/call-capture.md`.
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub mod denoise;
pub mod feeder;
#[cfg(feature = "ui")]
pub mod mic_cpal;
#[cfg(all(feature = "ui", target_os = "linux"))]
pub mod pipewire_dl;
pub mod pipewire_frames;
pub mod screen;
pub mod screen_bgra;
#[cfg(all(feature = "ui", target_os = "linux"))]
pub mod screen_linux;
#[cfg(all(feature = "ui", target_os = "macos"))]
pub mod screen_mac;
#[cfg(all(feature = "ui", windows))]
pub mod screen_win;
#[cfg(all(feature = "ui", target_os = "linux"))]
pub mod screen_x11;
pub mod screen_yuv;
pub mod spa_pod;
pub mod system_audio;
#[cfg(all(feature = "ui", windows))]
pub mod system_audio_cpal;

pub use denoise::MicProcessor;
pub use feeder::{CaptureClock, CaptureFeeder, FeedFrame, FrameSink};

/// Why a capture backend could not start or stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// The OS refused access; the text says where to allow it.
    #[error("{0}")]
    Permission(String),
    #[error("No microphone found.")]
    NoDevice,
    #[error("{0}")]
    Unsupported(String),
    /// The device failed or went away.
    #[error("{0}")]
    Device(String),
}

/// What a capture backend reports beside its frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureEvent {
    /// The shared source is hidden (screen sharing; never the microphone).
    Paused,
    Resumed,
    Failed(CaptureError),
    /// The source closed (device unplugged, window closed).
    Ended,
}

/// One 10 ms microphone frame: 480 mono samples at 48 kHz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    pub pcm: Vec<i16>,
    pub captured_at: Instant,
}

/// One I420 picture (screen sharing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub i420: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub captured_at: Instant,
}

/// A microphone backend. It produces frames on its own thread into the
/// sink it is started with, and reports failures as sink events.
pub trait MicCapture {
    /// Open `device` (a name from the engine's device list; `None` is the
    /// system default) and start producing frames.
    fn start(
        &mut self,
        device: Option<&str>,
        sink: FrameSink<AudioFrame>,
    ) -> Result<(), CaptureError>;
    /// Close the device. Idempotent.
    fn stop(&mut self);
}

/// Live switches and the level a microphone capture shares with the
/// driver and the UI. Atomics, so the capture thread never locks.
#[derive(Debug)]
pub struct MicControls {
    noise_suppression: AtomicBool,
    muted: AtomicBool,
    level: Arc<LevelCell>,
    /// Shared system audio added to each frame ("Share System Audio").
    system_audio: Mutex<Option<Arc<system_audio::SystemAudioMix>>>,
}

impl MicControls {
    pub fn new(noise_suppression: bool, level: Arc<LevelCell>) -> Self {
        Self {
            noise_suppression: AtomicBool::new(noise_suppression),
            muted: AtomicBool::new(false),
            level,
            system_audio: Mutex::new(None),
        }
    }

    /// The system audio to mix in, while it is shared.
    pub fn system_audio(&self) -> Option<Arc<system_audio::SystemAudioMix>> {
        self.system_audio.lock_or_recover().clone()
    }

    pub fn set_system_audio(&self, mix: Option<Arc<system_audio::SystemAudioMix>>) {
        *self.system_audio.lock_or_recover() = mix;
    }

    pub fn noise_suppression(&self) -> bool {
        self.noise_suppression.load(Ordering::Relaxed)
    }

    pub fn set_noise_suppression(&self, on: bool) {
        self.noise_suppression.store(on, Ordering::Relaxed);
    }

    /// Muted captures keep running and send silence, so unmuting is
    /// instant and the engine's external source never runs dry.
    pub fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn level(&self) -> &Arc<LevelCell> {
        &self.level
    }
}

/// The highest microphone level since the last read (tgcalls' scale,
/// `calls::audio_level`), written by a capture thread and read by the
/// driver or the UI.
#[derive(Debug)]
pub struct LevelCell {
    /// `f32` bits; [`NO_LEVEL`] means no window completed since the read.
    peak_since_read: AtomicU32,
}

const NO_LEVEL: u32 = u32::MAX;

impl Default for LevelCell {
    fn default() -> Self {
        Self {
            peak_since_read: AtomicU32::new(NO_LEVEL),
        }
    }
}

impl LevelCell {
    /// Keep the higher of `level` and what is stored.
    pub fn record(&self, level: f32) {
        let mut seen = self.peak_since_read.load(Ordering::Acquire);
        loop {
            let current = if seen == NO_LEVEL {
                0.0
            } else {
                f32::from_bits(seen)
            };
            let next = level.max(current).to_bits();
            match self.peak_since_read.compare_exchange(
                seen,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => seen = actual,
            }
        }
    }

    /// The highest level since the previous take, then reset; `None`
    /// when no window completed since.
    pub fn take(&self) -> Option<f32> {
        let bits = self.peak_since_read.swap(NO_LEVEL, Ordering::AcqRel);
        (bits != NO_LEVEL).then(|| f32::from_bits(bits))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_cell_keeps_the_peak_until_read() {
        let cell = LevelCell::default();
        assert_eq!(cell.take(), None);
        cell.record(0.3);
        cell.record(0.1);
        cell.record(0.7);
        cell.record(0.2);
        assert_eq!(cell.take(), Some(0.7));
        assert_eq!(cell.take(), None);
        cell.record(0.0);
        assert_eq!(cell.take(), Some(0.0));
    }

    #[test]
    fn controls_switch_live() {
        let controls = MicControls::new(true, Arc::default());
        assert!(controls.noise_suppression());
        controls.set_noise_suppression(false);
        assert!(!controls.noise_suppression());
        assert!(!controls.muted());
        controls.set_muted(true);
        assert!(controls.muted());
    }
}
