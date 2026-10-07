//! Voice notes: 5-bit waveform (TDLib `bytes`) and a local OGG capture.
//!
//! Waveform packing matches tdesktop `documentWaveformEncode5bit` /
//! `documentWaveformDecode` (5-bit samples, MSB first). TDLib 1.8.67
//! `voiceNote.waveform` and `inputVoiceNote.waveform` are that byte string.
//!
//! Capture shells out to `ffmpeg` (libopus, mono OGG) when it is installed,
//! recording the default microphone (AVFoundation on macOS, PulseAudio on
//! Linux).
//! Quill does not vendor libopus. A missing encoder is an error, not a fake file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Instant;

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

/// Levels are measured on 8 kHz mono, one peak per 10 ms (tdesktop
/// `waveformEach = kCaptureFrequency / 100`).
const LEVEL_RATE: usize = 8000;
const LEVEL_EACH: usize = LEVEL_RATE / 100;

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

/// Read 16-bit mono PCM from ffmpeg and keep one peak (`/ 256`) per 10 ms.
fn spawn_level_reader(mut pcm: impl std::io::Read + Send + 'static, levels: Arc<Mutex<Vec<u8>>>) {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let (mut peak, mut counted) = (0u16, 0usize);
        let mut odd: Option<u8> = None;
        while let Ok(read) = pcm.read(&mut buffer) {
            if read == 0 {
                break;
            }
            let mut fresh = Vec::new();
            let mut bytes = buffer[..read].iter().copied();
            while let Some(low) = odd.take().or_else(|| bytes.next()) {
                let Some(high) = bytes.next() else {
                    odd = Some(low);
                    break;
                };
                let sample = i16::from_le_bytes([low, high]);
                peak = peak.max(sample.unsigned_abs());
                counted += 1;
                if counted == LEVEL_EACH {
                    fresh.push((peak / 256) as u8);
                    peak = 0;
                    counted = 0;
                }
            }
            if !fresh.is_empty()
                && let Ok(mut levels) = levels.lock()
            {
                levels.extend(fresh);
            }
        }
    });
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

/// In-progress microphone capture (tdesktop `VoiceRecordBar`): ffmpeg
/// encodes Opus OGG and streams 8 kHz PCM back for the levels.
pub struct VoiceCapture {
    pub path: PathBuf,
    log: PathBuf,
    started: Instant,
    child: Option<Child>,
    /// Bars shown while recording (the waveform so far).
    pub bars: Vec<u8>,
    levels: Arc<Mutex<Vec<u8>>>,
    fixed_seconds: Option<i32>,
    /// Screenshot fixtures must not be deleted on Cancel.
    keep_file: bool,
}

impl VoiceCapture {
    /// Start recording the default microphone. Fails when ffmpeg is missing.
    pub fn start() -> Result<Self, String> {
        let input = crate::media_tools::capture_input(false)?;
        let path = capture_path("voice", "ogg");
        let log = path.with_extension("log");
        let log_file = fs::File::create(&log).map_err(|err| err.to_string())?;
        let mut child = crate::media_tools::command("ffmpeg")
            .args(["-y", "-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(&input.args)
            .args(["-map", input.audio, "-ac", "1", "-ar", "48000"])
            .args(["-c:a", "libopus", "-b:a", "32k", "-application", "voip"])
            .arg(&path)
            .args(["-map", input.audio, "-ac", "1", "-ar", "8000"])
            .args(["-f", "s16le", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(log_file)
            .spawn()
            .map_err(|err| format!("Voice messages need ffmpeg ({err})."))?;
        let levels = Arc::new(Mutex::new(Vec::new()));
        if let Some(stdout) = child.stdout.take() {
            spawn_level_reader(stdout, levels.clone());
        }
        Ok(Self {
            path,
            log,
            started: Instant::now(),
            child: Some(child),
            bars: Vec::new(),
            levels,
            fixed_seconds: None,
            keep_file: false,
        })
    }

    /// Screenshot / fixture bar. Does not open a microphone.
    pub fn preview(path: PathBuf, seconds: i32, bars: Vec<u8>) -> Self {
        Self {
            log: path.with_extension("log"),
            path,
            started: Instant::now(),
            child: None,
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
        self.started
            .elapsed()
            .as_secs()
            .min(u64::from(i32::MAX as u32)) as i32
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

    /// The reason recording stopped on its own (no microphone access, no
    /// device), once ffmpeg has exited.
    pub fn failure(&mut self) -> Option<String> {
        let child = self.child.as_mut()?;
        child.try_wait().ok().flatten()?;
        self.child = None;
        Some(crate::media_tools::capture_failure(&self.log, false))
    }

    pub fn discard(mut self) {
        self.stop_child(false);
        if !self.keep_file {
            let _ = fs::remove_file(&self.path);
        }
    }

    /// Stop the encoder and keep the file when it is non-empty.
    pub fn finish(mut self) -> Result<VoiceDraft, String> {
        let duration = self.started.elapsed();
        self.stop_child(true);
        let len = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if len == 0 || !self.path.is_file() {
            let reason = crate::media_tools::capture_failure(&self.log, false);
            if !self.keep_file {
                let _ = fs::remove_file(&self.path);
            }
            return Err(reason);
        }
        let duration_secs = self
            .fixed_seconds
            .unwrap_or(duration.as_secs_f64().round() as i32)
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

    fn stop_child(&mut self, graceful: bool) {
        if let Some(mut child) = self.child.take() {
            crate::media_tools::stop_capture(&mut child, graceful);
        }
        if !self.keep_file {
            let _ = fs::remove_file(&self.log);
        }
    }
}

impl Drop for VoiceCapture {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.stop_child(false);
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
    fn duration_format_is_minutes_and_seconds() {
        assert_eq!(format_voice_duration(0), "0:00");
        assert_eq!(format_voice_duration(5), "0:05");
        assert_eq!(format_voice_duration(65), "1:05");
        assert_eq!(format_voice_duration(-3), "0:00");
    }
}
