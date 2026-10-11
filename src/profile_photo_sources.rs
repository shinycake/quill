//! Profile photo sources beyond a still file (Telegram Desktop
//! `UserpicButton`): the emoji avatar builder, a video avatar with the
//! main frame chosen on a scrubber, and a camera snapshot.
//!
//! This module holds the parts with no UI: the `setProfilePhoto` request
//! bodies for `inputChatPhotoAnimation` and `inputChatPhotoSticker`, the
//! builder's gradient presets, the video limits and the ffmpeg command
//! lines for the avatar clip, the preview frames and the snapshot.

use crate::ids::RequestId;
use serde_json::{Value, json};

/// Telegram Desktop `kProfileVideoSide`: the avatar clip is 800 x 800.
pub const AVATAR_VIDEO_SIDE: u32 = 800;
/// `kProfileVideoFps`.
pub const AVATAR_VIDEO_FPS: u32 = 30;
/// `kProfileVideoMinDuration` (1 s) in seconds.
pub const AVATAR_MIN_SECS: f64 = 1.0;
/// `kProfileVideoMaxDuration` (9.6 s) in seconds; TDLib allows 10.
pub const AVATAR_MAX_SECS: f64 = 9.6;
/// Side of the scrubber's preview frames, in pixels.
pub const PREVIEW_SIDE: u32 = 240;
/// Frames the camera discards before the snapshot, so the exposure has
/// settled (the first frames of a webcam are dark).
const CAMERA_WARMUP_FRAMES: u32 = 10;

/// Telegram Desktop's emoji builder palette (`PaletteGradients`), as listed
/// in its source. Four colors each, `0xRRGGBB`.
const PALETTE: [[u32; 4]; 7] = [
    [0x20E2CD, 0x0EE1F1, 0x4D8DFF, 0x2BBFFF],
    [0x45F7B7, 0x1FF1D9, 0x5EB6FB, 0x1FCEEB],
    [0xC1E526, 0x80DF2B, 0x09D260, 0x5EDC40],
    [0xFFD412, 0xFFA743, 0xF5694E, 0xF5772C],
    [0xF6A730, 0xFF7742, 0xF64884, 0xEF5B41],
    [0xFFB23A, 0xFE7E62, 0xF94BA0, 0xFB5C80],
    [0xFF72A9, 0xE269FF, 0x837CFF, 0xB063FF],
];

/// How many gradient presets the builder offers.
pub const PRESET_COUNT: usize = PALETTE.len();

/// The colors of preset `index` (wrapping), in the order Telegram Desktop
/// uses them: its list rotated by 180 degrees (first and third color
/// swapped, second and fourth swapped).
pub fn gradient_preset(index: usize) -> [u32; 4] {
    let [a, b, c, d] = PALETTE[index % PALETTE.len()];
    [c, d, a, b]
}

/// `backgroundFill` for a chosen color list. One color is solid, two are a
/// vertical gradient, more are a freeform gradient. A chat photo sticker
/// ignores the rotation angle, so it is always 0.
pub fn background_fill(colors: &[u32]) -> Value {
    let rgb = |color: u32| i64::from(color & 0x00FF_FFFF);
    match colors {
        [] => json!({ "@type": "backgroundFillSolid", "color": 0 }),
        [one] => json!({ "@type": "backgroundFillSolid", "color": rgb(*one) }),
        [top, bottom] => json!({
            "@type": "backgroundFillGradient",
            "top_color": rgb(*top),
            "bottom_color": rgb(*bottom),
            "rotation_angle": 0,
        }),
        many => json!({
            "@type": "backgroundFillFreeformGradient",
            "colors": many.iter().map(|c| rgb(*c)).collect::<Vec<_>>(),
        }),
    }
}

/// What sits on the gradient (`ChatPhotoStickerType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhotoSticker {
    CustomEmoji {
        custom_emoji_id: i64,
    },
    RegularOrMask {
        sticker_set_id: i64,
        sticker_id: i64,
    },
}

/// `chatPhotoSticker type background_fill`.
pub fn chat_photo_sticker(sticker: PhotoSticker, colors: &[u32]) -> Value {
    let kind = match sticker {
        PhotoSticker::CustomEmoji { custom_emoji_id } => json!({
            "@type": "chatPhotoStickerTypeCustomEmoji",
            "custom_emoji_id": custom_emoji_id,
        }),
        PhotoSticker::RegularOrMask {
            sticker_set_id,
            sticker_id,
        } => json!({
            "@type": "chatPhotoStickerTypeRegularOrMask",
            "sticker_set_id": sticker_set_id,
            "sticker_id": sticker_id,
        }),
    };
    json!({
        "@type": "chatPhotoSticker",
        "type": kind,
        "background_fill": background_fill(colors),
    })
}

/// `setProfilePhoto` with `inputChatPhotoSticker` (TDLib 1.8.68,
/// `schema/td_api.tl:1051`): the emoji or sticker avatar.
pub fn set_profile_photo_sticker(
    extra: RequestId,
    sticker: PhotoSticker,
    colors: &[u32],
    is_public: bool,
) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoSticker",
            "sticker": chat_photo_sticker(sticker, colors),
        },
        "is_public": is_public,
    })
    .to_string()
}

/// `setProfilePhoto` with `inputChatPhotoAnimation` (schema line 1048):
/// the video avatar. `main_frame_timestamp` is in seconds from the start
/// of `path`, the clip as it was encoded.
pub fn set_profile_photo_animation(
    extra: RequestId,
    path: &str,
    main_frame_timestamp: f64,
    is_public: bool,
) -> String {
    json!({
        "@type": "setProfilePhoto",
        "@extra": extra.as_extra(),
        "photo": {
            "@type": "inputChatPhotoAnimation",
            "animation": { "@type": "inputFileLocal", "path": path },
            "main_frame_timestamp": main_frame_timestamp,
        },
        "is_public": is_public,
    })
    .to_string()
}

/// `getDefaultProfilePhotoCustomEmojiStickers = Stickers;` (schema line
/// 15151): the custom emoji the builder offers.
pub fn get_default_profile_photo_custom_emoji_stickers(extra: RequestId) -> String {
    json!({
        "@type": "getDefaultProfilePhotoCustomEmojiStickers",
        "@extra": extra.as_extra(),
    })
    .to_string()
}

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

/// The clip that will be uploaded: the start of the picked file, cut to
/// at most `AVATAR_MAX_SECS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VideoAvatarPlan {
    /// Length of the encoded clip, in seconds.
    pub length: f64,
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
            length: duration.min(AVATAR_MAX_SECS),
        })
    }

    /// The main frame for a scrubber at `fraction` (0 to 1) of the clip,
    /// in seconds from the start of the encoded video, to a millisecond.
    pub fn main_frame(&self, fraction: f32) -> f64 {
        let fraction = if fraction.is_finite() {
            f64::from(fraction.clamp(0.0, 1.0))
        } else {
            0.0
        };
        ((fraction * self.length) * 1000.0).round() / 1000.0
    }

    /// Clamp a timestamp to the clip.
    pub fn clamp_timestamp(&self, seconds: f64) -> f64 {
        if seconds.is_finite() {
            seconds.clamp(0.0, self.length)
        } else {
            0.0
        }
    }
}

/// The center square of the picture, scaled to `side`.
fn square_filter(side: u32) -> String {
    format!("crop=min(iw\\,ih):min(iw\\,ih),scale={side}:{side}")
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|arg| arg.to_string()).collect()
}

/// ffmpeg arguments that turn `input` into the avatar clip `output`:
/// center square, 800 x 800 at 30 fps, no sound, H.264 and a bitrate that
/// keeps 9.6 s under TDLib's 2 MB.
pub fn encode_args(input: &str, output: &str, plan: VideoAvatarPlan) -> Vec<String> {
    let mut args = strings(&["-y", "-i", input, "-t"]);
    args.push(format!("{:.3}", plan.length));
    args.push("-an".into());
    args.push("-vf".into());
    args.push(format!(
        "{},fps={AVATAR_VIDEO_FPS},format=yuv420p",
        square_filter(AVATAR_VIDEO_SIDE)
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

/// ffmpeg arguments that save the square preview of `input` at `seconds`
/// as the image `output`.
pub fn preview_frame_args(input: &str, seconds: f64, output: &str) -> Vec<String> {
    let mut args = strings(&["-y", "-ss"]);
    args.push(format!("{seconds:.3}"));
    args.extend(strings(&["-i", input, "-frames:v", "1", "-vf"]));
    args.push(square_filter(PREVIEW_SIDE));
    args.extend(strings(&["-q:v", "3", output]));
    args
}

/// The camera input of `media_tools::capture_input` without the
/// microphone. `args` is the full list (video and audio inputs).
pub fn video_only_input(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        kept.push(arg.clone());
        if arg == "-i" {
            if let Some(device) = iter.next() {
                kept.push(camera_device(device));
            }
            break;
        }
    }
    kept
}

/// The camera part of an input name: AVFoundation's `video:audio` pair
/// keeps the video and drops the sound, DirectShow's `video=..:audio=..`
/// the same.
fn camera_device(device: &str) -> String {
    if let Some((video, _)) = device.split_once(":audio=") {
        return video.to_string();
    }
    if let Some((video, _)) = device.split_once(':') {
        return format!("{video}:none");
    }
    device.to_string()
}

/// ffmpeg arguments for a camera snapshot (`input` from
/// `video_only_input`): skip the dark warm-up frames, mirror like a
/// selfie (Telegram Desktop mirrors the camera frame), crop the center
/// square at 800 x 800 and write one JPEG.
pub fn camera_snapshot_args(input: &[String], output: &str) -> Vec<String> {
    let mut args = strings(&["-y"]);
    args.extend(input.iter().cloned());
    args.push("-vf".into());
    args.push(format!(
        "select=gte(n\\,{CAMERA_WARMUP_FRAMES}),hflip,{}",
        square_filter(AVATAR_VIDEO_SIDE)
    ));
    args.extend(strings(&["-frames:v", "1", "-q:v", "2", output]));
    args
}

/// A probed clip: length in seconds and size in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
}

/// Read `ffprobe -of json` output for the first video stream. The stream
/// duration wins, then the container's (WebM and Matroska often omit the
/// first).
pub fn parse_probe(value: &Value) -> Option<ClipInfo> {
    let stream = value.get("streams")?.get(0)?;
    let seconds = |v: Option<&Value>| -> Option<f64> {
        let v = v?;
        v.as_f64()
            .or_else(|| v.as_str().and_then(|text| text.parse().ok()))
            .filter(|d| d.is_finite() && *d >= 0.0)
    };
    let duration = seconds(stream.get("duration"))
        .or_else(|| seconds(value.get("format").and_then(|f| f.get("duration"))))?;
    Some(ClipInfo {
        duration,
        width: u32::try_from(stream.get("width")?.as_u64()?).ok()?,
        height: u32::try_from(stream.get("height")?.as_u64()?).ok()?,
    })
}

/// Run ffprobe on a local file. Blocks; call it off the UI thread.
pub fn probe_clip(path: &std::path::Path) -> Option<ClipInfo> {
    let output = crate::media_tools::command("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,duration:format=duration",
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
        AVATAR_MAX_SECS, PhotoSticker, VideoAvatarError, VideoAvatarPlan, background_fill,
        camera_snapshot_args, encode_args, gradient_preset, preview_frame_args,
        set_profile_photo_animation, set_profile_photo_sticker, video_only_input,
    };
    use crate::ids::RequestId;
    use serde_json::{Value, json};

    fn parse(text: String) -> Value {
        serde_json::from_str(&text).unwrap()
    }

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn probe_reads_stream_then_container_duration() {
        let both = json!({
            "streams": [{ "width": 640, "height": 480, "duration": "3.5" }],
            "format": { "duration": "9.0" },
        });
        let info = super::parse_probe(&both).unwrap();
        assert_eq!((info.width, info.height, info.duration), (640, 480, 3.5));
        let webm = json!({
            "streams": [{ "width": 100, "height": 100, "duration": "N/A" }],
            "format": { "duration": "2.25" },
        });
        assert_eq!(super::parse_probe(&webm).unwrap().duration, 2.25);
        assert!(super::parse_probe(&json!({ "streams": [] })).is_none());
    }

    #[test]
    fn presets_are_the_desktop_palette_rotated_by_half() {
        assert_eq!(super::PRESET_COUNT, 7);
        assert_eq!(gradient_preset(0), [0x4D8DFF, 0x2BBFFF, 0x20E2CD, 0x0EE1F1]);
        assert_eq!(gradient_preset(6), [0x837CFF, 0xB063FF, 0xFF72A9, 0xE269FF]);
        assert_eq!(gradient_preset(7), gradient_preset(0));
    }

    #[test]
    fn background_fill_picks_the_fill_by_color_count() {
        assert_eq!(
            background_fill(&[0xFF0000]),
            json!({ "@type": "backgroundFillSolid", "color": 0xFF0000 })
        );
        assert_eq!(
            background_fill(&[0x112233, 0x445566]),
            json!({
                "@type": "backgroundFillGradient",
                "top_color": 0x112233,
                "bottom_color": 0x445566,
                "rotation_angle": 0,
            })
        );
        assert_eq!(
            background_fill(&gradient_preset(3)),
            json!({
                "@type": "backgroundFillFreeformGradient",
                "colors": [0xF5694E, 0xF5772C, 0xFFD412, 0xFFA743],
            })
        );
        // Alpha bits never leak into the RGB24 value.
        assert_eq!(background_fill(&[0xFF12_3456])["color"], 0x12_3456);
    }

    #[test]
    fn emoji_avatar_request_uses_a_custom_emoji_sticker() {
        let v = parse(set_profile_photo_sticker(
            RequestId(5),
            PhotoSticker::CustomEmoji {
                custom_emoji_id: 5_368_324_170_671_202_286,
            },
            &gradient_preset(0),
            false,
        ));
        assert_eq!(v["@type"], "setProfilePhoto");
        assert_eq!(v["@extra"], "5");
        assert_eq!(v["is_public"], false);
        assert_eq!(v["photo"]["@type"], "inputChatPhotoSticker");
        let sticker = &v["photo"]["sticker"];
        assert_eq!(sticker["@type"], "chatPhotoSticker");
        assert_eq!(sticker["type"]["@type"], "chatPhotoStickerTypeCustomEmoji");
        assert_eq!(
            sticker["type"]["custom_emoji_id"],
            5_368_324_170_671_202_286_i64
        );
        assert_eq!(
            sticker["background_fill"]["@type"],
            "backgroundFillFreeformGradient"
        );
    }

    #[test]
    fn sticker_avatar_request_names_set_and_sticker() {
        let v = parse(set_profile_photo_sticker(
            RequestId(6),
            PhotoSticker::RegularOrMask {
                sticker_set_id: 77,
                sticker_id: 88,
            },
            &[0x00FF00],
            true,
        ));
        assert_eq!(v["is_public"], true);
        let kind = &v["photo"]["sticker"]["type"];
        assert_eq!(kind["@type"], "chatPhotoStickerTypeRegularOrMask");
        assert_eq!(kind["sticker_set_id"], 77);
        assert_eq!(kind["sticker_id"], 88);
        assert_eq!(
            v["photo"]["sticker"]["background_fill"]["@type"],
            "backgroundFillSolid"
        );
    }

    #[test]
    fn video_avatar_request_carries_the_main_frame() {
        let v = parse(set_profile_photo_animation(
            RequestId(7),
            "/tmp/avatar.mp4",
            2.5,
            false,
        ));
        assert_eq!(v["photo"]["@type"], "inputChatPhotoAnimation");
        assert_eq!(v["photo"]["animation"]["@type"], "inputFileLocal");
        assert_eq!(v["photo"]["animation"]["path"], "/tmp/avatar.mp4");
        assert_eq!(v["photo"]["main_frame_timestamp"], 2.5);
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
        assert_eq!(VideoAvatarPlan::new(5.0, 640, 480).unwrap().length, 5.0);
        assert_eq!(
            VideoAvatarPlan::new(60.0, 640, 480).unwrap().length,
            AVATAR_MAX_SECS
        );
        assert!(VideoAvatarPlan::new(2.0, 1000, 100).is_ok());
    }

    #[test]
    fn main_frame_follows_the_scrubber_and_stays_inside_the_clip() {
        let plan = VideoAvatarPlan::new(4.0, 640, 480).unwrap();
        assert_eq!(plan.main_frame(0.0), 0.0);
        assert_eq!(plan.main_frame(0.5), 2.0);
        assert_eq!(plan.main_frame(1.0), 4.0);
        assert_eq!(plan.main_frame(7.0), 4.0);
        assert_eq!(plan.main_frame(-1.0), 0.0);
        assert_eq!(plan.main_frame(f32::NAN), 0.0);
        assert_eq!(plan.clamp_timestamp(9.0), 4.0);
        assert_eq!(plan.clamp_timestamp(-1.0), 0.0);
        assert_eq!(plan.clamp_timestamp(f64::INFINITY), 0.0);
    }

    #[test]
    fn encode_command_makes_a_silent_square_clip() {
        let plan = VideoAvatarPlan::new(20.0, 640, 480).unwrap();
        let args = encode_args("in.mov", "out.mp4", plan);
        assert_eq!(args.last().unwrap(), "out.mp4");
        let at = |flag: &str| {
            let i = args.iter().position(|a| a == flag).unwrap();
            args[i + 1].clone()
        };
        assert_eq!(at("-t"), "9.600");
        assert_eq!(at("-i"), "in.mov");
        assert!(args.contains(&"-an".to_string()));
        let filter = at("-vf");
        assert!(filter.contains("scale=800:800"));
        assert!(filter.contains("fps=30"));
        assert!(filter.starts_with("crop=min(iw\\,ih):min(iw\\,ih)"));
    }

    #[test]
    fn preview_command_seeks_before_opening() {
        let args = preview_frame_args("in.mov", 1.25, "frame.jpg");
        assert_eq!(&args[..3], ["-y", "-ss", "1.250"]);
        assert_eq!(args[3], "-i");
        assert!(args.iter().any(|a| a.contains("scale=240:240")));
        assert_eq!(args.last().unwrap(), "frame.jpg");
    }

    #[test]
    fn camera_input_drops_the_microphone() {
        let mac = list(&[
            "-f",
            "avfoundation",
            "-framerate",
            "30",
            "-pixel_format",
            "nv12",
            "-i",
            "default:default",
        ]);
        let kept = video_only_input(&mac);
        assert_eq!(kept.last().unwrap(), "default:none");
        assert_eq!(kept.len(), mac.len());

        let linux = list(&[
            "-f",
            "v4l2",
            "-framerate",
            "15",
            "-i",
            "/dev/video0",
            "-f",
            "pulse",
            "-i",
            "default",
        ]);
        assert_eq!(
            video_only_input(&linux),
            list(&["-f", "v4l2", "-framerate", "15", "-i", "/dev/video0"])
        );

        let windows = list(&["-f", "dshow", "-i", "video=Cam:audio=Mic"]);
        assert_eq!(
            video_only_input(&windows),
            list(&["-f", "dshow", "-i", "video=Cam"])
        );
    }

    #[test]
    fn snapshot_skips_warmup_mirrors_and_writes_one_frame() {
        let input = list(&["-f", "v4l2", "-i", "/dev/video0"]);
        let args = camera_snapshot_args(&input, "me.jpg");
        assert_eq!(&args[..5], ["-y", "-f", "v4l2", "-i", "/dev/video0"]);
        let i = args.iter().position(|a| a == "-vf").unwrap();
        assert!(args[i + 1].starts_with("select=gte(n\\,10),hflip,crop="));
        assert!(args[i + 1].ends_with("scale=800:800"));
        let n = args.iter().position(|a| a == "-frames:v").unwrap();
        assert_eq!(args[n + 1], "1");
        assert_eq!(args.last().unwrap(), "me.jpg");
    }
}
