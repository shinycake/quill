//! The video avatar: its limits, the editor's starting point and the
//! ffmpeg command line (Telegram Desktop `PrepareProfileVideo` with its
//! video editor: a square crop, 1 to 9.6 s, 800 x 800 at 30 fps, no
//! sound). The editor itself is the send-media one (`ui::video_editor`).

use crate::video_edit::{VideoEdits, VideoSource, turn_filters, turned_size};
use serde_json::Value;

/// `kProfileVideoSide`: the avatar clip is 800 x 800.
pub const AVATAR_VIDEO_SIDE: u32 = 800;
/// `kProfileVideoFps`.
pub const AVATAR_VIDEO_FPS: u32 = 30;
/// `kProfileVideoMinDuration`: 1 s.
pub const AVATAR_MIN_MS: i64 = 1000;
/// `kProfileVideoMaxDuration`: 9.6 s (TDLib allows 10).
pub const AVATAR_MAX_MS: i64 = 9600;

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

/// Check a probed clip: at least a second long, and no more than ten
/// times wider than tall or the other way round (`AcceptableDimensions`).
pub fn check_source(source: &VideoSource) -> Result<(), VideoAvatarError> {
    let (width, height) = (source.width, source.height);
    if width == 0
        || height == 0
        || width > height.saturating_mul(10)
        || height > width.saturating_mul(10)
    {
        return Err(VideoAvatarError::BadShape);
    }
    if source.duration_ms < AVATAR_MIN_MS {
        return Err(VideoAvatarError::TooShort);
    }
    Ok(())
}

/// The editor's starting point: the first 9.6 s at most and the centered
/// square.
pub fn initial_edits(source: &VideoSource) -> VideoEdits {
    VideoEdits {
        till_ms: source.duration_ms.clamp(0, AVATAR_MAX_MS),
        crop: crate::sticker_video::square_crop(source.width, source.height),
        ..VideoEdits::full(source.duration_ms)
    }
}

/// The main frame (`video_start_ts`): the cover's place in the encoded
/// clip, in seconds to a millisecond.
pub fn main_frame_secs(edits: &VideoEdits) -> f64 {
    let into = (edits.cover_ms - edits.from_ms).clamp(0, edits.length_ms().max(0));
    into as f64 / 1000.0
}

/// The encoded clip's size for the editor's label: the 1200 kbit/s
/// target over the kept range plus the MP4 container (no sound).
pub fn estimated_size(edits: &VideoEdits) -> u64 {
    let seconds = edits.length_ms().max(0) as f64 / 1000.0;
    let container = 2_000.0 + 12.0 * seconds * f64::from(AVATAR_VIDEO_FPS);
    (1_200_000.0 * seconds / 8.0 + container).round().max(1.0) as u64
}

fn seconds(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

/// ffmpeg arguments that write the edited clip `output`: the kept range,
/// turned and mirrored, the square cut and scaled to 800 x 800 at 30 fps,
/// no sound, H.264 at a bitrate that keeps 9.6 s under TDLib's 2 MB.
pub fn encode_args(
    input: &str,
    output: &str,
    edits: &VideoEdits,
    source: &VideoSource,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".into(), "-ss".into(), seconds(edits.from_ms)];
    args.extend(["-t".into(), seconds(edits.length_ms().max(1))]);
    args.extend(["-i".into(), input.to_string(), "-an".into()]);
    let mut filters = turn_filters(edits.turns, edits.flipped);
    if let Some(crop) = edits.crop {
        let (width, height) = turned_size(source.width, source.height, edits.turns);
        let (x, y, w, h) = crop.pixels(width, height);
        filters.push(format!("crop={w}:{h}:{x}:{y}"));
    }
    let side = AVATAR_VIDEO_SIDE;
    filters.push(format!("scale={side}:{side}:flags=lanczos"));
    filters.push(format!("fps={AVATAR_VIDEO_FPS}"));
    filters.push("format=yuv420p".into());
    args.extend(["-vf".into(), filters.join(",")]);
    for arg in [
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
    ] {
        args.push(arg.to_string());
    }
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

#[cfg(test)]
mod tests {
    use super::{
        AVATAR_MAX_MS, VideoAvatarError, check_source, encode_args, estimated_size, initial_edits,
        main_frame_secs, parse_probe,
    };
    use crate::video_edit::{Crop, VideoEdits, VideoSource};
    use serde_json::json;

    fn source(duration_ms: i64, width: u32, height: u32) -> VideoSource {
        VideoSource {
            duration_ms,
            width,
            height,
            ..VideoSource::default()
        }
    }

    #[test]
    fn limits_match_desktop() {
        assert_eq!(
            check_source(&source(500, 640, 480)),
            Err(VideoAvatarError::TooShort)
        );
        assert_eq!(
            check_source(&source(5000, 2000, 100)),
            Err(VideoAvatarError::BadShape)
        );
        assert_eq!(
            check_source(&source(5000, 0, 100)),
            Err(VideoAvatarError::BadShape)
        );
        assert!(check_source(&source(2000, 1000, 100)).is_ok());
    }

    #[test]
    fn the_editor_starts_on_a_centered_square_of_at_most_nine_point_six_seconds() {
        let long = initial_edits(&source(60_000, 1280, 720));
        assert_eq!((long.from_ms, long.till_ms), (0, AVATAR_MAX_MS));
        assert_eq!(
            long.crop,
            Some(Crop {
                x: 219,
                y: 0,
                w: 562,
                h: 1000
            })
        );
        let short = initial_edits(&source(4000, 500, 500));
        assert_eq!((short.till_ms, short.crop), (4000, None));
    }

    #[test]
    fn the_main_frame_counts_from_the_kept_start() {
        let edits = VideoEdits {
            from_ms: 2000,
            till_ms: 8000,
            cover_ms: 3250,
            ..VideoEdits::full(10_000)
        };
        assert_eq!(main_frame_secs(&edits), 1.25);
        let before = VideoEdits {
            cover_ms: 0,
            ..edits
        };
        assert_eq!(main_frame_secs(&before), 0.0);
        // Six seconds at 1200 kbit/s, plus the container.
        assert_eq!(estimated_size(&edits), 900_000 + 2_000 + 2_160);
    }

    #[test]
    fn the_encode_turns_crops_and_stays_silent() {
        let edits = VideoEdits {
            from_ms: 1500,
            till_ms: 4000,
            turns: 1,
            flipped: true,
            crop: Some(Crop {
                x: 0,
                y: 0,
                w: 1000,
                h: 500,
            }),
            ..VideoEdits::full(10_000)
        };
        let args = encode_args("in.mov", "out.mp4", &edits, &source(10_000, 1280, 720));
        let joined = args.join(" ");
        assert!(joined.starts_with("-y -ss 1.500 -t 2.500 -i in.mov -an -vf "));
        assert!(joined.contains(
            "transpose=1,hflip,crop=720:640:0:0,scale=800:800:flags=lanczos,fps=30,format=yuv420p"
        ));
        assert!(joined.ends_with("+faststart out.mp4"));
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
