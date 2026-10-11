//! The video avatar: limits, the trim range, the square crop and the ffmpeg
//! command lines (Telegram Desktop `PrepareProfileVideo` with its video
//! editor: an 800 x 800 crop, 1 to 9.6 s, 30 fps, no sound).

use serde_json::Value;

/// `kProfileVideoSide`: the avatar clip is 800 x 800.
pub const AVATAR_VIDEO_SIDE: u32 = 800;
/// `kProfileVideoFps`.
pub const AVATAR_VIDEO_FPS: u32 = 30;
/// `kProfileVideoMinDuration` (1 s) in seconds.
pub const AVATAR_MIN_SECS: f64 = 1.0;
/// `kProfileVideoMaxDuration` (9.6 s) in seconds; TDLib allows 10.
pub const AVATAR_MAX_SECS: f64 = 9.6;
/// Side of the scrubber's preview frames, in pixels.
pub const PREVIEW_SIDE: u32 = 240;
/// The crop can zoom into a quarter of the shorter side.
pub const MAX_ZOOM: f32 = 4.0;

/// Why a clip can't be a video avatar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VideoAvatarError {
    /// Shorter than `AVATAR_MIN_SECS` or without a known length.
    TooShort,
    /// More than ten times wider than tall, or the other way round
    /// (Telegram Desktop `AcceptableDimensions`).
    BadShape,
}

impl VideoAvatarError {
    pub fn message(self) -> &'static str {
        match self {
            Self::TooShort => "The video must be at least 1 second long.",
            Self::BadShape => "This video's shape can't be cropped to a square.",
        }
    }
}

/// The part of the clip that is kept, in seconds from the file's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trim {
    pub start: f64,
    pub end: f64,
}

impl Trim {
    pub fn length(&self) -> f64 {
        self.end - self.start
    }

    /// The main frame for a scrubber at `fraction` (0 to 1) of the kept
    /// part: seconds from the start of the encoded clip, to a millisecond.
    pub fn main_frame(&self, fraction: f32) -> f64 {
        let fraction = if fraction.is_finite() {
            f64::from(fraction.clamp(0.0, 1.0))
        } else {
            0.0
        };
        millis(fraction * self.length())
    }

    /// Where the main frame sits in the source file.
    pub fn source_time(&self, main_frame: f64) -> f64 {
        millis(self.start + main_frame.clamp(0.0, self.length()))
    }
}

fn millis(seconds: f64) -> f64 {
    (seconds * 1000.0).round() / 1000.0
}

/// A probed clip: the source and what may be cut from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoAvatarPlan {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
}

impl VideoAvatarPlan {
    /// Check a probed clip (`duration` in seconds, size in pixels).
    pub fn new(duration: f64, width: u32, height: u32) -> Result<Self, VideoAvatarError> {
        if width == 0
            || height == 0
            || width > height.saturating_mul(10)
            || height > width.saturating_mul(10)
        {
            return Err(VideoAvatarError::BadShape);
        }
        if !duration.is_finite() || duration < AVATAR_MIN_SECS {
            return Err(VideoAvatarError::TooShort);
        }
        Ok(Self {
            duration,
            width,
            height,
        })
    }

    /// The start of the clip, at most 9.6 s.
    pub fn default_trim(&self) -> Trim {
        Trim {
            start: 0.0,
            end: millis(self.duration.min(AVATAR_MAX_SECS)),
        }
    }

    /// Fit a dragged range into the limits (1 to 9.6 s inside the clip).
    /// The handle that was not moved stays put unless the range can't
    /// otherwise be valid.
    pub fn clamp_trim(&self, start: f64, end: f64, moved_start: bool) -> Trim {
        let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
        let mut start = finite(start, 0.0).clamp(0.0, self.duration);
        let mut end = finite(end, self.duration).clamp(0.0, self.duration);
        if moved_start {
            start = start.min(self.duration - AVATAR_MIN_SECS);
            end = end
                .min(start + AVATAR_MAX_SECS)
                .max(start + AVATAR_MIN_SECS);
        } else {
            end = end.max(AVATAR_MIN_SECS);
            start = start.max(end - AVATAR_MAX_SECS).min(end - AVATAR_MIN_SECS);
        }
        Trim {
            start: millis(start.max(0.0)),
            end: millis(end.min(self.duration)),
        }
    }
}

/// The square cut out of a frame, in source pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub side: u32,
}

/// Zoom and position of the square (Telegram Desktop's crop, set with
/// sliders): `zoom` 1 is the largest square, `x` and `y` run 0 to 1 over
/// the room the square has to move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SquareCrop {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

impl Default for SquareCrop {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            x: 0.5,
            y: 0.5,
        }
    }
}

impl SquareCrop {
    /// The square for a frame of `width` x `height`.
    pub fn rect(&self, width: u32, height: u32) -> CropRect {
        let unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.5
            }
        };
        let zoom = if self.zoom.is_finite() {
            self.zoom.clamp(1.0, MAX_ZOOM)
        } else {
            1.0
        };
        let short = width.min(height).max(1);
        let side = ((short as f32 / zoom).round() as u32).clamp(1, short);
        CropRect {
            x: (unit(self.x) * (width.saturating_sub(side)) as f32).round() as u32,
            y: (unit(self.y) * (height.saturating_sub(side)) as f32).round() as u32,
            side,
        }
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

fn crop_filter(rect: CropRect, out: u32) -> String {
    format!(
        "crop={0}:{0}:{1}:{2},scale={out}:{out}",
        rect.side, rect.x, rect.y
    )
}

/// ffmpeg arguments that cut `trim` out of `input`, crop `rect` and write
/// the avatar clip `output`: 800 x 800 at 30 fps, no sound, H.264 and a
/// bitrate that keeps 9.6 s under TDLib's 2 MB.
pub fn encode_args(input: &str, output: &str, trim: Trim, rect: CropRect) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(format!("{:.3}", trim.start));
    args.push("-t".into());
    args.push(format!("{:.3}", trim.length()));
    args.extend(strings(&["-i", input, "-an", "-vf"]));
    args.push(format!(
        "{},fps={AVATAR_VIDEO_FPS},format=yuv420p",
        crop_filter(rect, AVATAR_VIDEO_SIDE)
    ));
    args.extend(strings(&[
        "-c:v",
        "libx264",
        "-b:v",
        "1200k",
        "-maxrate",
        "1500k",
        "-bufsize",
        "3000k",
        "-movflags",
        "+faststart",
        output,
    ]));
    args
}

/// ffmpeg arguments that save the cropped preview of `input` at `seconds`
/// (source time) as the image `output`.
pub fn preview_frame_args(input: &str, seconds: f64, rect: CropRect, output: &str) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(format!("{seconds:.3}"));
    args.extend(strings(&["-i", input, "-frames:v", "1", "-vf"]));
    args.push(crop_filter(rect, PREVIEW_SIDE));
    args.extend(strings(&["-q:v", "3", output]));
    args
}

/// Read `ffprobe -of json` output for the first video stream. The stream
/// duration wins, then the container's (WebM and Matroska often omit the
/// first). A phone clip turned by 90 or 270 degrees swaps its size, as
/// ffmpeg turns it before cropping.
pub fn parse_probe(value: &Value) -> Option<(f64, u32, u32)> {
    let stream = value.get("streams")?.get(0)?;
    let seconds = |v: Option<&Value>| -> Option<f64> {
        let v = v?;
        v.as_f64()
            .or_else(|| v.as_str().and_then(|text| text.parse().ok()))
            .filter(|d| d.is_finite() && *d >= 0.0)
    };
    let duration = seconds(stream.get("duration"))
        .or_else(|| seconds(value.get("format").and_then(|f| f.get("duration"))))?;
    let width = u32::try_from(stream.get("width")?.as_u64()?).ok()?;
    let height = u32::try_from(stream.get("height")?.as_u64()?).ok()?;
    let turn = stream
        .get("side_data_list")
        .and_then(|list| list.as_array())
        .and_then(|list| list.iter().find_map(|d| d.get("rotation")?.as_f64()))
        .or_else(|| seconds(stream.get("tags").and_then(|t| t.get("rotate"))))
        .unwrap_or(0.0);
    let quarter = (turn.abs().round() as i64) % 180 == 90;
    Some(if quarter {
        (duration, height, width)
    } else {
        (duration, width, height)
    })
}

/// Run ffprobe on a local file. Blocks; call it off the UI thread.
pub fn probe_clip(path: &std::path::Path) -> Option<(f64, u32, u32)> {
    let output = crate::media_tools::command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,duration:stream_tags=rotate:stream_side_data=rotation:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_probe(&serde_json::from_slice(&output.stdout).ok()?)
}

#[cfg(test)]
mod tests {
    use super::{
        AVATAR_MAX_SECS, CropRect, SquareCrop, Trim, VideoAvatarError, VideoAvatarPlan,
        encode_args, parse_probe, preview_frame_args,
    };
    use serde_json::json;

    fn plan(duration: f64) -> VideoAvatarPlan {
        VideoAvatarPlan::new(duration, 640, 480).unwrap()
    }

    #[test]
    fn plan_limits_match_desktop() {
        assert_eq!(
            VideoAvatarPlan::new(0.5, 640, 480),
            Err(VideoAvatarError::TooShort)
        );
        assert_eq!(
            VideoAvatarPlan::new(f64::NAN, 640, 480),
            Err(VideoAvatarError::TooShort)
        );
        assert_eq!(
            VideoAvatarPlan::new(5.0, 2000, 100),
            Err(VideoAvatarError::BadShape)
        );
        assert_eq!(
            VideoAvatarPlan::new(5.0, 0, 100),
            Err(VideoAvatarError::BadShape)
        );
        assert_eq!(
            plan(5.0).default_trim(),
            Trim {
                start: 0.0,
                end: 5.0
            }
        );
        assert_eq!(plan(60.0).default_trim().end, AVATAR_MAX_SECS);
        assert!(VideoAvatarPlan::new(2.0, 1000, 100).is_ok());
    }

    #[test]
    fn dragging_the_end_handle_keeps_one_to_nine_point_six_seconds() {
        let p = plan(30.0);
        // Too long: the start follows.
        assert_eq!(
            p.clamp_trim(2.0, 20.0, false),
            Trim {
                start: 10.4,
                end: 20.0
            }
        );
        // Too short: the start gives way.
        assert_eq!(
            p.clamp_trim(5.0, 5.2, false),
            Trim {
                start: 4.2,
                end: 5.2
            }
        );
        // Past the clip.
        assert_eq!(p.clamp_trim(25.0, 99.0, false).end, 30.0);
        // Near the beginning the end can't go under one second.
        assert_eq!(
            p.clamp_trim(0.0, 0.3, false),
            Trim {
                start: 0.0,
                end: 1.0
            }
        );
    }

    #[test]
    fn dragging_the_start_handle_pushes_the_end() {
        let p = plan(30.0);
        assert_eq!(
            p.clamp_trim(10.0, 12.0, true),
            Trim {
                start: 10.0,
                end: 12.0
            }
        );
        assert_eq!(
            p.clamp_trim(10.0, 11.0, true),
            Trim {
                start: 10.0,
                end: 11.0
            }
        );
        assert_eq!(
            p.clamp_trim(10.0, 10.2, true),
            Trim {
                start: 10.0,
                end: 11.0
            }
        );
        // Too long: the end follows.
        assert_eq!(p.clamp_trim(0.0, 25.0, true).end, 9.6);
        // Start at the very end leaves one second.
        assert_eq!(
            p.clamp_trim(30.0, 30.0, true),
            Trim {
                start: 29.0,
                end: 30.0
            }
        );
        // Garbage in, a valid range out.
        let t = p.clamp_trim(f64::NAN, f64::NAN, true);
        assert!(t.length() >= 1.0 && t.length() <= 9.6);
    }

    #[test]
    fn main_frame_follows_the_scrubber_inside_the_trim() {
        let t = Trim {
            start: 3.0,
            end: 7.0,
        };
        assert_eq!(t.main_frame(0.0), 0.0);
        assert_eq!(t.main_frame(0.5), 2.0);
        assert_eq!(t.main_frame(1.0), 4.0);
        assert_eq!(t.main_frame(7.0), 4.0);
        assert_eq!(t.main_frame(-1.0), 0.0);
        assert_eq!(t.main_frame(f32::NAN), 0.0);
        assert_eq!(t.source_time(2.0), 5.0);
        assert_eq!(t.source_time(99.0), 7.0);
    }

    #[test]
    fn crop_rect_zooms_and_moves_inside_the_frame() {
        let full = SquareCrop::default().rect(640, 480);
        assert_eq!(
            full,
            CropRect {
                x: 80,
                y: 0,
                side: 480
            }
        );
        let left = SquareCrop {
            zoom: 1.0,
            x: 0.0,
            y: 0.0,
        }
        .rect(640, 480);
        assert_eq!((left.x, left.y), (0, 0));
        let right = SquareCrop {
            zoom: 1.0,
            x: 1.0,
            y: 1.0,
        }
        .rect(640, 480);
        assert_eq!((right.x, right.y, right.side), (160, 0, 480));
        let zoomed = SquareCrop {
            zoom: 2.0,
            x: 1.0,
            y: 1.0,
        }
        .rect(640, 480);
        assert_eq!(
            zoomed,
            CropRect {
                x: 400,
                y: 240,
                side: 240
            }
        );
        // Limits and garbage.
        assert_eq!(
            SquareCrop {
                zoom: 99.0,
                x: 0.5,
                y: 0.5
            }
            .rect(400, 400)
            .side,
            100
        );
        assert_eq!(
            SquareCrop {
                zoom: 0.1,
                x: 0.5,
                y: 0.5
            }
            .rect(400, 400)
            .side,
            400
        );
        let bad = SquareCrop {
            zoom: f32::NAN,
            x: f32::NAN,
            y: 7.0,
        }
        .rect(300, 200);
        assert_eq!((bad.side, bad.x, bad.y), (200, 50, 0));
    }

    #[test]
    fn encode_command_cuts_crops_and_stays_silent() {
        let trim = Trim {
            start: 2.5,
            end: 12.1,
        };
        let rect = CropRect {
            x: 80,
            y: 0,
            side: 480,
        };
        let args = encode_args("in.mov", "out.mp4", trim, rect);
        assert_eq!(args.last().unwrap(), "out.mp4");
        let at = |flag: &str| {
            let i = args.iter().position(|a| a == flag).unwrap();
            args[i + 1].clone()
        };
        assert_eq!(at("-ss"), "2.500");
        assert_eq!(at("-t"), "9.600");
        assert_eq!(at("-i"), "in.mov");
        assert!(args.contains(&"-an".to_string()));
        assert!(
            at("-vf").starts_with("crop=480:480:80:0,scale=800:800,fps=30"),
            "{}",
            at("-vf")
        );
        // Seeking comes before the input.
        let ss = args.iter().position(|a| a == "-ss").unwrap();
        let i = args.iter().position(|a| a == "-i").unwrap();
        assert!(ss < i);
    }

    #[test]
    fn preview_command_seeks_and_crops() {
        let rect = CropRect {
            x: 0,
            y: 10,
            side: 100,
        };
        let args = preview_frame_args("in.mov", 1.25, rect, "frame.jpg");
        assert_eq!(&args[..3], ["-y", "-ss", "1.250"]);
        assert_eq!(args[3], "-i");
        assert!(args.iter().any(|a| a == "crop=100:100:0:10,scale=240:240"));
        assert_eq!(args.last().unwrap(), "frame.jpg");
    }

    #[test]
    fn probe_reads_duration_and_turns_rotated_clips() {
        let both = json!({
            "streams": [{ "width": 640, "height": 480, "duration": "3.5" }],
            "format": { "duration": "9.0" },
        });
        assert_eq!(parse_probe(&both), Some((3.5, 640, 480)));
        let webm = json!({
            "streams": [{ "width": 100, "height": 100, "duration": "N/A" }],
            "format": { "duration": "2.25" },
        });
        assert_eq!(parse_probe(&webm).unwrap().0, 2.25);
        let phone = json!({
            "streams": [{ "width": 1920, "height": 1080, "duration": "4",
                "side_data_list": [{ "rotation": -90 }] }],
        });
        assert_eq!(parse_probe(&phone), Some((4.0, 1080, 1920)));
        let tagged = json!({
            "streams": [{ "width": 1920, "height": 1080, "duration": "4",
                "tags": { "rotate": "270" } }],
        });
        assert_eq!(parse_probe(&tagged), Some((4.0, 1080, 1920)));
        let upside_down = json!({
            "streams": [{ "width": 1920, "height": 1080, "duration": "4",
                "side_data_list": [{ "rotation": 180 }] }],
        });
        assert_eq!(parse_probe(&upside_down), Some((4.0, 1920, 1080)));
        assert!(parse_probe(&json!({ "streams": [] })).is_none());
    }
}
