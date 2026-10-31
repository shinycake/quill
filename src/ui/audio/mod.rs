//! In-process audio for voice notes, audio files, viewer soundtracks and
//! notification sounds. Everything plays through one `rodio` output
//! (CoreAudio, WASAPI, ALSA/PulseAudio), so no external player (ffplay,
//! afplay, osascript) is needed on any platform.
//!
//! Decoding: rodio's bundled symphonia handles MP3, AAC/M4A (MP4), FLAC,
//! Ogg Vorbis and WAV/PCM; Ogg/Opus (every Telegram voice note) goes through
//! the pure-Rust decoder in [`opus`]. Speed changes keep the pitch
//! ([`tempo`]). See `docs/decisions/codex-audio-in-process.md`.
#![deny(clippy::indexing_slicing)]

#[cfg(test)]
mod cpal_stream_tests;
pub(super) mod engine;
mod opus;
mod output;
mod tempo;

use std::cell::RefCell;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use quill::connect::SoundResolution;
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, Source};

use super::call_tones;
pub(super) use engine::{AudioEngine, EngineState, Simulation};
pub(super) use output::{Hold, Outlet, OutputPlayer, SharedOutput};
use tempo::{SharedSpeed, Tempo};

/// Which decoder reads a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Codec {
    /// Ogg container carrying Opus: the pure-Rust decoder.
    Opus,
    /// Everything symphonia knows (MP3, AAC/M4A, FLAC, Vorbis, WAV).
    Generic,
}

/// Pick the decoder from the file's first bytes. The content decides, not
/// the name: TDLib stores voice notes as `.oga`, `.ogg` or `.opus`, and a
/// `.ogg` may hold Vorbis or Opus. MIME type and extension only hint the
/// generic decoder (see [`format_hint`]).
pub(super) fn choose_codec(header: &[u8]) -> Codec {
    let head = header.get(..64).unwrap_or(header);
    let is_ogg = head.starts_with(b"OggS");
    let has_opus_head = head.windows(8).any(|w| w == b"OpusHead");
    if is_ogg && has_opus_head {
        Codec::Opus
    } else {
        Codec::Generic
    }
}

/// The container hint symphonia probes first: the file extension when it
/// names a known container, otherwise the MIME type.
pub(super) fn format_hint(extension: Option<&str>, mime: Option<&str>) -> Option<&'static str> {
    let by_ext = |ext: &str| match ext.to_ascii_lowercase().as_str() {
        "mp3" | "mpga" => Some("mp3"),
        "m4a" | "mp4" | "m4b" | "aac" | "mov" => Some("m4a"),
        "flac" => Some("flac"),
        "ogg" | "oga" | "opus" => Some("ogg"),
        "wav" | "wave" => Some("wav"),
        _ => None,
    };
    let by_mime = |mime: &str| match mime.to_ascii_lowercase().as_str() {
        "audio/mpeg" | "audio/mp3" | "audio/mpeg3" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" | "audio/aac" | "audio/mp4a-latm"
        | "video/mp4" => Some("m4a"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/ogg" | "audio/opus" | "audio/vorbis" | "application/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => Some("wav"),
        _ => None,
    };
    extension
        .and_then(by_ext)
        .or_else(|| mime.and_then(by_mime))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AudioError {
    /// The file can't be read.
    Open,
    /// No decoder understands the file.
    Unsupported,
    /// No audio output device.
    NoOutput,
    /// A seek has waited too long on the output device.
    DeviceStalled,
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::Open => f.write_str("couldn't open the audio file"),
            AudioError::Unsupported => f.write_str("this audio format can't be played"),
            AudioError::NoOutput => f.write_str("no audio output device"),
            AudioError::DeviceStalled => f.write_str("the audio device isn't responding"),
        }
    }
}

/// Open `path` as a rodio source, stretched to the shared speed.
fn open_source(path: &Path, speed: &SharedSpeed) -> Result<Box<dyn Source + Send>, AudioError> {
    Ok(Box::new(Tempo::new(open_decoded(path)?, speed.clone())))
}

/// Open `path` as the decoder's own source (no speed stretching).
fn open_decoded(path: &Path) -> Result<Box<dyn Source + Send>, AudioError> {
    let mut file = File::open(path).map_err(|_| AudioError::Open)?;
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut header = [0u8; 64];
    let read = read_prefix(&mut file, &mut header);
    file.seek(SeekFrom::Start(0))
        .map_err(|_| AudioError::Open)?;
    let reader = BufReader::new(file);
    Ok(match choose_codec(header.get(..read).unwrap_or_default()) {
        Codec::Opus => {
            Box::new(opus::OpusSource::new(reader).map_err(|_| AudioError::Unsupported)?)
        }
        Codec::Generic => {
            let extension = path.extension().and_then(|e| e.to_str());
            let mut builder = Decoder::builder()
                .with_data(reader)
                .with_byte_len(len)
                .with_seekable(true);
            if let Some(hint) = format_hint(extension, None) {
                builder = builder.with_hint(hint);
            }
            Box::new(builder.build().map_err(|_| AudioError::Unsupported)?)
        }
    })
}

fn read_prefix(file: &mut File, buf: &mut [u8]) -> usize {
    let mut filled = 0;
    while filled < buf.len() {
        let Some(rest) = buf.get_mut(filled..) else {
            break;
        };
        match file.read(rest) {
            Ok(0) | Err(_) => break,
            Ok(n) => filled += n,
        }
    }
    filled
}

thread_local! {
    /// The app's output, shared with video soundtracks (`StreamSound`),
    /// which are started from places that don't hold the app.
    static VIDEO_OUTPUT: RefCell<Option<SharedOutput>> = const { RefCell::new(None) };
}

/// Let video soundtracks play on the app's one output.
pub(super) fn share_output_with_video(output: &SharedOutput) {
    VIDEO_OUTPUT.with(|slot| *slot.borrow_mut() = Some(output.clone()));
}

/// A video's soundtrack from the in-process decoder
/// (`quill::video_decode::AudioTap`), on the shared output and stretched to
/// the playback speed without changing pitch. The decoder's player keeps
/// time from what this pulls, so pausing it pauses the picture too.
pub(super) struct StreamSound {
    player: OutputPlayer,
    speed: SharedSpeed,
    /// Set by a seek, taken by the stretcher ([`Flushed`]).
    flush: Arc<AtomicBool>,
}

impl StreamSound {
    /// `None` when there is no output device (the tap is dropped, and the
    /// video falls back to its wall clock).
    pub(super) fn start(
        tap: quill::video_decode::AudioTap,
        volume: f32,
        speed: f64,
        playing: bool,
    ) -> Option<Self> {
        Self::start_with(TapSource(tap), volume, speed, playing)
    }

    fn start_with<S>(source: S, volume: f32, speed: f64, playing: bool) -> Option<Self>
    where
        S: Source + Send + 'static,
    {
        let outlet =
            VIDEO_OUTPUT.with(|slot| slot.borrow().as_ref().and_then(SharedOutput::outlet))?;
        let speed = SharedSpeed::new(speed);
        let flush = Arc::new(AtomicBool::new(false));
        let player = outlet.player();
        player.set_volume(volume.clamp(0.0, 1.0));
        if !playing {
            player.pause();
        }
        player.append(Flushed {
            inner: Tempo::new(source, speed.clone()),
            flush: flush.clone(),
        });
        Some(Self {
            player,
            speed,
            flush,
        })
    }

    /// The decoder keeps time from what the output pulls: with no device
    /// the picture holds still too.
    pub(super) fn play(&self) {
        self.player.play();
    }

    pub(super) fn pause(&self) {
        self.player.pause();
    }

    /// Playing, but the device failed to open: nothing pulls the sound, so
    /// the decoder's clock would hold the picture still. Dropping the
    /// sound lets the clip run on its wall clock.
    pub(super) fn lost(&self) -> bool {
        self.player.lost()
    }

    pub(super) fn set_volume(&self, volume: f32) {
        self.player.set_volume(volume.clamp(0.0, 1.0));
    }

    pub(super) fn set_speed(&self, speed: f64) {
        self.speed.set(speed);
    }

    /// After a seek: drop what the speed stretcher buffered from before,
    /// at its next pull. Nothing waits for the audio thread or opens the
    /// device: a paused video seeks on the UI thread.
    pub(super) fn flush(&self) {
        self.flush.store(true, Ordering::Relaxed);
    }
}

/// A source that starts over (`try_seek` to zero: the stretcher drops what
/// it holds) when asked, at its next pull.
struct Flushed<S> {
    inner: S,
    flush: Arc<AtomicBool>,
}

impl<S: Source> Iterator for Flushed<S> {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.flush.load(Ordering::Relaxed) && self.flush.swap(false, Ordering::Relaxed) {
            let _ = self.inner.try_seek(Duration::ZERO);
        }
        self.inner.next()
    }
}

impl<S: Source> Source for Flushed<S> {
    fn current_span_len(&self) -> Option<usize> {
        self.inner.current_span_len()
    }

    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

impl Drop for StreamSound {
    fn drop(&mut self) {
        self.player.stop();
    }
}

/// The decoder's sound as a rodio source: endless (silence while the
/// decoder catches up or after the end), seeking is the decoder's job.
struct TapSource(quill::video_decode::AudioTap);

impl Iterator for TapSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        Some(self.0.next_sample())
    }
}

impl Source for TapSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        rodio::ChannelCount::new(self.0.channels()).unwrap_or(rodio::ChannelCount::MIN)
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        rodio::SampleRate::new(self.0.sample_rate()).unwrap_or(rodio::SampleRate::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }

    fn try_seek(&mut self, _pos: Duration) -> Result<(), rodio::source::SeekError> {
        // The decoder already moved; this only resets the stretcher above.
        Ok(())
    }
}

/// What a notification plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NotificationSound {
    /// The app's synthesized tone.
    DefaultTone,
    /// A downloaded sound file.
    File(PathBuf),
}

/// Map the driver's sound resolution to something playable now. A sound
/// still downloading plays when the download completes
/// (`Session::pending_sound_plays`), not now.
pub(super) fn notification_sound(resolution: SoundResolution) -> Option<NotificationSound> {
    match resolution {
        SoundResolution::DefaultTone => Some(NotificationSound::DefaultTone),
        SoundResolution::FilePath(path) => Some(NotificationSound::File(path)),
        SoundResolution::Pending => None,
    }
}

/// Plays notification sounds on the shared output, over any voice note
/// without cutting it. A new sound replaces the previous one.
pub(super) struct NotificationSounds {
    output: SharedOutput,
    player: Option<OutputPlayer>,
}

impl NotificationSounds {
    pub(super) fn new(output: SharedOutput) -> Self {
        Self {
            output,
            player: None,
        }
    }

    /// Play `sound`; silent when there is no output. A sound file that
    /// can't be decoded falls back to the default tone, so the alert is
    /// never lost.
    pub(super) fn play(&mut self, sound: NotificationSound) {
        self.play_at(sound, None);
    }

    /// Like [`Self::play`], with a per-chat or per-scope volume percent
    /// that replaces the app-wide one (`quill::notify_volume`).
    pub(super) fn play_at(&mut self, sound: NotificationSound, volume_percent: Option<u8>) {
        if let Some(old) = self.player.take() {
            old.stop();
        }
        let Some(outlet) = self.output.outlet() else {
            return;
        };
        let player = outlet.player();
        // Settings > Notifications volume (tdesktop `notificationsVolume`).
        player.set_volume(quill::notify_volume::resolve_gain(
            volume_percent,
            quill::notify_prefs::current().gain(),
        ));
        let source: Option<Box<dyn Source + Send>> = match &sound {
            NotificationSound::File(path) => open_source(path, &SharedSpeed::new(1.0)).ok(),
            NotificationSound::DefaultTone => None,
        };
        match source {
            Some(source) => player.append_alert(source),
            None => player.append_alert(default_tone()),
        }
        self.player = Some(player);
    }
}

fn default_tone() -> SamplesBuffer {
    SamplesBuffer::new(
        NonZero::<u16>::MIN,
        NonZero::new(call_tones::SAMPLE_RATE).unwrap_or(NonZero::<u32>::MIN),
        call_tones::notification(),
    )
}

#[cfg(test)]
mod tests {
    use super::opus::{OpusSource, packet_frames};
    use super::tempo::{SharedSpeed, Tempo, clamp_speed};
    use super::{
        Codec, NotificationSound, SharedOutput, StreamSound, choose_codec, format_hint,
        notification_sound, open_source, share_output_with_video,
    };
    use quill::connect::SoundResolution;
    use rodio::Source;
    use rodio::buffer::SamplesBuffer;
    use std::num::NonZero;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    /// A 0.6 s, 440 Hz mono Opus voice clip (`ffmpeg -f lavfi -i
    /// sine=frequency=440:duration=0.6 -ac 1 -c:a libopus -b:a 16k`),
    /// generated for this repo (CC0).
    const OPUS_FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/tone-440hz.opus.ogg");

    fn wav(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data_len = (samples.len() * 2) as u32;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + data_len).to_le_bytes());
        out.extend(b"WAVEfmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * u32::from(channels) * 2).to_le_bytes());
        out.extend((channels * 2).to_le_bytes());
        out.extend(16u16.to_le_bytes());
        out.extend(b"data");
        out.extend(data_len.to_le_bytes());
        for s in samples {
            out.extend(s.to_le_bytes());
        }
        out
    }

    fn sine(rate: u32, seconds: f32, freq: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|i| 0.5 * (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    fn buffer(rate: u32, samples: Vec<f32>) -> SamplesBuffer {
        SamplesBuffer::new(NonZero::<u16>::MIN, NonZero::new(rate).unwrap(), samples)
    }

    fn scratch(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let dir = quill::scratch::process_dir("quill-audio-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn codec_is_chosen_by_content_not_name() {
        assert_eq!(choose_codec(OPUS_FIXTURE), Codec::Opus);
        assert_eq!(choose_codec(&wav(8000, 1, &[0; 8])), Codec::Generic);
        assert_eq!(choose_codec(b"ID3\x04\0\0\0\0\0\0"), Codec::Generic);
        assert_eq!(choose_codec(b"\xff\xfb\x90\x00"), Codec::Generic);
        assert_eq!(choose_codec(b"fLaC\0\0\0\x22"), Codec::Generic);
        assert_eq!(choose_codec(b""), Codec::Generic);
        // An Ogg that is not Opus (Vorbis) takes the generic decoder.
        let mut vorbis = b"OggS\0\x02".to_vec();
        vorbis.extend([0u8; 22]);
        vorbis.extend(b"\x01\x1e\x01vorbis");
        assert_eq!(choose_codec(&vorbis), Codec::Generic);
    }

    #[test]
    fn format_hint_prefers_extension_then_mime() {
        assert_eq!(format_hint(Some("MP3"), None), Some("mp3"));
        assert_eq!(format_hint(Some("m4a"), Some("audio/mpeg")), Some("m4a"));
        assert_eq!(format_hint(Some("oga"), None), Some("ogg"));
        assert_eq!(format_hint(Some("opus"), None), Some("ogg"));
        assert_eq!(format_hint(None, Some("audio/mpeg")), Some("mp3"));
        assert_eq!(format_hint(Some("bin"), Some("audio/x-wav")), Some("wav"));
        assert_eq!(format_hint(Some("flac"), None), Some("flac"));
        assert_eq!(format_hint(Some("xyz"), Some("text/plain")), None);
        assert_eq!(format_hint(None, None), None);
    }

    #[test]
    fn opus_packet_durations_follow_the_toc() {
        // CELT 20 ms single frame (config 31, code 0).
        assert_eq!(packet_frames(&[31 << 3]), Some(960));
        // SILK 60 ms, two frames (config 3, code 1).
        assert_eq!(packet_frames(&[(3 << 3) | 1, 0]), Some(5760));
        // Hybrid 10 ms, arbitrary frame count 3 (config 12, code 3).
        assert_eq!(packet_frames(&[(12 << 3) | 3, 3]), Some(1440));
        assert_eq!(packet_frames(&[]), None);
        assert_eq!(packet_frames(&[3, 0]), None);
    }

    #[test]
    fn opus_fixture_decodes_to_its_duration() {
        let source = OpusSource::new(std::io::Cursor::new(OPUS_FIXTURE)).expect("opus");
        assert_eq!(source.channels().get(), 1);
        assert_eq!(source.sample_rate().get(), 48_000);
        let total = source.total_duration().expect("duration").as_secs_f64();
        assert!((total - 0.6).abs() < 0.03, "duration {total}");
        let samples: Vec<f32> = source.collect();
        let secs = samples.len() as f64 / 48_000.0;
        assert!((secs - total).abs() < 0.001, "decoded {secs}s vs {total}s");
        let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!(peak > 0.08 && peak < 1.0, "peak {peak}");
    }

    #[test]
    fn opus_seek_lands_on_the_requested_position() {
        let full: Vec<f32> = OpusSource::new(std::io::Cursor::new(OPUS_FIXTURE))
            .unwrap()
            .collect();
        let mut source = OpusSource::new(std::io::Cursor::new(OPUS_FIXTURE)).unwrap();
        source.try_seek(Duration::from_millis(250)).unwrap();
        let tail: Vec<f32> = source.by_ref().collect();
        let expected = full.len() - 12_000;
        assert!(
            tail.len().abs_diff(expected) <= 1,
            "{} vs {expected}",
            tail.len()
        );
        // Past the end: nothing left; back to 0: everything again.
        source.try_seek(Duration::from_secs(60)).unwrap();
        assert_eq!(source.by_ref().count(), 0);
        source.try_seek(Duration::ZERO).unwrap();
        assert_eq!(source.count(), full.len());
    }

    #[test]
    fn opus_rejects_other_ogg_and_garbage() {
        assert!(OpusSource::new(std::io::Cursor::new(b"not ogg at all".to_vec())).is_err());
        assert!(OpusSource::new(std::io::Cursor::new(Vec::new())).is_err());
    }

    #[test]
    fn open_source_plays_wav_regardless_of_extension() {
        let samples: Vec<i16> = sine(16_000, 0.25, 300.0)
            .iter()
            .map(|s| (s * 32767.0) as i16)
            .collect();
        for name in ["voice.bin", "clip.wav"] {
            let path = scratch(name, &wav(16_000, 1, &samples));
            let source = open_source(&path, &SharedSpeed::new(1.0)).expect("wav");
            assert_eq!(source.sample_rate().get(), 16_000);
            let count = source.count();
            assert!(count.abs_diff(samples.len()) < 1000, "{name}: {count}");
        }
    }

    #[test]
    fn open_source_plays_the_opus_fixture_and_rejects_garbage() {
        let speed = SharedSpeed::new(1.0);
        let path = scratch("voice.oga", OPUS_FIXTURE);
        let source = open_source(&path, &speed).expect("opus");
        assert_eq!(source.sample_rate().get(), 48_000);
        assert!(source.count() > 20_000);
        let junk = scratch("junk.mp3", b"definitely not audio, just text");
        assert!(open_source(&junk, &speed).is_err());
        assert!(open_source(&junk.with_file_name("missing.mp3"), &speed).is_err());
    }

    #[test]
    fn tempo_at_1x_is_transparent() {
        let input = sine(16_000, 0.5, 220.0);
        let output: Vec<f32> =
            Tempo::new(buffer(16_000, input.clone()), SharedSpeed::new(1.0)).collect();
        assert!(output.len().abs_diff(input.len()) < 700, "{}", output.len());
        let n = input.len().min(output.len());
        // Skip the first half block (fade-in) and the tail.
        let worst = (700..n - 700)
            .map(|i| (input[i] - output[i]).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-3, "worst deviation {worst}");
    }

    #[test]
    fn tempo_scales_duration_and_keeps_the_pitch() {
        let rate = 16_000;
        let input = sine(rate, 1.0, 200.0);
        for (speed, expect) in [(2.0, 0.5), (1.5, 1.0 / 1.5), (0.5, 2.0)] {
            let out: Vec<f32> =
                Tempo::new(buffer(rate, input.clone()), SharedSpeed::new(speed)).collect();
            let secs = out.len() as f64 / f64::from(rate);
            assert!(
                (secs - expect).abs() < 0.08,
                "{speed}x: {secs}s vs {expect}s"
            );
            // Pitch: rising zero crossings per second stay ~200.
            let body = &out[2000..out.len() - 2000];
            let crossings = body
                .windows(2)
                .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
                .count();
            let hz = crossings as f64 / (body.len() as f64 / f64::from(rate));
            assert!((hz - 200.0).abs() < 8.0, "{speed}x pitch {hz} Hz");
        }
    }

    #[test]
    fn tempo_speed_changes_mid_stream_and_seek_restarts() {
        let rate = 16_000;
        let speed = SharedSpeed::new(1.0);
        let mut tempo = Tempo::new(buffer(rate, sine(rate, 1.0, 200.0)), speed.clone());
        for _ in 0..4000 {
            tempo.next();
        }
        speed.set(2.0);
        let rest = tempo.by_ref().count();
        // 4000 samples at 1x, then the remaining ~0.75 s at 2x.
        assert!(rest < 7000, "{rest}");
        assert!(rest > 4000, "{rest}");
        // Seeking rewinds the inner source and restarts the stretcher.
        let mut again = Tempo::new(buffer(rate, sine(rate, 1.0, 200.0)), SharedSpeed::new(1.0));
        again.by_ref().take(5000).count();
        again.try_seek(Duration::from_millis(500)).unwrap();
        let tail = again.count();
        assert!(tail.abs_diff(8000) < 700, "{tail}");
    }

    #[test]
    fn speed_is_clamped_to_the_supported_span() {
        assert_eq!(clamp_speed(0.1), 0.5);
        assert_eq!(clamp_speed(9.0), 2.0);
        assert_eq!(clamp_speed(1.2), 1.2);
        assert_eq!(clamp_speed(f64::NAN), 1.0);
        assert_eq!(SharedSpeed::new(7.0).get(), 2.0);
    }

    #[test]
    fn notification_sound_follows_the_resolution() {
        assert_eq!(
            notification_sound(SoundResolution::DefaultTone),
            Some(NotificationSound::DefaultTone)
        );
        let path = std::path::PathBuf::from("/tmp/ding.mp3");
        assert_eq!(
            notification_sound(SoundResolution::FilePath(path.clone())),
            Some(NotificationSound::File(path))
        );
        // Still downloading: nothing now, it plays when the file lands.
        assert_eq!(notification_sound(SoundResolution::Pending), None);
    }

    /// An endless silent soundtrack that counts its seeks.
    struct Seeks(Arc<AtomicUsize>);

    impl Iterator for Seeks {
        type Item = f32;

        fn next(&mut self) -> Option<f32> {
            Some(0.0)
        }
    }

    impl Source for Seeks {
        fn current_span_len(&self) -> Option<usize> {
            None
        }

        fn channels(&self) -> rodio::ChannelCount {
            NonZero::<u16>::MIN
        }

        fn sample_rate(&self) -> rodio::SampleRate {
            NonZero::new(48_000).unwrap()
        }

        fn total_duration(&self) -> Option<Duration> {
            None
        }

        fn try_seek(&mut self, _pos: Duration) -> Result<(), rodio::source::SeekError> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }

    /// A video seek returns at once, even while the device is opening (a
    /// paused video seeks on the UI thread); the soundtrack's stretcher
    /// starts over at its next pull.
    #[test]
    fn a_video_seek_never_waits_for_the_device() {
        let (output, gate) = SharedOutput::gated();
        share_output_with_video(&output);
        let seeks = Arc::new(AtomicUsize::new(0));
        let sound = StreamSound::start_with(Seeks(seeks.clone()), 1.0, 1.0, true).unwrap();
        sound.flush();
        assert_eq!(seeks.load(Ordering::Relaxed), 0, "nothing pulled yet");
        gate.answer(true);
        output.settle_device();
        gate.pull(512);
        assert_eq!(seeks.load(Ordering::Relaxed), 1);
        gate.pull(512);
        assert_eq!(seeks.load(Ordering::Relaxed), 1, "once per seek");
    }
}
