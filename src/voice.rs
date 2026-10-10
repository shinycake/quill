//! Voice notes: 5-bit waveform (TDLib `bytes`) and a local OGG capture.
//!
//! Waveform packing matches tdesktop `documentWaveformEncode5bit` /
//! `documentWaveformDecode` (5-bit samples, MSB first). TDLib 1.8.67
//! `voiceNote.waveform` and `inputVoiceNote.waveform` are that byte string.
//!
//! Capture runs in-process on every platform: cpal reads the default
//! microphone ([`crate::voice_input`]) and a pure-Rust Opus encoder writes
//! the OGG ([`crate::voice_opus`]). No ffmpeg, no libopus.

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(feature = "ui")]
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// tdesktop voice waveform is a short run of 5-bit bars (often about 100).
pub const WAVEFORM_BAR_CAP: usize = 64;

pub fn format_voice_duration(seconds: i32) -> String {
    let seconds = seconds.max(0);
    let minutes = seconds / 60;
    let rest = seconds % 60;
    format!("{minutes}:{rest:02}")
}

/// Pack amplitudes `0..=31` into TDLib / Telegram 5-bit waveform bytes.
pub fn encode_waveform_5bit(samples: &[u8]) -> Vec<u8> {
    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    let mut out = Vec::with_capacity((samples.len() * 5).div_ceil(8));
    for sample in samples {
        bits = (bits << 5) | u32::from(sample & 0x1F);
        bit_count += 5;
        while bit_count >= 8 {
            bit_count -= 8;
            out.push(((bits >> bit_count) & 0xFF) as u8);
        }
    }
    if bit_count > 0 {
        out.push(((bits << (8 - bit_count)) & 0xFF) as u8);
    }
    out
}

/// Inverse of [`encode_waveform_5bit`]. Padding bits shorter than 5 are dropped.
pub fn decode_waveform_5bit(encoded: &[u8]) -> Vec<u8> {
    let mut bits: u32 = 0;
    let mut bit_count: u32 = 0;
    let mut out = Vec::with_capacity(encoded.len() * 8 / 5);
    for &byte in encoded {
        bits = (bits << 8) | u32::from(byte);
        bit_count += 8;
        while bit_count >= 5 {
            bit_count -= 5;
            out.push(((bits >> bit_count) & 0x1F) as u8);
        }
    }
    out
}

pub fn waveform_base64(samples: &[u8]) -> String {
    STANDARD.encode(encode_waveform_5bit(samples))
}

pub fn waveform_bars_from_bytes(encoded: &[u8]) -> Vec<u8> {
    decode_waveform_5bit(encoded)
}

/// Finished capture ready for `inputMessageVoiceNote`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDraft {
    pub path: PathBuf,
    pub duration_secs: i32,
    pub bars: Vec<u8>,
}

impl VoiceDraft {
    pub fn waveform_b64(&self) -> String {
        waveform_base64(&self.bars)
    }
}

/// tdesktop `Player::kWaveformSamplesCount`: bars in a sent waveform.
pub const WAVEFORM_SAMPLES: usize = 100;

/// tdesktop `CollectWaveform`: per-10 ms peaks (`peak / 256`) squeezed to
/// `count` bars, scaled against 1.8× their mean (at least 2500) so a quiet
/// recording still fills the bubble. Fewer levels than bars keep one bar
/// per level.
pub fn collect_waveform(levels: &[u8], count: usize) -> Vec<u8> {
    if levels.is_empty() || count == 0 {
        return Vec::new();
    }
    let peaks: Vec<u32> = if levels.len() >= count {
        let mut peaks = Vec::with_capacity(count);
        let (mut peak, mut sum) = (0u32, 0usize);
        for &level in levels {
            peak = peak.max(u32::from(level) * 256);
            sum += count;
            if sum >= levels.len() {
                sum -= levels.len();
                peaks.push(peak);
                peak = 0;
            }
        }
        peaks
    } else {
        levels.iter().map(|&level| u32::from(level) * 256).collect()
    };
    let mean = peaks.iter().map(|&p| u64::from(p)).sum::<u64>() as f64 / peaks.len() as f64;
    let top = ((mean * 1.8) as u32).max(2500);
    peaks
        .iter()
        .map(|&peak| (peak.min(top) * 31 / top).min(31) as u8)
        .collect()
}

/// Recordings: `{media_cache_base}/captures`, created 0700. A sent voice
/// or video message keeps pointing at its recording, so it plays from here.
pub fn capture_root() -> PathBuf {
    crate::local_path::media_cache_base().join("captures")
}

/// `roots` plus the recordings folder, for display/playback sandboxing.
pub fn with_capture_root(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = capture_root();
    crate::local_path::ensure_private_dir(&root);
    roots.push(root);
    roots
}

/// A new recording's file: `{capture_root}/quill-{kind}-{pid}-{nanos}.{ext}`.
pub(crate) fn capture_path(kind: &str, ext: &str) -> PathBuf {
    let root = capture_root();
    crate::local_path::ensure_private_dir(&root);
    root.join(format!(
        "quill-{kind}-{}-{}.{ext}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// Recording time that stops while paused.
#[derive(Debug, Clone, Copy)]
pub struct RunClock {
    done: Duration,
    since: Option<Instant>,
}

impl RunClock {
    /// A clock that starts running at `now`.
    pub fn started_at(now: Instant) -> Self {
        Self {
            done: Duration::ZERO,
            since: Some(now),
        }
    }

    pub fn is_running(&self) -> bool {
        self.since.is_some()
    }

    pub fn pause_at(&mut self, now: Instant) {
        if let Some(since) = self.since.take() {
            self.done += now.saturating_duration_since(since);
        }
    }

    pub fn resume_at(&mut self, now: Instant) {
        self.since.get_or_insert(now);
    }

    pub fn elapsed_at(&self, now: Instant) -> Duration {
        self.done
            + self
                .since
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }
}

/// What the capture asks of its encoder thread.
#[cfg(feature = "ui")]
#[derive(Default)]
struct Control {
    /// The recording is paused: the thread drops audio, and flushes what it
    /// has so the file can be played.
    paused: AtomicBool,
    /// The thread has flushed the file since the pause.
    flushed: AtomicBool,
}

/// A running microphone capture: the cpal stream, the encoder thread it
/// feeds, and how many samples have been encoded.
#[cfg(feature = "ui")]
struct Live {
    mic: crate::voice_input::MicStream,
    worker: std::thread::JoinHandle<Result<u64, String>>,
    samples: Arc<std::sync::atomic::AtomicU64>,
    control: Arc<Control>,
}

#[cfg(not(feature = "ui"))]
struct Live;

/// Seconds without a single sample before the microphone counts as dead
/// (a refused permission can deliver nothing instead of an error).
#[cfg(feature = "ui")]
const NO_AUDIO_SECS: u64 = 3;

/// In-progress microphone capture (tdesktop `VoiceRecordBar`): the default
/// input is encoded to Opus OGG in-process while 10 ms levels feed the
/// live waveform.
pub struct VoiceCapture {
    pub path: PathBuf,
    clock: RunClock,
    live: Option<Live>,
    /// Bars shown while recording (the waveform so far).
    pub bars: Vec<u8>,
    levels: Arc<Mutex<Vec<u8>>>,
    fixed_seconds: Option<i32>,
    /// Screenshot fixtures must not be deleted on Cancel.
    keep_file: bool,
}

impl VoiceCapture {
    /// Start recording the default microphone.
    #[cfg(feature = "ui")]
    pub fn start() -> Result<Self, String> {
        use std::io::BufWriter;
        use std::sync::atomic::{AtomicU64, Ordering};

        let path = capture_path("voice", "ogg");
        let file = fs::File::create(&path).map_err(|err| err.to_string())?;
        let mut encoder = crate::voice_opus::VoiceEncoder::new(BufWriter::new(file))?;
        let (sender, receiver) = std::sync::mpsc::channel::<Vec<f32>>();
        let mic = match crate::voice_input::open_default(sender) {
            Ok(mic) => mic,
            Err(err) => {
                let _ = fs::remove_file(&path);
                return Err(err);
            }
        };
        let levels = Arc::new(Mutex::new(Vec::new()));
        let samples = Arc::new(AtomicU64::new(0));
        let control = Arc::new(Control::default());
        let worker = {
            let (levels, samples, control) = (levels.clone(), samples.clone(), control.clone());
            std::thread::spawn(move || {
                let mut sent = 0;
                let mut paused = false;
                // Ends when the stream (the only sender) is dropped.
                loop {
                    let chunk = match receiver.recv_timeout(Duration::from_millis(30)) {
                        Ok(chunk) => Some(chunk),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let want = control.paused.load(Ordering::Acquire);
                    if want != paused {
                        paused = want;
                        if paused {
                            encoder.pause()?;
                            control.flushed.store(true, Ordering::Release);
                        } else {
                            encoder.resume();
                            control.flushed.store(false, Ordering::Release);
                        }
                    }
                    let Some(chunk) = chunk.filter(|_| !paused) else {
                        continue;
                    };
                    encoder.push(&chunk)?;
                    samples.store(encoder.samples() as u64, Ordering::Relaxed);
                    if encoder.levels().len() > sent
                        && let Ok(mut shared) = levels.lock()
                    {
                        shared.extend_from_slice(&encoder.levels()[sent..]);
                        sent = encoder.levels().len();
                    }
                }
                let total = encoder.samples() as u64;
                encoder.finish()?;
                Ok(total)
            })
        };
        Ok(Self {
            path,
            clock: RunClock::started_at(Instant::now()),
            live: Some(Live {
                mic,
                worker,
                samples,
                control,
            }),
            bars: Vec::new(),
            levels,
            fixed_seconds: None,
            keep_file: false,
        })
    }

    #[cfg(not(feature = "ui"))]
    pub fn start() -> Result<Self, String> {
        Err("Recording needs the full Quill build.".into())
    }

    /// Screenshot / fixture bar. Does not open a microphone.
    pub fn preview(path: PathBuf, seconds: i32, bars: Vec<u8>) -> Self {
        Self {
            path,
            clock: RunClock::started_at(Instant::now()),
            live: None,
            bars,
            levels: Arc::default(),
            fixed_seconds: Some(seconds.max(0)),
            keep_file: true,
        }
    }

    pub fn elapsed_secs(&self) -> i32 {
        if let Some(seconds) = self.fixed_seconds {
            return seconds;
        }
        self.clock
            .elapsed_at(Instant::now())
            .as_secs()
            .min(u64::from(i32::MAX as u32)) as i32
    }

    /// Stop listening but keep what was recorded. The microphone is
    /// released so the system indicator goes off; the file is made
    /// playable for a preview.
    pub fn pause(&mut self) {
        if !self.clock.is_running() {
            return;
        }
        self.clock.pause_at(Instant::now());
        #[cfg(feature = "ui")]
        if let Some(live) = &self.live {
            use std::sync::atomic::Ordering;
            live.control.paused.store(true, Ordering::Release);
            live.mic.pause();
        }
    }

    /// Carry on recording after a [`Self::pause`].
    pub fn resume(&mut self) {
        if self.clock.is_running() {
            return;
        }
        self.clock.resume_at(Instant::now());
        #[cfg(feature = "ui")]
        if let Some(live) = &self.live {
            use std::sync::atomic::Ordering;
            live.control.paused.store(false, Ordering::Release);
            live.mic.play();
        }
    }

    pub fn is_paused(&self) -> bool {
        !self.clock.is_running()
    }

    /// The recording so far, once a pause has made the file playable.
    pub fn preview_path(&self) -> Option<&Path> {
        if !self.is_paused() {
            return None;
        }
        #[cfg(feature = "ui")]
        if let Some(live) = &self.live {
            use std::sync::atomic::Ordering;
            return live
                .control
                .flushed
                .load(Ordering::Acquire)
                .then_some(self.path.as_path());
        }
        // A fixture has no encoder to wait for.
        self.live.is_none().then_some(self.path.as_path())
    }

    /// Refresh the live bars from the levels heard so far.
    pub fn sample_bar(&mut self) {
        if self.fixed_seconds.is_some() {
            return;
        }
        if let Ok(levels) = self.levels.lock() {
            self.bars = collect_waveform(&levels, WAVEFORM_BAR_CAP);
        }
    }

    /// The reason recording stopped on its own (device unplugged, no
    /// microphone access), if it did.
    #[cfg(feature = "ui")]
    pub fn failure(&mut self) -> Option<String> {
        use std::sync::atomic::Ordering;
        let live = self.live.as_ref()?;
        let error = live.mic.error.lock().ok().and_then(|mut e| e.take());
        let reason = error.or_else(|| {
            let idle = self.clock.elapsed_at(Instant::now()).as_secs() >= NO_AUDIO_SECS
                && live.samples.load(Ordering::Relaxed) == 0;
            idle.then(|| crate::media_tools::no_audio_message(false))
        })?;
        self.stop_live(false);
        Some(reason)
    }

    #[cfg(not(feature = "ui"))]
    pub fn failure(&mut self) -> Option<String> {
        None
    }

    pub fn discard(mut self) {
        self.stop_live(false);
        if !self.keep_file {
            let _ = fs::remove_file(&self.path);
        }
    }

    /// Stop the encoder and keep the file when it holds audio.
    pub fn finish(mut self) -> Result<VoiceDraft, String> {
        let wall = self.clock.elapsed_at(Instant::now());
        let samples = self.stop_live(true);
        let len = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        let encoded = match samples {
            Some(Err(ref reason)) => Err(reason.clone()),
            Some(Ok(0)) => Err(crate::media_tools::no_audio_message(false)),
            _ if self.keep_file => Ok(()),
            _ if len == 0 || !self.path.is_file() => {
                Err(crate::media_tools::no_audio_message(false))
            }
            _ => Ok(()),
        };
        if let Err(reason) = encoded {
            if !self.keep_file {
                let _ = fs::remove_file(&self.path);
            }
            return Err(reason);
        }
        // The encoded sample count is exact; the wall clock only stands in
        // for fixtures.
        let duration_secs = match (self.fixed_seconds, samples) {
            (Some(fixed), _) => fixed,
            (None, Some(Ok(total))) => (total as f64 / 48_000.0).round() as i32,
            _ => wall.as_secs_f64().round() as i32,
        }
        .max(1);
        let levels = self.levels.lock().map(|l| l.clone()).unwrap_or_default();
        let mut bars = collect_waveform(&levels, WAVEFORM_SAMPLES);
        if bars.is_empty() {
            bars = std::mem::take(&mut self.bars);
        }
        if bars.is_empty() {
            bars.push(8);
        }
        Ok(VoiceDraft {
            path: std::mem::take(&mut self.path),
            duration_secs,
            bars,
        })
    }

    /// Close the microphone and join the encoder. `Some(result)` only when
    /// `graceful`; otherwise everything is just dropped.
    #[cfg(feature = "ui")]
    fn stop_live(&mut self, graceful: bool) -> Option<Result<u64, String>> {
        let live = self.live.take()?;
        drop(live.mic); // the stream's sender drops and the worker finishes
        let result = live
            .worker
            .join()
            .unwrap_or_else(|_| Err("The voice encoder crashed.".into()));
        graceful.then_some(result)
    }

    #[cfg(not(feature = "ui"))]
    fn stop_live(&mut self, _graceful: bool) -> Option<Result<u64, String>> {
        self.live = None;
        None
    }
}

impl Drop for VoiceCapture {
    fn drop(&mut self) {
        if self.live.is_some() {
            self.stop_live(false);
        }
    }
}

/// A control in the recording bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordControl {
    /// Hands-free recording: Esc stops cancelling.
    Lock,
    /// Stop listening and keep what was said.
    Pause,
    /// Listen again after a pause.
    Resume,
    /// Play what was recorded so far (only while paused).
    Preview,
    /// "Play once": the recipient can listen a single time.
    PlayOnce,
    /// Throw the recording away.
    Discard,
    Send,
}

/// What the bar needs to know to choose its controls.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecordBarFacts {
    /// A round video message (it cannot be paused).
    pub video: bool,
    pub paused: bool,
    /// Self-destructing messages exist only in private chats.
    pub once_allowed: bool,
}

/// The bar's controls, left to right. Tdesktop swaps the lock for a pause
/// button once locked and shows the play button on a paused recording;
/// Quill keeps the lock beside them and puts the one-time switch before
/// the delete button.
pub fn record_controls(facts: RecordBarFacts) -> Vec<RecordControl> {
    let mut controls = Vec::new();
    if facts.video {
        controls.push(RecordControl::Lock);
    } else if facts.paused {
        controls.push(RecordControl::Preview);
        controls.push(RecordControl::Resume);
    } else {
        controls.push(RecordControl::Lock);
        controls.push(RecordControl::Pause);
    }
    if facts.once_allowed {
        controls.push(RecordControl::PlayOnce);
    }
    controls.push(RecordControl::Discard);
    controls.push(RecordControl::Send);
    controls
}

impl RecordControl {
    /// Tooltip and accessible name (tdesktop `lng_record_*`).
    pub fn label(self, active: bool) -> &'static str {
        match self {
            RecordControl::Lock if active => "Unlock recording",
            RecordControl::Lock => "Lock recording",
            RecordControl::Pause => "Pause recording",
            RecordControl::Resume => "Resume recording",
            RecordControl::Preview if active => "Pause playback",
            RecordControl::Preview => "Play recording",
            RecordControl::PlayOnce if active => "The recipient will be able to listen only once.",
            RecordControl::PlayOnce => "Click to set this message to Play Once.",
            RecordControl::Discard => "Delete recording",
            RecordControl::Send => "Send recording",
        }
    }
}

/// True when `path` is an existing file (send-path check happens at the driver).
pub fn voice_file_ready(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_5bit_roundtrip_matches_tdesktop_packing() {
        let samples = [31u8, 0, 1, 16, 7];
        let encoded = encode_waveform_5bit(&samples);
        let decoded = decode_waveform_5bit(&encoded);
        assert_eq!(&decoded[..samples.len()], &samples);
        assert_eq!(waveform_base64(&samples), STANDARD.encode(&encoded));
    }

    #[test]
    fn collected_waveform_scales_against_the_mean_like_tdesktop() {
        // 200 levels squeeze to 100 bars of the louder level of each pair.
        let levels: Vec<u8> = (0..200).map(|i| if i % 2 == 0 { 0 } else { 40 }).collect();
        let bars = collect_waveform(&levels, 100);
        assert_eq!(bars.len(), 100);
        // Every peak equals the mean, which is below the 1.8× ceiling.
        assert!(bars.iter().all(|&bar| bar == 17), "{bars:?}");
        // Silence stays flat (the 2500 floor), short recordings keep a bar each.
        assert_eq!(collect_waveform(&[0, 0, 0], 100), vec![0, 0, 0]);
        assert_eq!(collect_waveform(&[255], 100), vec![17]);
        assert!(collect_waveform(&[], 100).is_empty());
    }

    #[test]
    fn the_run_clock_skips_paused_time() {
        let t0 = Instant::now();
        let mut clock = RunClock::started_at(t0);
        assert_eq!(
            clock.elapsed_at(t0 + Duration::from_secs(3)),
            Duration::from_secs(3)
        );
        clock.pause_at(t0 + Duration::from_secs(3));
        assert!(!clock.is_running());
        assert_eq!(
            clock.elapsed_at(t0 + Duration::from_secs(60)),
            Duration::from_secs(3)
        );
        // Pausing twice changes nothing.
        clock.pause_at(t0 + Duration::from_secs(70));
        clock.resume_at(t0 + Duration::from_secs(100));
        assert!(clock.is_running());
        assert_eq!(
            clock.elapsed_at(t0 + Duration::from_secs(104)),
            Duration::from_secs(7)
        );
        // Resuming twice keeps the first instant.
        clock.resume_at(t0 + Duration::from_secs(102));
        assert_eq!(
            clock.elapsed_at(t0 + Duration::from_secs(104)),
            Duration::from_secs(7)
        );
    }

    #[test]
    fn the_bar_offers_pause_while_recording_and_resume_when_paused() {
        use RecordControl::*;
        let live = RecordBarFacts::default();
        assert_eq!(record_controls(live), vec![Lock, Pause, Discard, Send]);
        let paused = RecordBarFacts {
            paused: true,
            ..live
        };
        assert_eq!(
            record_controls(paused),
            vec![Preview, Resume, Discard, Send]
        );
        // Play once only where self-destructing messages exist.
        let private = RecordBarFacts {
            once_allowed: true,
            ..paused
        };
        assert_eq!(
            record_controls(private),
            vec![Preview, Resume, PlayOnce, Discard, Send]
        );
    }

    #[test]
    fn a_video_message_cannot_pause_or_preview() {
        use RecordControl::*;
        let video = RecordBarFacts {
            video: true,
            paused: true,
            once_allowed: false,
        };
        assert_eq!(record_controls(video), vec![Lock, Discard, Send]);
    }

    #[test]
    fn control_labels_follow_tdesktop() {
        assert_eq!(RecordControl::Pause.label(false), "Pause recording");
        assert_eq!(RecordControl::Resume.label(false), "Resume recording");
        assert_eq!(RecordControl::Preview.label(false), "Play recording");
        assert_eq!(RecordControl::Discard.label(false), "Delete recording");
        assert_eq!(
            RecordControl::PlayOnce.label(true),
            "The recipient will be able to listen only once."
        );
    }

    #[test]
    fn a_fixture_pauses_and_previews_without_a_microphone() {
        let path = PathBuf::from("/tmp/quill-fixture.ogg");
        let mut capture = VoiceCapture::preview(path.clone(), 4, vec![1, 2]);
        assert!(!capture.is_paused());
        assert_eq!(capture.preview_path(), None);
        capture.pause();
        assert!(capture.is_paused());
        assert_eq!(capture.preview_path(), Some(path.as_path()));
        capture.resume();
        assert!(!capture.is_paused());
        assert_eq!(capture.preview_path(), None);
    }

    #[test]
    fn duration_format_is_minutes_and_seconds() {
        assert_eq!(format_voice_duration(0), "0:00");
        assert_eq!(format_voice_duration(5), "0:05");
        assert_eq!(format_voice_duration(65), "1:05");
        assert_eq!(format_voice_duration(-3), "0:00");
    }
}
