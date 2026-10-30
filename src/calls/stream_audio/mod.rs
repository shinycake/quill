//! A call's sound played by Quill: the call library has no audio devices
//! (as Telegram Desktop builds tgcalls), so every call's sound comes out
//! here, and streams get a volume.
//!
//! The library hands over the call's decoded sound in 10 ms frames of
//! signed 16-bit native-endian PCM, 48 kHz mono ([`SOURCE_RATE`]). The
//! frames are mixed, converted to the output's rate when it is not 48 kHz,
//! and queued as mono in a [`ring::SampleRing`]; the output (`output`, UI
//! builds) spreads them over the device's channels at the [`gain::Gain`].
//!
//! Design: `docs/decisions/stream-volume.md`,
//! `docs/decisions/call-sound-output.md`.
#![deny(clippy::indexing_slicing)]

pub mod gain;
#[cfg(feature = "ui")]
mod output;
pub mod ring;
pub mod watch;

use crate::locks::LockExt;
use crate::voice_opus::Resampler;
use gain::Gain;
use ring::SampleRing;
use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use watch::PlaybackHealth;

/// The rate of the call library's sound (`QC_AUDIO_RATE`), always mono.
pub const SOURCE_RATE: u32 = 48_000;

const RING_CAPACITY_MS: u32 = 200;
const OVERRUN_DROP_MS: u32 = 20;
const PLAYOUT_PRIME_MS: u32 = 60;

fn samples(rate: u32, ms: u32) -> usize {
    usize::try_from(u64::from(rate) * u64::from(ms) / 1000).unwrap_or(usize::MAX)
}

pub struct StreamAudio {
    /// The output's rate; the ring holds mono samples at it.
    rate: u32,
    ring: SampleRing,
    gain: Gain,
    /// Only used when `rate` is not [`SOURCE_RATE`].
    convert: Mutex<RateConversion>,
    frames: AtomicU64,
    peak: AtomicU16,
    output_failed: AtomicBool,
}

/// [`SOURCE_RATE`] to the output's rate, with buffers that keep their
/// capacity between frames.
struct RateConversion {
    resampler: Resampler,
    mixed: Vec<f32>,
    converted: Vec<f32>,
}

impl StreamAudio {
    /// Sound for an output running at `rate`.
    pub fn new(rate: u32, volume: f32, muted: bool) -> Self {
        let rate = rate.max(1);
        Self {
            rate,
            ring: SampleRing::new(
                samples(rate, RING_CAPACITY_MS),
                samples(rate, OVERRUN_DROP_MS),
            ),
            gain: Gain::new(volume, muted),
            convert: Mutex::new(RateConversion {
                resampler: Resampler::between(SOURCE_RATE, rate),
                mixed: Vec::new(),
                converted: Vec::new(),
            }),
            frames: AtomicU64::new(0),
            peak: AtomicU16::new(0),
            output_failed: AtomicBool::new(false),
        }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn ring(&self) -> &SampleRing {
        &self.ring
    }

    pub fn gain(&self) -> &Gain {
        &self.gain
    }

    pub fn prime_samples(&self) -> usize {
        samples(self.rate, PLAYOUT_PRIME_MS)
    }

    /// Runs on the engine's audio thread every 10 ms, so it allocates only
    /// while the conversion buffers first grow: the batch is mixed sample
    /// by sample into the ring.
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
        let mixed = |index| {
            let sample = mixed_sample(frames.clone(), index);
            peak.set(peak.get().max(sample.unsigned_abs()));
            sample
        };
        if self.rate == SOURCE_RATE {
            self.ring.push_with(len, mixed);
        } else {
            let mut convert = self.convert.lock_or_recover();
            let RateConversion {
                resampler,
                mixed: source,
                converted,
            } = &mut *convert;
            source.clear();
            source.extend((0..len).map(|index| f32::from(mixed(index))));
            converted.clear();
            resampler.process(source, converted);
            self.ring.push_with(converted.len(), |index| {
                converted.get(index).map_or(0, |&sample| {
                    sample
                        .round()
                        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
                })
            });
        }
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
    use super::StreamAudio;

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }

    /// What one batch puts in the ring.
    fn mix(frames: &[&[u8]]) -> Vec<i16> {
        let audio = StreamAudio::new(48_000, 1.0, false);
        audio.push_frames(frames.iter().copied());
        drain(&audio)
    }

    fn drain(audio: &StreamAudio) -> Vec<i16> {
        let mut out = vec![0; audio.ring().len()];
        audio.ring().pop(&mut out);
        out
    }

    /// A 10 ms frame of the call library's sound: a 440 Hz tone.
    fn tone_frame(index: usize) -> Vec<u8> {
        let samples: Vec<i16> = (index * 480..(index + 1) * 480)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                (8_000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16
            })
            .collect();
        bytes(&samples)
    }

    /// Rising zero crossings per second of `samples` played at `rate`.
    fn pitch(samples: &[i16], rate: u32) -> f32 {
        let rising = samples
            .windows(2)
            .filter(|pair| matches!(pair, [a, b] if *a < 0 && *b >= 0))
            .count();
        rising as f32 * rate as f32 / samples.len() as f32
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
        let audio = StreamAudio::new(48_000, 1.0, false);
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

    /// 2026-10-30: on a stereo output the 48 kHz mono call sound was queued
    /// as stereo, so it played twice as fast and an octave high. The ring
    /// holds 10 ms of output per 10 ms frame, whatever the output's rate.
    #[test]
    fn a_frame_queues_ten_ms_at_the_output_rate() {
        for rate in [16_000, 24_000, 44_100, 48_000, 96_000] {
            let audio = StreamAudio::new(rate, 1.0, false);
            let mut queued = 0;
            for index in 0..10 {
                audio.push_frames([tone_frame(index).as_slice()]);
                queued += drain(&audio).len();
            }
            let want = rate as usize / 10;
            assert!(queued.abs_diff(want) <= 1, "{rate} Hz: {queued} of {want}");
        }
    }

    #[test]
    fn the_tone_keeps_its_pitch_at_any_output_rate() {
        for rate in [16_000, 44_100, 48_000, 96_000] {
            let audio = StreamAudio::new(rate, 1.0, false);
            let mut played = Vec::new();
            for index in 0..100 {
                audio.push_frames([tone_frame(index).as_slice()]);
                played.extend(drain(&audio));
            }
            let heard = pitch(&played, rate);
            assert!((heard - 440.0).abs() < 2.0, "{rate} Hz: {heard} Hz");
        }
    }

    #[test]
    fn a_stalled_output_keeps_the_newest_200_ms() {
        for rate in [44_100, 48_000] {
            let audio = StreamAudio::new(rate, 1.0, false);
            for index in 0..50 {
                audio.push_frames([tone_frame(index).as_slice()]);
            }
            let len = audio.ring().len();
            assert!(len <= super::samples(rate, 200), "{rate}: {len}");
            assert!(len >= super::samples(rate, 180), "{rate}: {len}");
        }
    }
}
