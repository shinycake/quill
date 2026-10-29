//! The microphone path between the device and the call: mono 48 kHz
//! samples of any chunk size become exactly-480-sample (10 ms) frames,
//! go through RNNoise when noise suppression is on (`nnnoiseless`, the
//! port of the library tgcalls uses for tdesktop's
//! `setIsNoiseSuppressionEnabled`), become silence while muted, and
//! report their level in tgcalls' windows (`audio_level::PeakWindow`).
//!
//! Runs on the capture thread; no allocation per frame beyond the
//! frame's own buffer.
#![deny(clippy::indexing_slicing)]

use super::{AudioFrame, MicControls};
use crate::calls::audio_level::PeakWindow;
use crate::calls::engine::{EXTERNAL_AUDIO_FRAME_SAMPLES, EXTERNAL_AUDIO_RATE};
use nnnoiseless::DenoiseState;
use std::time::{Duration, Instant};

const FRAME: usize = EXTERNAL_AUDIO_FRAME_SAMPLES;
/// RNNoise works on the 16-bit scale, not -1..1.
const I16_SCALE: f32 = 32_768.0;

/// Turns microphone chunks into processed 10 ms frames.
pub struct MicProcessor {
    /// Samples (-1..1) waiting for a full frame.
    pending: Vec<f32>,
    denoiser: Box<DenoiseState<'static>>,
    /// Whether the previous frame went through the denoiser; switching
    /// it back on starts from a fresh state, not a stale one.
    denoised_last: bool,
    window: PeakWindow,
    /// The raw microphone's windows while muted, for the "You are on
    /// mute" hint (tdesktop measures the device itself then).
    muted_window: PeakWindow,
}

impl Default for MicProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl MicProcessor {
    pub fn new() -> Self {
        Self {
            pending: Vec::with_capacity(FRAME * 2),
            denoiser: DenoiseState::new(),
            denoised_last: false,
            window: PeakWindow::default(),
            muted_window: PeakWindow::default(),
        }
    }

    /// Feed mono 48 kHz samples in -1..1 that ended at `now`; every full
    /// frame is processed and handed to `emit`, oldest first.
    pub fn push(
        &mut self,
        samples: &[f32],
        controls: &MicControls,
        now: Instant,
        mut emit: impl FnMut(AudioFrame),
    ) {
        let mut pending = std::mem::take(&mut self.pending);
        pending.extend_from_slice(samples);
        let (frames, rest) = pending.as_chunks::<FRAME>();
        for (index, chunk) in frames.iter().enumerate() {
            // Samples captured after this frame ended, for its timestamp.
            let after = pending
                .len()
                .saturating_sub(index.saturating_add(1).saturating_mul(FRAME));
            let captured_at = now.checked_sub(samples_duration(after)).unwrap_or(now);
            let pcm = self.process_frame(chunk, controls);
            emit(AudioFrame { pcm, captured_at });
        }
        let rest = rest.to_vec();
        pending.clear();
        pending.extend_from_slice(&rest);
        self.pending = pending;
    }

    /// One 480-sample frame to i16, through the denoiser when on.
    fn process_frame(&mut self, input: &[f32; FRAME], controls: &MicControls) -> Vec<i16> {
        if controls.muted() {
            self.denoised_last = false;
            // Muted means silent: the system audio of this frame is dropped
            // too, so unmuting does not replay it late.
            if let Some(mix) = controls.system_audio() {
                mix.mix_into(&mut [0.0; FRAME], 0.0);
            }
            self.window
                .push(&[0.0; FRAME], |level| controls.level().record(level));
            self.muted_window
                .push(input, |level| controls.muted_level().record(level));
            return vec![0; FRAME];
        }
        let mut scaled = [0.0_f32; FRAME];
        for (out, sample) in scaled.iter_mut().zip(input) {
            *out = sample.clamp(-1.0, 1.0) * I16_SCALE;
        }
        let processed = if controls.noise_suppression() {
            if !self.denoised_last {
                self.denoiser = DenoiseState::new();
            }
            self.denoised_last = true;
            let mut denoised = [0.0_f32; FRAME];
            self.denoiser.process_frame(&mut denoised, &scaled);
            denoised
        } else {
            self.denoised_last = false;
            scaled
        };
        let mut normalized = [0.0_f32; FRAME];
        for (out, sample) in normalized.iter_mut().zip(processed.iter()) {
            *out = sample / I16_SCALE;
        }
        self.window
            .push(&normalized, |level| controls.level().record(level));
        // Shared system audio joins after noise suppression and the level
        // (`system_audio`).
        let mut out = processed;
        if let Some(mix) = controls.system_audio() {
            mix.mix_into(&mut out, I16_SCALE);
        }
        out.iter().map(|&sample| to_i16(sample)).collect()
    }
}

/// A sample on the 16-bit scale, rounded and clamped.
fn to_i16(sample: f32) -> i16 {
    if !sample.is_finite() {
        return 0;
    }
    // In range after the clamp, so the cast cannot saturate oddly.
    sample
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

fn samples_duration(samples: usize) -> Duration {
    let samples = u64::try_from(samples).unwrap_or(u64::MAX);
    Duration::from_micros(samples.saturating_mul(1_000_000) / u64::from(EXTERNAL_AUDIO_RATE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::audio_level::level_from_peak;
    use crate::calls::capture::LevelCell;
    use std::sync::Arc;

    fn controls(noise: bool) -> MicControls {
        MicControls::new(noise, Arc::new(LevelCell::default()))
    }

    fn collect(
        processor: &mut MicProcessor,
        chunk: &[f32],
        controls: &MicControls,
    ) -> Vec<AudioFrame> {
        let mut frames = Vec::new();
        processor.push(chunk, controls, Instant::now(), |frame| frames.push(frame));
        frames
    }

    #[test]
    fn rnnoise_frame_is_one_external_frame() {
        assert_eq!(DenoiseState::FRAME_SIZE, EXTERNAL_AUDIO_FRAME_SAMPLES);
    }

    #[test]
    fn arbitrary_chunks_become_exact_frames() {
        let controls = controls(false);
        let mut processor = MicProcessor::new();
        let mut frames = Vec::new();
        let mut total = 0;
        // Chunk sizes devices actually deliver, plus odd ones.
        for size in [441, 512, 1, 479, 960, 1024, 7, 300] {
            let chunk: Vec<f32> = (0..size).map(|i| (i as f32 / 1000.0).sin() * 0.5).collect();
            total += size;
            frames.extend(collect(&mut processor, &chunk, &controls));
        }
        assert_eq!(frames.len(), total / FRAME);
        assert!(frames.iter().all(|frame| frame.pcm.len() == FRAME));
        assert_eq!(processor.pending.len(), total % FRAME);
    }

    #[test]
    fn timestamps_follow_the_samples() {
        let controls = controls(false);
        let mut processor = MicProcessor::new();
        let now = Instant::now();
        let mut frames = Vec::new();
        processor.push(&[0.0; FRAME * 3 + 100], &controls, now, |f| frames.push(f));
        assert_eq!(frames.len(), 3);
        // The last frame ended 100 samples before `now`; each earlier one
        // 10 ms before that.
        let first = now.duration_since(frames[0].captured_at);
        let last = now.duration_since(frames[2].captured_at);
        assert_eq!(last, samples_duration(100));
        assert_eq!(first, samples_duration(100 + 2 * FRAME));
        assert!(
            frames
                .windows(2)
                .all(|w| w[0].captured_at < w[1].captured_at)
        );
    }

    #[test]
    fn passes_through_when_off_with_an_i16_round_trip() {
        let controls = controls(false);
        let mut processor = MicProcessor::new();
        let input: Vec<i16> = (0..FRAME as i32)
            .map(|i| ((i * 97) % 20_000 - 10_000) as i16)
            .collect();
        let samples: Vec<f32> = input.iter().map(|&s| f32::from(s) / I16_SCALE).collect();
        let frames = collect(&mut processor, &samples, &controls);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].pcm, input);
        // Full scale clamps instead of wrapping.
        let loud = collect(&mut processor, &[2.0; FRAME], &controls);
        assert!(loud[0].pcm.iter().all(|&s| s == i16::MAX));
        let quiet = collect(&mut processor, &[-2.0; FRAME], &controls);
        assert!(quiet[0].pcm.iter().all(|&s| s == i16::MIN));
    }

    #[test]
    fn silence_stays_silence_through_the_denoiser() {
        let controls = controls(true);
        let mut processor = MicProcessor::new();
        for _ in 0..20 {
            let frames = collect(&mut processor, &[0.0; FRAME], &controls);
            assert_eq!(frames.len(), 1);
            assert!(frames[0].pcm.iter().all(|&s| s == 0));
        }
    }

    #[test]
    fn denoiser_lowers_steady_noise() {
        let controls = controls(true);
        let mut processor = MicProcessor::new();
        // Deterministic white-ish noise at about -20 dBFS.
        let mut seed = 0x1234_5678_u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((seed >> 8) as f32 / (1 << 24) as f32 - 0.5) * 0.2
        };
        let energy = |pcm: &[i16]| pcm.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>();
        let mut input_energy = 0.0;
        let mut output_energy = 0.0;
        for i in 0..200 {
            let chunk: Vec<f32> = (0..FRAME).map(|_| noise()).collect();
            let frames = collect(&mut processor, &chunk, &controls);
            // Skip the first second while RNNoise settles.
            if i >= 100 {
                let raw: Vec<i16> = chunk.iter().map(|&s| to_i16(s * I16_SCALE)).collect();
                input_energy += energy(&raw);
                output_energy += energy(&frames[0].pcm);
            }
        }
        assert!(
            output_energy < input_energy * 0.5,
            "noise kept: {output_energy} of {input_energy}"
        );
    }

    #[test]
    fn muted_frames_are_silent_and_measure_zero() {
        let controls = controls(true);
        controls.set_muted(true);
        let mut processor = MicProcessor::new();
        let mut frames = Vec::new();
        // 4,400 samples complete one level window.
        for _ in 0..10 {
            frames.extend(collect(&mut processor, &[0.8; FRAME], &controls));
        }
        assert!(frames.iter().all(|f| f.pcm.iter().all(|&s| s == 0)));
        assert_eq!(controls.level().take(), Some(0.0));
        // The raw microphone is still measured, for the muted-speech hint.
        let muted = controls.muted_level().take().expect("a muted window");
        assert!((muted - level_from_peak(0.8)).abs() < 1e-4);
    }

    #[test]
    fn level_follows_the_processed_frames() {
        let controls = controls(false);
        let mut processor = MicProcessor::new();
        for _ in 0..10 {
            collect(&mut processor, &[0.5; FRAME], &controls);
        }
        let level = controls.level().take().expect("a window completed");
        assert!(
            level > 1.0,
            "a loud input is voice on tgcalls' scale: {level}"
        );
    }

    #[test]
    fn system_audio_joins_after_denoise_and_not_while_muted() {
        let controls = controls(true);
        let mix =
            std::sync::Arc::new(crate::calls::capture::system_audio::SystemAudioMix::default());
        controls.set_system_audio(Some(mix.clone()));
        let mut processor = MicProcessor::new();
        mix.push(&[0.25; FRAME]);
        let frames = collect(&mut processor, &[0.0; FRAME], &controls);
        // Silence through RNNoise stays silence; the music is added after.
        assert!(
            frames[0].pcm.iter().all(|&s| s == 8192),
            "{:?}",
            &frames[0].pcm[..4]
        );
        assert_eq!(controls.level().take(), None, "no level window yet");
        controls.set_muted(true);
        mix.push(&[0.25; FRAME]);
        let frames = collect(&mut processor, &[0.0; FRAME], &controls);
        assert!(frames[0].pcm.iter().all(|&s| s == 0));
        assert_eq!(mix.queued(), 0, "dropped, not replayed after unmuting");
    }
}
