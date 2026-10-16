//! A live stream's sound played by Quill instead of ntgcalls, so it can
//! have a volume. Only stream-mode calls (channel live streams and live
//! stories) use it; 1:1 calls and video chats keep WebRTC's own output
//! and its echo cancellation.
//!
//! ntgcalls v3.0.3 hands the decoded audio of an external playback
//! writer to the frames callback (`stream_manager.cpp`,
//! `setup_audio_playback_callbacks`): one 10 ms frame per source, signed
//! 16-bit native-endian PCM, interleaved, at the description's rate and
//! channel count (`AudioReceiver::resample_frame`). The frames are mixed
//! into a [`ring::SampleRing`], and the output (`output`, UI builds)
//! plays them at the [`gain::Gain`].
//!
//! Design: `docs/decisions/stream-volume.md`.
#![deny(clippy::indexing_slicing)]

pub mod gain;
#[cfg(feature = "ui")]
mod output;
pub mod ring;
pub mod watch;

use gain::Gain;
use ring::SampleRing;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use watch::PlaybackHealth;

const RING_CAPACITY_MS: u32 = 200;
const OVERRUN_DROP_MS: u32 = 20;
const PLAYOUT_PRIME_MS: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcmFormat {
    pub rate: u32,
    /// 1 or 2: ntgcalls converts only between mono and stereo.
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

    pub fn push_frames<'a>(&self, frames: impl IntoIterator<Item = &'a [u8]>) {
        let mixed = mix(frames);
        if mixed.is_empty() {
            return;
        }
        self.ring.push(&mixed);
        self.frames.fetch_add(1, Ordering::Relaxed);
    }

    pub fn mark_output_failed(&self) {
        self.output_failed.store(true, Ordering::Relaxed);
    }

    pub fn health(&self) -> PlaybackHealth {
        PlaybackHealth {
            frames: self.frames.load(Ordering::Relaxed),
            output_failed: self.output_failed.load(Ordering::Relaxed),
        }
    }
}

/// Sum frames of native-endian `i16` samples, clipping at the `i16`
/// range. Frames of different lengths mix over the longest; a trailing
/// odd byte is ignored.
pub fn mix<'a>(frames: impl IntoIterator<Item = &'a [u8]>) -> Vec<i16> {
    let mut sum: Vec<i32> = Vec::new();
    for frame in frames {
        let samples = frame.len() / 2;
        if sum.len() < samples {
            sum.resize(samples, 0);
        }
        let decoded = frame
            .chunks_exact(2)
            .filter_map(|pair| <[u8; 2]>::try_from(pair).ok())
            .map(i16::from_ne_bytes);
        for (acc, sample) in sum.iter_mut().zip(decoded) {
            *acc = acc.saturating_add(i32::from(sample));
        }
    }
    sum.into_iter()
        .map(|value| value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16)
        .collect()
}

/// Whether Quill plays streams itself. `QUILL_STREAM_AUDIO_DEVICE=1`
/// leaves them on ntgcalls' device output; builds without the UI feature
/// have no output and always do.
pub fn quill_plays_streams() -> bool {
    plays_streams_from(
        cfg!(feature = "ui"),
        std::env::var("QUILL_STREAM_AUDIO_DEVICE").ok().as_deref(),
    )
}

pub fn plays_streams_from(has_output: bool, env: Option<&str>) -> bool {
    has_output && env.map(str::trim) != Some("1")
}

/// A stream's sound playing on an output device. Dropping it closes the
/// device.
pub struct StreamPlayer {
    audio: std::sync::Arc<StreamAudio>,
    #[cfg(feature = "ui")]
    _output: output::Output,
}

impl StreamPlayer {
    /// Open the output named `device` (an ntgcalls speaker name; `None`
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
    use super::{PcmFormat, StreamAudio, mix, plays_streams_from};

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }

    #[test]
    fn sources_mix_and_clip() {
        let a = bytes(&[100, -200, 30_000]);
        let b = bytes(&[1, 2, 30_000]);
        assert_eq!(mix([a.as_slice(), b.as_slice()]), vec![101, -198, i16::MAX]);
        let low = bytes(&[-30_000]);
        assert_eq!(mix([low.as_slice(), low.as_slice()]), vec![i16::MIN]);
    }

    #[test]
    fn frames_of_different_lengths_mix_over_the_longest() {
        let short = bytes(&[5]);
        let long = bytes(&[1, 2, 3]);
        assert_eq!(mix([short.as_slice(), long.as_slice()]), vec![6, 2, 3]);
        let mut odd = bytes(&[7]);
        odd.push(0xff);
        assert_eq!(mix([odd.as_slice()]), vec![7]);
        assert!(mix(std::iter::empty::<&[u8]>()).is_empty());
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

    #[test]
    fn the_env_switch_forces_device_playback() {
        assert!(plays_streams_from(true, None));
        assert!(plays_streams_from(true, Some("0")));
        assert!(!plays_streams_from(true, Some("1")));
        assert!(!plays_streams_from(true, Some(" 1 ")));
        assert!(!plays_streams_from(false, None));
    }
}
