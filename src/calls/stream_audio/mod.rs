//! A call's sound played by Quill: the call library has no audio devices
//! (as Telegram Desktop builds tgcalls), so every call's sound comes out
//! here, and streams get a volume.
//!
//! The library hands over the call's decoded sound in 10 ms frames of
//! signed 16-bit native-endian PCM, interleaved, at the format's rate and
//! channel count. The frames are mixed
//! into a [`ring::SampleRing`], and the output (`output`, UI builds)
//! plays them at the [`gain::Gain`].
//!
//! Design: `docs/decisions/stream-volume.md`,
//! `docs/decisions/call-sound-output.md`.
#![deny(clippy::indexing_slicing)]

pub mod gain;
#[cfg(feature = "ui")]
mod output;
pub mod ring;
pub mod watch;

use gain::Gain;
use ring::SampleRing;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use watch::PlaybackHealth;

const RING_CAPACITY_MS: u32 = 200;
const OVERRUN_DROP_MS: u32 = 20;
const PLAYOUT_PRIME_MS: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmFormat {
    pub rate: u32,
    /// 1 or 2.
    pub channels: u8,
}

impl PcmFormat {
    pub fn for_output(rate: u32, device_channels: u16) -> Self {
        Self {
            rate: rate.max(1),
            channels: if device_channels >= 2 { 2 } else { 1 },
        }
    }

    pub fn interleaved_samples(&self, ms: u32) -> usize {
        let frames = u64::from(self.rate) * u64::from(ms) / 1000;
        usize::try_from(frames.saturating_mul(u64::from(self.channels))).unwrap_or(usize::MAX)
    }
}

pub struct StreamAudio {
    format: PcmFormat,
    ring: SampleRing,
    gain: Gain,
    frames: AtomicU64,
    peak: AtomicU16,
    output_failed: AtomicBool,
}

impl StreamAudio {
    pub fn new(format: PcmFormat, volume: f32, muted: bool) -> Self {
        Self {
            format,
            ring: SampleRing::new(
                format.interleaved_samples(RING_CAPACITY_MS),
                format.interleaved_samples(OVERRUN_DROP_MS),
            ),
            gain: Gain::new(volume, muted),
            frames: AtomicU64::new(0),
            peak: AtomicU16::new(0),
            output_failed: AtomicBool::new(false),
        }
    }

    pub fn format(&self) -> PcmFormat {
        self.format
    }

    pub fn ring(&self) -> &SampleRing {
        &self.ring
    }

    pub fn gain(&self) -> &Gain {
        &self.gain
    }

    pub fn prime_samples(&self) -> usize {
        self.format.interleaved_samples(PLAYOUT_PRIME_MS)
    }

    /// Runs on the engine's audio thread every 10 ms, so it never
    /// allocates: the batch is mixed sample by sample into the ring.
    pub fn push_frames<'a, I>(&self, frames: I)
    where
        I: IntoIterator<Item = &'a [u8]>,
        I::IntoIter: Clone,
    {
        let frames = frames.into_iter();
        let len = frames
            .clone()
            .map(|frame| frame.len() / 2)
            .max()
            .unwrap_or(0);
        if len == 0 {
            return;
        }
        let peak = Cell::new(0_u16);
        self.ring.push_with(len, |index| {
            let sample = mixed_sample(frames.clone(), index);
            peak.set(peak.get().max(sample.unsigned_abs()));
            sample
        });
        self.frames.fetch_add(1, Ordering::Relaxed);
        self.peak.fetch_max(peak.get(), Ordering::Relaxed);
    }

    pub fn mark_output_failed(&self) {
        self.output_failed.store(true, Ordering::Relaxed);
    }

    pub fn health(&self) -> PlaybackHealth {
        PlaybackHealth {
            frames: self.frames.load(Ordering::Relaxed),
            peak: self.peak.load(Ordering::Relaxed),
            output_failed: self.output_failed.load(Ordering::Relaxed),
        }
    }
}

/// Sample `index` of every frame (native-endian `i16`) summed and
/// clipped at the `i16` range. A frame shorter than `index` adds nothing,
/// and a trailing odd byte is ignored.
fn mixed_sample<'a>(frames: impl Iterator<Item = &'a [u8]>, index: usize) -> i16 {
    let sum = frames
        .filter_map(|frame| frame.as_chunks::<2>().0.get(index))
        .fold(0i32, |sum, pair| {
            sum.saturating_add(i32::from(i16::from_ne_bytes(*pair)))
        });
    sum.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

/// A stream's sound playing on an output device. Dropping it closes the
/// device.
pub struct StreamPlayer {
    audio: std::sync::Arc<StreamAudio>,
    #[cfg(feature = "ui")]
    _output: output::Output,
}

impl StreamPlayer {
    /// Open the output named `device` (a speaker name as the engine lists it; `None`
    /// or an unknown name is the system default) at `volume`.
    pub fn open(device: Option<&str>, volume: f32, muted: bool) -> Result<Self, String> {
        #[cfg(feature = "ui")]
        {
            let (output, audio) = output::Output::open(device, volume, muted)?;
            Ok(Self {
                audio,
                _output: output,
            })
        }
        #[cfg(not(feature = "ui"))]
        {
            let _ = (device, volume, muted);
            Err("This build has no audio output.".into())
        }
    }

    pub fn audio(&self) -> &std::sync::Arc<StreamAudio> {
        &self.audio
    }
}

#[cfg(test)]
mod tests {
    use super::{PcmFormat, StreamAudio};

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }

    /// What one batch puts in the ring.
    fn mix(frames: &[&[u8]]) -> Vec<i16> {
        let audio = StreamAudio::new(PcmFormat::for_output(48_000, 1), 1.0, false);
        audio.push_frames(frames.iter().copied());
        let mut out = vec![0; audio.ring().len()];
        audio.ring().pop(&mut out);
        out
    }

    #[test]
    fn sources_mix_and_clip() {
        let a = bytes(&[100, -200, 30_000]);
        let b = bytes(&[1, 2, 30_000]);
        assert_eq!(mix(&[&a, &b]), vec![101, -198, i16::MAX]);
        let low = bytes(&[-30_000]);
        assert_eq!(mix(&[&low, &low]), vec![i16::MIN]);
    }

    #[test]
    fn health_counts_frames_and_keeps_the_loudest_sample() {
        let audio = StreamAudio::new(PcmFormat::for_output(48_000, 1), 1.0, false);
        assert_eq!(audio.health().peak, 0);
        audio.push_frames([bytes(&[0, 0]).as_slice()]);
        assert_eq!((audio.health().frames, audio.health().peak), (1, 0));
        audio.push_frames([bytes(&[300, i16::MIN]).as_slice()]);
        audio.push_frames([bytes(&[-50]).as_slice()]);
        assert_eq!((audio.health().frames, audio.health().peak), (3, 32_768));
    }

    #[test]
    fn frames_of_different_lengths_mix_over_the_longest() {
        let short = bytes(&[5]);
        let long = bytes(&[1, 2, 3]);
        assert_eq!(mix(&[&short, &long]), vec![6, 2, 3]);
        let mut odd = bytes(&[7]);
        odd.push(0xff);
        assert_eq!(mix(&[&odd]), vec![7]);
        assert!(mix(&[]).is_empty());
    }

    #[test]
    fn the_format_follows_the_device_up_to_stereo() {
        assert_eq!(PcmFormat::for_output(48_000, 1).channels, 1);
        assert_eq!(PcmFormat::for_output(44_100, 2).channels, 2);
        assert_eq!(PcmFormat::for_output(48_000, 6).channels, 2);
        let stereo = PcmFormat::for_output(48_000, 2);
        assert_eq!(stereo.interleaved_samples(10), 960);
        assert_eq!(
            PcmFormat::for_output(44_100, 1).interleaved_samples(10),
            441
        );
    }

    #[test]
    fn mono_and_stereo_frames_queue_whole() {
        for channels in [1u16, 2] {
            let format = PcmFormat::for_output(48_000, channels);
            let audio = StreamAudio::new(format, 1.0, false);
            let frame = bytes(&vec![3; format.interleaved_samples(10)]);
            audio.push_frames([frame.as_slice()]);
            assert_eq!(audio.ring().len(), format.interleaved_samples(10));
            assert_eq!(audio.health().frames, 1);
        }
    }

    #[test]
    fn a_stalled_output_keeps_the_newest_200_ms() {
        let format = PcmFormat::for_output(48_000, 2);
        let audio = StreamAudio::new(format, 1.0, false);
        let frame = bytes(&vec![1; format.interleaved_samples(10)]);
        for _ in 0..50 {
            audio.push_frames([frame.as_slice()]);
        }
        let len = audio.ring().len();
        assert!(len <= format.interleaved_samples(200), "{len}");
        assert!(len >= format.interleaved_samples(180), "{len}");
    }
}
