//! Voice notes: 5-bit waveform (TDLib `bytes`) and a local OGG capture.
//!
//! Waveform packing matches tdesktop `documentWaveformEncode5bit` /
//! `documentWaveformDecode` (5-bit samples, MSB first). TDLib 1.8.67
//! `voiceNote.waveform` and `inputVoiceNote.waveform` are that byte string.
//!
//! Capture shells out to `ffmpeg` (libopus, mono OGG) when it is installed.
//! Quill does not vendor libopus. A missing encoder is an error, not a fake file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
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

/// In-progress microphone capture (tdesktop `VoiceRecordBar`).
pub struct VoiceCapture {
    pub path: PathBuf,
    started: Instant,
    child: Option<Child>,
    pub bars: Vec<u8>,
    last_size: u64,
    fixed_seconds: Option<i32>,
    /// Screenshot fixtures must not be deleted on Cancel.
    keep_file: bool,
}

impl VoiceCapture {
    /// Start `ffmpeg` pulse → mono Opus OGG. Fails when ffmpeg is missing.
    pub fn start() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "quill-voice-{}-{}.ogg",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let child = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "pulse",
                "-i",
                "default",
                "-ac",
                "1",
                "-c:a",
                "libopus",
                "-b:a",
                "32k",
                "-application",
                "voip",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("voice recording needs ffmpeg ({err})"))?;
        Ok(Self {
            path,
            started: Instant::now(),
            child: Some(child),
            bars: Vec::new(),
            last_size: 0,
            fixed_seconds: None,
            keep_file: false,
        })
    }

    /// Screenshot / fixture bar. Does not open a microphone.
    pub fn preview(path: PathBuf, seconds: i32, bars: Vec<u8>) -> Self {
        Self {
            path,
            started: Instant::now(),
            child: None,
            bars,
            last_size: 0,
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

    /// One bar from bytes appended to the OGG since the last sample (capped).
    pub fn sample_bar(&mut self) {
        if self.bars.len() >= WAVEFORM_BAR_CAP {
            return;
        }
        let size = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        let delta = size.saturating_sub(self.last_size);
        self.last_size = size;
        let level = if delta == 0 {
            2
        } else {
            ((delta / 32).min(31) as u8).max(4)
        };
        self.bars.push(level);
    }

    pub fn discard(mut self) {
        self.stop_child(false);
        if !self.keep_file {
            let _ = fs::remove_file(&self.path);
        }
    }

    /// Stop the encoder and keep the file when it is non-empty.
    pub fn finish(mut self) -> Result<VoiceDraft, String> {
        self.stop_child(true);
        let len = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if len == 0 || !self.path.is_file() {
            if !self.keep_file {
                let _ = fs::remove_file(&self.path);
            }
            return Err("voice recording was empty".into());
        }
        let duration_secs = self.elapsed_secs().max(1);
        if self.bars.is_empty() {
            self.bars.push(8);
        }
        Ok(VoiceDraft {
            path: std::mem::take(&mut self.path),
            duration_secs,
            bars: std::mem::take(&mut self.bars),
        })
    }

    fn stop_child(&mut self, graceful: bool) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if graceful {
            let _ = Command::new("kill")
                .args(["-INT", &child.id().to_string()])
                .status();
            let _ = child.wait();
        } else {
            let _ = child.kill();
            let _ = child.wait();
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
    fn duration_format_is_minutes_and_seconds() {
        assert_eq!(format_voice_duration(0), "0:00");
        assert_eq!(format_voice_duration(5), "0:05");
        assert_eq!(format_voice_duration(65), "1:05");
        assert_eq!(format_voice_duration(-3), "0:00");
    }
}
