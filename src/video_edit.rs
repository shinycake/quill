//! The video editor for a staged local video (tdesktop `Editor::VideoEditor`
//! as opened by `OpenWithPreparedVideoFile`): trim to a range, pick the
//! cover frame, choose a quality and turn a video into a GIF (no sound).
//!
//! tdesktop transcodes the file when any of these changed
//! (`Editor::VideoEdited`) and sends the cover frame as the video's start
//! timestamp (`videoCoverOffset = cover - from`). This module holds the
//! numbers and the ffmpeg command lines; the panel is `ui/video_editor.rs`.
//! Crop, rotate and flip, which tdesktop's editor also has, are not here.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// The shortest range the trim handles can leave, in milliseconds.
pub const MIN_TRIM_MS: i64 = 500;
/// Quality levels by shorter side (tdesktop `VideoQualityLevels`).
const QUALITY_LEVELS: [u32; 4] = [360, 480, 720, 1080];
/// Side of the cover frame preview, in pixels.
pub const PREVIEW_SIDE: u32 = 640;

/// What the user changed in the editor. Times are milliseconds from the
/// start of the source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoEdits {
    pub from_ms: i64,
    pub till_ms: i64,
    /// The cover frame; always inside `from_ms..=till_ms`.
    pub cover_ms: i64,
    /// Target shorter side; 0 keeps the original resolution.
    pub quality: u32,
    /// Drop the sound and send the clip as a GIF.
    pub gif: bool,
}

impl VideoEdits {
    /// The whole clip, untouched.
    pub fn full(duration_ms: i64) -> Self {
        Self {
            from_ms: 0,
            till_ms: duration_ms.max(0),
            cover_ms: 0,
            quality: 0,
            gif: false,
        }
    }

    /// Fit the range and the cover into a clip of `duration_ms`. The handle
    /// that was not moved stays put unless the range can't otherwise keep
    /// `MIN_TRIM_MS`.
    pub fn clamped(mut self, duration_ms: i64, moved_start: bool) -> Self {
        let duration = duration_ms.max(0);
        let floor = MIN_TRIM_MS.min(duration);
        if moved_start {
            self.from_ms = self.from_ms.clamp(0, duration - floor);
            self.till_ms = self.till_ms.clamp(self.from_ms + floor, duration);
        } else {
            self.till_ms = self.till_ms.clamp(floor, duration);
            self.from_ms = self.from_ms.clamp(0, self.till_ms - floor);
        }
        self.cover_ms = self.cover_ms.clamp(self.from_ms, self.till_ms);
        self
    }

    /// Length of the kept part.
    pub fn length_ms(&self) -> i64 {
        (self.till_ms - self.from_ms).max(0)
    }

    /// The range is shorter than the clip.
    pub fn is_trimmed(&self, duration_ms: i64) -> bool {
        self.from_ms > 0 || self.till_ms < duration_ms
    }

    /// tdesktop `VideoEdited`: the file has to be encoded again. A changed
    /// cover alone only moves the start timestamp.
    pub fn needs_transcode(&self, duration_ms: i64) -> bool {
        self.is_trimmed(duration_ms) || self.quality != 0 || self.gif
    }

    /// `inputVideo.start_timestamp` in whole seconds, counted from the start
    /// of the file that is sent (the trimmed one, when it was cut).
    pub fn start_timestamp_secs(&self, duration_ms: i64) -> i32 {
        let base = if self.needs_transcode(duration_ms) {
            self.from_ms
        } else {
            0
        };
        i32::try_from((self.cover_ms - base).max(0) / 1000).unwrap_or(0)
    }
}

/// A time for the editor's labels: `0:07.4`, or `1:02:03.0` past an hour.
pub fn format_clock(ms: i64) -> String {
    let tenths = ms.max(0) / 100;
    let (secs, tenth) = (tenths / 10, tenths % 10);
    let (hours, minutes, seconds) = (secs / 3600, secs / 60 % 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{tenth}")
    } else {
        format!("{minutes}:{seconds:02}.{tenth}")
    }
}

/// The levels the quality slider offers for a clip: each fixed level below
/// the shorter side, then the original (`0`).
pub fn quality_levels(width: u32, height: u32) -> Vec<u32> {
    let shorter = width.min(height);
    let mut levels: Vec<u32> = QUALITY_LEVELS
        .iter()
        .copied()
        .take_while(|side| *side < shorter)
        .collect();
    if shorter > 0 {
        levels.push(0);
    }
    levels
}

/// Label of a quality level (tdesktop `VideoQualityLabel`).
pub fn quality_label(level: u32) -> String {
    if level == 0 {
        "Original".into()
    } else {
        format!("{level}p")
    }
}

/// Size after scaling the shorter side to `side` (even numbers, as the
/// encoder wants). `side` 0 or not smaller than the clip keeps the size.
pub fn scaled_size(width: u32, height: u32, side: u32) -> (u32, u32) {
    let shorter = width.min(height);
    if side == 0 || shorter == 0 || side >= shorter {
        return (width, height);
    }
    let scale = |long: u32| ((u64::from(long) * u64::from(side) / u64::from(shorter)) as u32) & !1;
    if width <= height {
        (side & !1, scale(height).max(2))
    } else {
        (scale(width).max(2), side & !1)
    }
}

/// A probed clip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSource {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
}

/// Read `ffprobe -of json` output with every stream listed.
pub fn parse_source(value: &Value) -> Option<VideoSource> {
    let streams = value.get("streams")?.as_array()?;
    let kind = |stream: &Value| {
        stream
            .get("codec_type")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let video = streams
        .iter()
        .find(|stream| kind(stream).as_deref() == Some("video"))?;
    let has_audio = streams
        .iter()
        .any(|stream| kind(stream).as_deref() == Some("audio"));
    let single = serde_json::json!({
        "streams": [video],
        "format": value.get("format").cloned().unwrap_or(Value::Null),
    });
    let (seconds, width, height) = crate::profile_photo_video::parse_probe(&single)?;
    let duration_ms = (seconds * 1000.0).round() as i64;
    (duration_ms > 0 && width > 0 && height > 0).then_some(VideoSource {
        duration_ms,
        width,
        height,
        has_audio,
    })
}

/// Run ffprobe on a local file. Blocks; call it off the UI thread.
pub fn probe_source(path: &Path) -> Option<VideoSource> {
    let output = crate::media_tools::command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,duration:stream_tags=rotate:stream_side_data=rotation:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_source(&serde_json::from_slice(&output.stdout).ok()?)
}

fn seconds(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| (*arg).to_string()).collect()
}

/// ffmpeg arguments that write the edited clip `output` (H.264 in MP4,
/// AAC sound unless the clip becomes a GIF).
pub fn encode_args(
    input: &str,
    output: &str,
    edits: &VideoEdits,
    source: &VideoSource,
) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(seconds(edits.from_ms));
    args.push("-t".into());
    args.push(seconds(edits.length_ms()));
    args.extend(strings(&["-i", input]));
    if edits.gif || !source.has_audio {
        args.push("-an".into());
    }
    let mut filters = Vec::new();
    if edits.quality != 0 && edits.quality < source.width.min(source.height) {
        filters.push(if source.width <= source.height {
            format!("scale={}:-2", edits.quality & !1)
        } else {
            format!("scale=-2:{}", edits.quality & !1)
        });
    }
    filters.push("format=yuv420p".into());
    args.push("-vf".into());
    args.push(filters.join(","));
    args.extend(strings(&[
        "-c:v", "libx264", "-preset", "veryfast", "-crf", "23",
    ]));
    if !edits.gif && source.has_audio {
        args.extend(strings(&["-c:a", "aac", "-b:a", "128k"]));
    }
    args.extend(strings(&["-movflags", "+faststart", output]));
    args
}

/// ffmpeg arguments that save the frame at `at_ms` as the image `output`,
/// at most `PREVIEW_SIDE` px on the longer side.
pub fn frame_args(input: &str, at_ms: i64, output: &str) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(seconds(at_ms.max(0)));
    args.extend(strings(&["-i", input, "-frames:v", "1", "-vf"]));
    args.push(format!(
        "scale='min({0},iw)':'min({0},ih)':force_original_aspect_ratio=decrease",
        PREVIEW_SIDE
    ));
    args.extend(strings(&["-q:v", "3", output]));
    args
}

fn work_dir() -> PathBuf {
    std::env::temp_dir().join("quill-video-edits")
}

fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let output = crate::media_tools::command("ffmpeg")
        .args(args)
        .output()
        .map_err(|_| "ffmpeg isn't available".to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err("ffmpeg could not process this video".into())
    }
}

/// Encode `source_path` with `edits` into a new file. Blocks; call it off
/// the UI thread.
pub fn render_edited(
    source_path: &Path,
    edits: &VideoEdits,
    source: &VideoSource,
) -> Result<PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|_| "couldn't prepare the video".to_string())?;
    let output = dir.join(format!("edit-{}.mp4", stamp()));
    let args = encode_args(
        &source_path.to_string_lossy(),
        &output.to_string_lossy(),
        edits,
        source,
    );
    run_ffmpeg(&args)?;
    crate::local_path::pick_send_path(&output).ok_or_else(|| "couldn't prepare the video".into())
}

/// Save the frame at `at_ms` of `source_path` as a JPEG, for the editor's
/// preview. Blocks; call it off the UI thread.
pub fn render_frame(source_path: &Path, at_ms: i64) -> Result<PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|_| "couldn't read the video".to_string())?;
    let output = dir.join(format!("frame-{}.jpg", stamp()));
    run_ffmpeg(&frame_args(
        &source_path.to_string_lossy(),
        at_ms,
        &output.to_string_lossy(),
    ))?;
    crate::local_path::pick_send_path(&output).ok_or_else(|| "couldn't read the video".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn source() -> VideoSource {
        VideoSource {
            duration_ms: 20_000,
            width: 1920,
            height: 1080,
            has_audio: true,
        }
    }

    #[test]
    fn clamp_keeps_a_minimum_range_and_the_cover_inside() {
        let edits = VideoEdits {
            from_ms: 19_900,
            till_ms: 20_000,
            cover_ms: 0,
            quality: 0,
            gif: false,
        };
        let fixed = edits.clamped(20_000, true);
        assert_eq!(fixed.from_ms, 19_500);
        assert_eq!(fixed.till_ms, 20_000);
        assert_eq!(fixed.cover_ms, 19_500);
        let moved_end = VideoEdits {
            from_ms: 5_000,
            till_ms: 5_100,
            cover_ms: 9_000,
            ..edits
        }
        .clamped(20_000, false);
        assert_eq!(moved_end.from_ms, 4_600);
        assert_eq!(moved_end.till_ms, 5_100);
        assert_eq!(moved_end.cover_ms, 5_100);
        let short = VideoEdits::full(300).clamped(300, true);
        assert_eq!((short.from_ms, short.till_ms), (0, 300));
    }

    #[test]
    fn only_a_cover_change_needs_no_transcode() {
        let mut edits = VideoEdits::full(20_000);
        assert!(!edits.needs_transcode(20_000));
        edits.cover_ms = 7_400;
        assert!(!edits.needs_transcode(20_000));
        assert_eq!(edits.start_timestamp_secs(20_000), 7);
        edits.from_ms = 3_000;
        edits.till_ms = 15_000;
        assert!(edits.needs_transcode(20_000));
        // The sent file starts at `from`, so the cover is counted from it.
        assert_eq!(edits.start_timestamp_secs(20_000), 4);
        let mut quality = VideoEdits::full(20_000);
        quality.quality = 720;
        assert!(quality.needs_transcode(20_000));
        let mut gif = VideoEdits::full(20_000);
        gif.gif = true;
        assert!(gif.needs_transcode(20_000));
    }

    #[test]
    fn clock_labels_show_tenths_and_hours_only_when_needed() {
        assert_eq!(format_clock(0), "0:00.0");
        assert_eq!(format_clock(7_450), "0:07.4");
        assert_eq!(format_clock(62_000), "1:02.0");
        assert_eq!(format_clock(3_723_900), "1:02:03.9");
        assert_eq!(format_clock(-5), "0:00.0");
    }

    #[test]
    fn quality_levels_stop_below_the_shorter_side() {
        assert_eq!(quality_levels(1920, 1080), [360, 480, 720, 0]);
        assert_eq!(quality_levels(1280, 720), [360, 480, 0]);
        assert_eq!(quality_levels(1080, 1920), [360, 480, 720, 0]);
        assert_eq!(quality_levels(320, 240), [0]);
        assert!(quality_levels(0, 0).is_empty());
        assert_eq!(quality_label(0), "Original");
        assert_eq!(quality_label(480), "480p");
    }

    #[test]
    fn scaled_size_keeps_the_shape_with_even_sides() {
        assert_eq!(scaled_size(1920, 1080, 720), (1280, 720));
        assert_eq!(scaled_size(1080, 1920, 720), (720, 1280));
        assert_eq!(scaled_size(1921, 1081, 480), (852, 480));
        assert_eq!(scaled_size(640, 360, 0), (640, 360));
        assert_eq!(scaled_size(640, 360, 720), (640, 360));
    }

    #[test]
    fn encode_cuts_scales_and_drops_sound_for_a_gif() {
        let edits = VideoEdits {
            from_ms: 1_500,
            till_ms: 6_000,
            cover_ms: 2_000,
            quality: 480,
            gif: false,
        };
        let args = encode_args("in.mp4", "out.mp4", &edits, &source());
        let at = |flag: &str| args.iter().position(|arg| arg == flag).unwrap();
        assert_eq!(args[at("-ss") + 1], "1.500");
        assert_eq!(args[at("-t") + 1], "4.500");
        assert_eq!(args[at("-vf") + 1], "scale=-2:480,format=yuv420p");
        assert!(args.contains(&"aac".to_string()));
        assert!(!args.contains(&"-an".to_string()));
        assert_eq!(args.last().unwrap(), "out.mp4");
        let gif = VideoEdits { gif: true, ..edits };
        let args = encode_args("in.mp4", "out.mp4", &gif, &source());
        assert!(args.contains(&"-an".to_string()));
        assert!(!args.contains(&"aac".to_string()));
        let portrait = VideoSource {
            width: 1080,
            height: 1920,
            ..source()
        };
        let args = encode_args("in.mp4", "out.mp4", &edits, &portrait);
        assert!(args.contains(&"scale=480:-2,format=yuv420p".to_string()));
        let silent = VideoSource {
            has_audio: false,
            ..source()
        };
        let args = encode_args("in.mp4", "out.mp4", &VideoEdits::full(20_000), &silent);
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn frame_args_seek_before_the_input() {
        let args = frame_args("in.mp4", 2_345, "f.jpg");
        assert_eq!(args[1], "-ss");
        assert_eq!(args[2], "2.345");
        assert_eq!(args.last().unwrap(), "f.jpg");
        assert!(args.iter().any(|arg| arg.contains("min(640,iw)")));
    }

    #[test]
    fn ffmpeg_trims_and_scales_a_generated_clip() {
        let dir = std::env::temp_dir().join(format!("quill-video-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("source.mp4");
        // Skipped where ffmpeg (with an H.264 encoder) isn't installed.
        let made = crate::media_tools::command("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=3:size=640x480:rate=15",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=3",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
            ])
            .arg(&clip)
            .output()
            .is_ok_and(|out| out.status.success());
        let Some(source) = made.then(|| probe_source(&clip)).flatten() else {
            return;
        };
        assert!(source.has_audio);
        assert!((2900..=3200).contains(&source.duration_ms), "{source:?}");
        let edits = VideoEdits {
            from_ms: 500,
            till_ms: 1500,
            cover_ms: 800,
            quality: 360,
            gif: true,
        }
        .clamped(source.duration_ms, true);
        let output = render_edited(&clip, &edits, &source).expect("the edit renders");
        let edited = probe_source(&output).expect("the edited clip reads");
        assert!((900..=1200).contains(&edited.duration_ms), "{edited:?}");
        assert_eq!((edited.width, edited.height), (480, 360));
        assert!(!edited.has_audio, "a GIF has no sound");
        let frame = render_frame(&output, 300).expect("a frame");
        assert!(frame.is_file());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(output);
        let _ = std::fs::remove_file(frame);
    }

    #[test]
    fn parse_source_reads_size_length_and_sound() {
        let probe = json!({
            "streams": [
                {"codec_type": "video", "width": 1280, "height": 720, "duration": "12.5"},
                {"codec_type": "audio", "duration": "12.5"}
            ],
            "format": {"duration": "12.6"}
        });
        let parsed = parse_source(&probe).unwrap();
        assert_eq!(parsed.duration_ms, 12_500);
        assert_eq!((parsed.width, parsed.height), (1280, 720));
        assert!(parsed.has_audio);
        let silent = json!({
            "streams": [{"codec_type": "video", "width": 64, "height": 48}],
            "format": {"duration": "3.0"}
        });
        let parsed = parse_source(&silent).unwrap();
        assert_eq!(parsed.duration_ms, 3_000);
        assert!(!parsed.has_audio);
        assert!(parse_source(&json!({"streams": [{"codec_type": "audio"}]})).is_none());
    }
}
