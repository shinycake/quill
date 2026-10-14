//! The video editor for a staged local video (tdesktop `Editor::VideoEditor`
//! as opened by `OpenWithPreparedVideoFile`): trim to a range, pick the
//! cover frame, choose a quality and turn a video into a GIF (no sound).
//!
//! tdesktop transcodes the file when any of these changed
//! (`Editor::VideoEdited`) and sends the cover frame as the video's start
//! timestamp (`videoCoverOffset = cover - from`). This module holds the
//! numbers and the ffmpeg command lines; the panel is `ui/video_editor.rs`
//! and the controls' geometry is `video_timeline`.
#![deny(clippy::indexing_slicing)]

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
    /// Clockwise quarter turns, 0 to 3 (tdesktop `PhotoModifications::angle`).
    pub turns: u8,
    /// Mirrored left to right on the screen, after the turns (tdesktop
    /// rotates, then mirrors).
    pub flipped: bool,
    /// The part of the turned, mirrored frame that is kept.
    pub crop: Option<Crop>,
}

/// The kept part of the frame as thousandths of the turned frame (tdesktop
/// keeps a pixel `QRect`; thousandths keep the model independent of size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

/// The smallest crop side, in thousandths.
pub const MIN_CROP: u16 = 100;

impl Crop {
    /// The whole frame.
    pub const FULL: Self = Self {
        x: 0,
        y: 0,
        w: 1000,
        h: 1000,
    };

    /// Keep the crop inside the frame and at least `MIN_CROP` wide and high.
    pub fn clamped(self) -> Self {
        let w = self.w.clamp(MIN_CROP, 1000);
        let h = self.h.clamp(MIN_CROP, 1000);
        Self {
            x: self.x.min(1000 - w),
            y: self.y.min(1000 - h),
            w,
            h,
        }
    }

    /// The same part of the frame after a clockwise quarter turn.
    pub fn turned_cw(self) -> Self {
        Self {
            x: 1000u16.saturating_sub(self.y).saturating_sub(self.h),
            y: self.x,
            w: self.h,
            h: self.w,
        }
        .clamped()
    }

    /// The same part of the frame after a left-to-right mirror.
    pub fn mirrored(self) -> Self {
        Self {
            x: 1000 - self.x - self.w,
            ..self
        }
        .clamped()
    }

    /// Pixels `(x, y, w, h)` for a frame of `width` x `height`, with even
    /// sides as the encoder wants.
    pub fn pixels(self, width: u32, height: u32) -> (u32, u32, u32, u32) {
        let part = |total: u32, start: u16, len: u16| {
            let total = total.max(2);
            let len = ((u64::from(total) * u64::from(len) / 1000) as u32 & !1).clamp(2, total & !1);
            let start = ((u64::from(total) * u64::from(start) / 1000) as u32 & !1).min(total - len);
            (start, len)
        };
        let (x, w) = part(width, self.x, self.w);
        let (y, h) = part(height, self.y, self.h);
        (x, y, w, h)
    }
}

/// The size of a frame after `turns` clockwise quarter turns.
pub fn turned_size(width: u32, height: u32, turns: u8) -> (u32, u32) {
    if turns % 2 == 1 {
        (height, width)
    } else {
        (width, height)
    }
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
            turns: 0,
            flipped: false,
            crop: None,
        }
    }

    /// Turn, flip or crop was chosen.
    pub fn has_geometry(&self) -> bool {
        !self.turns.is_multiple_of(4) || self.flipped || self.crop.is_some()
    }

    /// The size the clip has after turning and cropping.
    pub fn edited_size(&self, source: &VideoSource) -> (u32, u32) {
        let (width, height) = turned_size(source.width, source.height, self.turns);
        match self.crop {
            Some(crop) => {
                let (_, _, w, h) = crop.pixels(width, height);
                (w, h)
            }
            None => (width, height),
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
        self.is_trimmed(duration_ms) || self.quality != 0 || self.gif || self.has_geometry()
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

/// A time for the editor's labels (`FormatDurationText` plus tenths):
/// `00:07.4`, or `1:02:03.0` past an hour.
pub fn format_clock(ms: i64) -> String {
    let tenths = ms.max(0) / 100;
    let (secs, tenth) = (tenths / 10, tenths % 10);
    let (hours, minutes, seconds) = (secs / 3600, secs / 60 % 60, secs % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{tenth}")
    } else {
        format!("{minutes:02}:{seconds:02}.{tenth}")
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VideoSource {
    pub duration_ms: i64,
    pub width: u32,
    pub height: u32,
    pub has_audio: bool,
    /// Bytes of the file; 0 when unknown.
    pub file_size: u64,
    /// Bits a second of the picture; 0 when unknown.
    pub video_bitrate: u64,
    /// Frames a thousand seconds (29.97 fps is 29_970), kept whole so a
    /// probe compares exactly; 0 when unknown.
    pub fps_milli: u32,
}

/// A number ffprobe wrote as JSON number or text.
fn probe_number(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .filter(|number| number.is_finite() && *number > 0.0)
}

/// `30000/1001` as frames a second.
fn probe_rate(value: Option<&Value>) -> u32 {
    let Some(text) = value.and_then(Value::as_str) else {
        return 0;
    };
    let mut parts = text.split('/');
    let top: f64 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .unwrap_or(0.0);
    let bottom: f64 = parts
        .next()
        .and_then(|part| part.parse().ok())
        .unwrap_or(1.0);
    if bottom > 0.0 && top.is_finite() && top > 0.0 {
        // At most a million fps, so the cast never saturates in practice.
        (top / bottom * 1000.0).round().clamp(0.0, 1e9) as u32
    } else {
        0
    }
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
    let format = value.get("format");
    let total_bitrate = probe_number(format.and_then(|f| f.get("bit_rate"))).unwrap_or(0.0);
    let video_bitrate = probe_number(video.get("bit_rate")).unwrap_or(if has_audio {
        (total_bitrate - 128_000.0).max(0.0)
    } else {
        total_bitrate
    });
    (duration_ms > 0 && width > 0 && height > 0).then_some(VideoSource {
        duration_ms,
        width,
        height,
        has_audio,
        file_size: probe_number(format.and_then(|f| f.get("size"))).unwrap_or(0.0) as u64,
        video_bitrate: video_bitrate as u64,
        fps_milli: probe_rate(video.get("avg_frame_rate")),
    })
}

/// Run ffprobe on a local file. Blocks; call it off the UI thread.
pub fn probe_source(path: &Path) -> Option<VideoSource> {
    let output = crate::media_tools::command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,width,height,duration,bit_rate,avg_frame_rate:stream_tags=rotate:stream_side_data=rotation:format=duration,size,bit_rate",
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

/// The ffmpeg filters for turns and flip (tdesktop turns clockwise, then
/// mirrors what is on the screen).
pub fn turn_filters(turns: u8, flipped: bool) -> Vec<String> {
    let mut filters = Vec::new();
    match turns % 4 {
        1 => filters.push("transpose=1".into()),
        2 => filters.push("hflip,vflip".into()),
        3 => filters.push("transpose=2".into()),
        _ => {}
    }
    if flipped {
        filters.push("hflip".to_string());
    }
    filters
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
    let mut filters = turn_filters(edits.turns, edits.flipped);
    let (width, height) = edits.edited_size(source);
    if let Some(crop) = edits.crop {
        let (turned_w, turned_h) = turned_size(source.width, source.height, edits.turns);
        let (x, y, w, h) = crop.pixels(turned_w, turned_h);
        filters.push(format!("crop={w}:{h}:{x}:{y}"));
    }
    if edits.quality != 0 && edits.quality < width.min(height) {
        filters.push(if width <= height {
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
/// flipped and turned like the edit, at most `PREVIEW_SIDE` px on the
/// longer side. The crop is drawn over the picture, not applied.
pub fn frame_args(input: &str, at_ms: i64, output: &str, turns: u8, flipped: bool) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(seconds(at_ms.max(0)));
    args.extend(strings(&["-i", input, "-frames:v", "1", "-vf"]));
    let mut filters = turn_filters(turns, flipped);
    filters.push(format!(
        "scale='min({0},iw)':'min({0},ih)':force_original_aspect_ratio=decrease",
        PREVIEW_SIDE
    ));
    args.push(filters.join(","));
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
    let output = crate::media_tools::quiet_command("ffmpeg")
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

/// Save the frame at `at_ms` of `source_path`, `height` px high, as a JPEG
/// for the editor's frame strip (as recorded: the strip is not turned).
/// Blocks; call it off the UI thread.
pub fn render_strip_frame(source_path: &Path, at_ms: i64, height: u32) -> Result<PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|_| "couldn't read the video".to_string())?;
    let output = dir.join(format!("strip-{}.jpg", stamp()));
    let mut args = strings(&["-y", "-ss"]);
    args.push(seconds(at_ms.max(0)));
    args.extend(strings(&["-i"]));
    args.push(source_path.to_string_lossy().into_owned());
    args.extend(strings(&["-frames:v", "1", "-vf"]));
    args.push(format!("scale=-2:{}", height.max(2) & !1));
    args.extend(strings(&["-q:v", "4"]));
    args.push(output.to_string_lossy().into_owned());
    run_ffmpeg(&args)?;
    Ok(output)
}

/// Save the frame at `at_ms` of `source_path` as a JPEG, for the editor's
/// preview. Blocks; call it off the UI thread.
pub fn render_frame(
    source_path: &Path,
    at_ms: i64,
    turns: u8,
    flipped: bool,
) -> Result<PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|_| "couldn't read the video".to_string())?;
    let output = dir.join(format!("frame-{}.jpg", stamp()));
    run_ffmpeg(&frame_args(
        &source_path.to_string_lossy(),
        at_ms,
        &output.to_string_lossy(),
        turns,
        flipped,
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
            ..VideoSource::default()
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
            ..VideoEdits::full(0)
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
        assert_eq!(format_clock(0), "00:00.0");
        assert_eq!(format_clock(7_450), "00:07.4");
        assert_eq!(format_clock(62_000), "01:02.0");
        assert_eq!(format_clock(3_723_900), "1:02:03.9");
        assert_eq!(format_clock(-5), "00:00.0");
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
            ..VideoEdits::full(0)
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
        let args = frame_args("in.mp4", 2_345, "f.jpg", 0, false);
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
            turns: 1,
            ..VideoEdits::full(0)
        }
        .clamped(source.duration_ms, true);
        let output = render_edited(&clip, &edits, &source).expect("the edit renders");
        let edited = probe_source(&output).expect("the edited clip reads");
        assert!((900..=1200).contains(&edited.duration_ms), "{edited:?}");
        assert_eq!((edited.width, edited.height), (360, 480));
        assert!(!edited.has_audio, "a GIF has no sound");
        let frame = render_frame(&output, 300, 1, true).expect("a frame");
        assert!(frame.is_file());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(output);
        let _ = std::fs::remove_file(frame);
    }

    #[test]
    fn crop_stays_inside_the_frame_with_even_pixels() {
        let crop = Crop {
            x: 950,
            y: 0,
            w: 500,
            h: 20,
        }
        .clamped();
        assert_eq!((crop.x, crop.w, crop.h), (500, 500, MIN_CROP));
        assert_eq!(Crop::FULL.pixels(641, 481), (0, 0, 640, 480));
        let (x, y, w, h) = Crop {
            x: 250,
            y: 500,
            w: 500,
            h: 500,
        }
        .pixels(1000, 800);
        assert_eq!((x, y, w, h), (250, 400, 500, 400));
        assert_eq!(turned_size(640, 480, 1), (480, 640));
        assert_eq!(turned_size(640, 480, 2), (640, 480));
    }

    #[test]
    fn flip_turn_and_crop_come_before_the_scale() {
        let edits = VideoEdits {
            turns: 1,
            flipped: true,
            crop: Some(Crop {
                x: 0,
                y: 0,
                w: 500,
                h: 1000,
            }),
            quality: 360,
            ..VideoEdits::full(20_000)
        };
        // 1920x1080 turned is 1080x1920; the crop keeps 540x1920; the
        // shorter side is 540, so 360 scales it.
        let args = encode_args("in.mp4", "out.mp4", &edits, &source());
        let at = args.iter().position(|arg| arg == "-vf").unwrap();
        assert_eq!(
            args[at + 1],
            "transpose=1,hflip,crop=540:1920:0:0,scale=360:-2,format=yuv420p"
        );
        assert_eq!(edits.edited_size(&source()), (540, 1920));
        assert!(edits.needs_transcode(20_000));
        assert!(edits.has_geometry());
        let half = VideoEdits {
            turns: 2,
            ..VideoEdits::full(20_000)
        };
        let args = encode_args("in.mp4", "out.mp4", &half, &source());
        assert!(args.contains(&"hflip,vflip,format=yuv420p".to_string()));
        let left = VideoEdits {
            turns: 3,
            ..VideoEdits::full(20_000)
        };
        let args = encode_args("in.mp4", "out.mp4", &left, &source());
        assert!(args.contains(&"transpose=2,format=yuv420p".to_string()));
        assert!(!VideoEdits::full(20_000).has_geometry());
    }

    #[test]
    fn a_crop_follows_turns_and_mirrors() {
        let crop = Crop {
            x: 100,
            y: 200,
            w: 300,
            h: 400,
        };
        assert_eq!(
            crop.turned_cw(),
            Crop {
                x: 400,
                y: 100,
                w: 400,
                h: 300
            }
        );
        assert_eq!(crop.turned_cw().turned_cw().turned_cw().turned_cw(), crop);
        assert_eq!(crop.mirrored(), Crop { x: 600, ..crop });
    }

    #[test]
    fn the_preview_frame_is_turned_but_not_cropped() {
        let args = frame_args("in.mp4", 0, "f.jpg", 1, true);
        let at = args.iter().position(|arg| arg == "-vf").unwrap();
        assert!(args[at + 1].starts_with("transpose=1,hflip,scale="));
        assert!(!args[at + 1].contains("crop"));
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
        assert_eq!(parsed.fps_milli, 0);
        let rich = json!({
            "streams": [{"codec_type": "video", "width": 64, "height": 48,
                "bit_rate": "900000", "avg_frame_rate": "30000/1001"}],
            "format": {"duration": "3.0", "size": "400000", "bit_rate": "1000000"}
        });
        let parsed = parse_source(&rich).unwrap();
        assert_eq!((parsed.file_size, parsed.video_bitrate), (400_000, 900_000));
        assert_eq!(parsed.fps_milli, 29_970);
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
