//! "Share System Audio" (tdesktop's chooser checkbox, shown only where
//! loopback capture exists: Windows). What the computer plays is captured
//! by a loopback backend (`system_audio_cpal`, WASAPI loopback), turned
//! into 48 kHz mono and queued here; the microphone path adds it to each
//! outgoing frame after noise suppression, as tgcalls adds
//! `addExternalAudioSamples` to a 1:1 call's audio. RNNoise never sees
//! it (it would treat music as noise), it does not count toward your
//! speaking level, and a muted microphone sends silence as before.
//!
//! The queue holds at most [`SystemAudioMix::MAX_SAMPLES`] (200 ms); the
//! oldest samples go first when the microphone falls behind.
#![deny(clippy::indexing_slicing)]

use crate::locks::LockExt;
use std::collections::VecDeque;
use std::sync::Mutex;

/// Whether this build can share system audio (tdesktop
/// `LoopbackAudioCaptureSupported`: Windows only).
pub fn system_audio_supported() -> bool {
    cfg!(all(feature = "ui", windows))
}

/// System audio waiting to be mixed: 48 kHz mono, -1..1.
#[derive(Debug, Default)]
pub struct SystemAudioMix {
    queue: Mutex<VecDeque<f32>>,
}

impl SystemAudioMix {
    /// 200 ms at 48 kHz.
    pub const MAX_SAMPLES: usize = 9_600;

    /// Queue captured samples, dropping the oldest past the limit.
    pub fn push(&self, samples: &[f32]) {
        let mut queue = self.queue.lock_or_recover();
        queue.extend(samples.iter().map(|sample| {
            if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            }
        }));
        let excess = queue.len().saturating_sub(Self::MAX_SAMPLES);
        queue.drain(..excess);
    }

    /// Add the next samples to `frame` (on the 16-bit scale, as the
    /// microphone path holds it), as many as are queued. Returns how many
    /// were mixed.
    pub fn mix_into(&self, frame: &mut [f32], scale: f32) -> usize {
        let mut queue = self.queue.lock_or_recover();
        let count = frame.len().min(queue.len());
        for (out, sample) in frame.iter_mut().zip(queue.drain(..count)) {
            *out += sample * scale;
        }
        count
    }

    /// Samples waiting.
    pub fn queued(&self) -> usize {
        self.queue.lock_or_recover().len()
    }
}

/// A running loopback capture. Dropping it stops it too.
pub trait SystemAudioCapture {
    fn stop(&mut self);
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn queued_audio_is_added_once_in_order() {
        let mix = SystemAudioMix::default();
        mix.push(&[0.5, -0.5, 0.25]);
        let mut frame = [100.0_f32; 2];
        assert_eq!(mix.mix_into(&mut frame, 1000.0), 2);
        assert_eq!(frame, [600.0, -400.0]);
        let mut frame = [0.0_f32; 4];
        assert_eq!(mix.mix_into(&mut frame, 1000.0), 1);
        assert_eq!(frame, [250.0, 0.0, 0.0, 0.0]);
        assert_eq!(mix.queued(), 0);
    }

    #[test]
    fn the_queue_keeps_the_newest_200_ms_and_clean_samples() {
        let mix = SystemAudioMix::default();
        mix.push(&vec![0.1; SystemAudioMix::MAX_SAMPLES]);
        mix.push(&[2.0, f32::NAN]);
        assert_eq!(mix.queued(), SystemAudioMix::MAX_SAMPLES);
        let mut frame = vec![0.0_f32; SystemAudioMix::MAX_SAMPLES];
        mix.mix_into(&mut frame, 1.0);
        assert_eq!(frame[SystemAudioMix::MAX_SAMPLES - 2], 1.0);
        assert_eq!(frame[SystemAudioMix::MAX_SAMPLES - 1], 0.0);
    }

    #[test]
    fn only_windows_shares_system_audio() {
        assert_eq!(system_audio_supported(), cfg!(all(feature = "ui", windows)));
    }
}
