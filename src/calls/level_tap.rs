//! A microphone level tap: a microphone opened through Quill's capture
//! path (`calls::capture`, cpal), measured in tgcalls' windows
//! (`calls::audio_level`), with the latest level readable from anywhere.
//!
//! During a call the call's own capture stream feeds the level
//! (`connect::call_audio`), so no second device open happens. The tap
//! opens the device by itself for the Settings > Calls microphone test,
//! and for a group call whose own capture is not running. CoreAudio, WASAPI (shared
//! mode) and PulseAudio / PipeWire share a capture device between
//! clients; a raw ALSA `hw:` device may refuse, in which case the tap
//! reports an error.

use crate::calls::capture::mic_cpal::CpalMic;
use crate::calls::capture::{
    AudioFrame, CaptureError, FrameSink, LevelCell, MicCapture, MicControls,
};
use crate::locks::LockExt;
use std::sync::{Arc, Mutex};

/// Where level readers get their values from. The driver's call audio
/// takes a boxed source so tests can feed levels without a microphone.
pub trait LevelSource {
    /// The highest level measured since the previous call, then reset;
    /// `None` when no window completed since.
    fn take_level(&self) -> Option<f32>;
    /// The error that stopped the source, if any.
    fn error(&self) -> Option<String>;
}

/// An open microphone measured for levels. Dropping it closes the device.
pub struct LevelTap {
    mic: CpalMic,
    controls: Arc<MicControls>,
    /// Frames nobody sends anywhere: the sink keeps at most one.
    _sink: FrameSink<AudioFrame>,
}

impl LevelTap {
    /// Open the default input device and start measuring the raw input,
    /// without noise suppression, as tdesktop's `AudioInputTester` does.
    pub fn open_default() -> Result<Self, String> {
        let controls = Arc::new(MicControls::new(false, Arc::new(LevelCell::default())));
        let mut mic = CpalMic::new(controls.clone());
        let sink = FrameSink::new(1);
        mic.start(None, sink.clone())
            .map_err(|err: CaptureError| err.to_string())?;
        Ok(Self {
            mic,
            controls,
            _sink: sink,
        })
    }
}

impl LevelSource for LevelTap {
    fn take_level(&self) -> Option<f32> {
        self.controls.level().take()
    }

    fn error(&self) -> Option<String> {
        self.mic.error()
    }
}

impl Drop for LevelTap {
    fn drop(&mut self) {
        self.mic.stop();
    }
}

/// A fixed level, for screenshot demos and tests.
pub struct FixedLevel(pub Mutex<f32>);

impl LevelSource for FixedLevel {
    fn take_level(&self) -> Option<f32> {
        Some(*self.0.lock_or_recover())
    }

    fn error(&self) -> Option<String> {
        None
    }
}
