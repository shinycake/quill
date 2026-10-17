//! Video stickers for the sticker creator (tdesktop
//! `boxes/sticker_creator_box.cpp`, `RunVideoEditorAndCreate`, and
//! `media/media_video_encode.cpp`, `RunWebmSticker`): a WEBM sticker or a
//! GIF is trimmed to at most three seconds, cropped to a square, and encoded
//! again as VP9 with alpha at the set's side, under the size the server
//! accepts. Quill drives the same external `ffmpeg` the video editor already
//! uses; the numbers and the command lines live here so they are unit
//! tested. The dialog is `ui/video_editor_set.rs`.
#![deny(clippy::indexing_slicing)]

use crate::sticker_set_box::SetKind;
use crate::video_edit::{Crop, VideoEdits, VideoSource, turned_size};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Longest video sticker (`kVideoStickerMaxDuration`).
pub const MAX_DURATION_MS: i64 = 3000;
/// Shortest video sticker (`kVideoStickerMinDuration`).
pub const MIN_DURATION_MS: i64 = 200;
/// Frame rate cap (`kVideoStickerFps`).
pub const FPS_LIMIT: u32 = 30;
/// Largest encoded sticker (`kMaxStickerWebmBytes`).
pub const MAX_STICKER_WEBM_BYTES: u64 = 256 * 1024;
/// Largest encoded emoji (`kMaxEmojiWebmBytes`).
pub const MAX_EMOJI_WEBM_BYTES: u64 = 64 * 1024;
/// VP9 quality steps tried until the file fits (`kWebmStickerCrf`, then
/// `kWebmStickerCrfLadder`).
pub const CRF_LADDER: [u32; 6] = [28, 34, 40, 47, 55, 63];

/// The toast when the source can't be read or the result won't fit
/// (`lng_bad_video`, with the app's own name).
pub const BAD_VIDEO_NOTE: &str = "Sorry, Quill can't process that type of video.";
/// The "New Sticker" box's wait note while the sticker is encoded.
pub const ENCODING_NOTE: &str = "Preparing the video…";
/// The caption over the saved GIFs in the existing-sticker chooser.
pub const GIFS_CAPTION: &str = "GIFs";

/// The largest file the server takes for a set of this kind
/// (`MaxWebmBytesForType`).
pub fn max_webm_bytes(kind: SetKind) -> u64 {
    match kind {
        SetKind::Stickers => MAX_STICKER_WEBM_BYTES,
        SetKind::Emoji => MAX_EMOJI_WEBM_BYTES,
    }
}

/// A probed source the sticker editor can open: it has frames and some
/// length (`FileInfo::valid`; a clip shorter than [`MIN_DURATION_MS`] is
/// taken whole, as tdesktop's timeline clamps the floor to the clip).
pub fn accepts_source(source: &VideoSource) -> bool {
    source.width > 0 && source.height > 0 && source.duration_ms > 0
}

/// The largest centered square of a `width`×`height` frame, in thousandths
/// (`Editor::OriginalCrop` for an `exactSize` square). `None` when the
/// frame is square already.
pub fn square_crop(width: u32, height: u32) -> Option<Crop> {
    if width == 0 || height == 0 || width == height {
        return None;
    }
    let side = width.min(height);
    let part = |total: u32| (u64::from(side) * 1000 / u64::from(total.max(1))) as u16;
    let (w, h) = (part(width), part(height));
    Some(
        Crop {
            x: (1000 - w) / 2,
            y: (1000 - h) / 2,
            w,
            h,
        }
        .clamped(),
    )
}

/// `crop` made square again in pixels after one of its sides changed: the
/// side the user set stays, the other follows, centered where the crop was.
pub fn squared_crop(crop: Crop, width: u32, height: u32, width_set: bool) -> Crop {
    if width == 0 || height == 0 {
        return crop.clamped();
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let (center_x, center_y) = (
        f64::from(crop.x) + f64::from(crop.w) / 2.0,
        f64::from(crop.y) + f64::from(crop.h) / 2.0,
    );
    let side_px = if width_set {
        f64::from(crop.w) / 1000.0 * w
    } else {
        f64::from(crop.h) / 1000.0 * h
    }
    .min(w.min(h))
    .max(1.0);
    let crop_w = (side_px / w * 1000.0).round().clamp(1.0, 1000.0);
    let crop_h = (side_px / h * 1000.0).round().clamp(1.0, 1000.0);
    Crop {
        x: (center_x - crop_w / 2.0).clamp(0.0, 1000.0 - crop_w) as u16,
        y: (center_y - crop_h / 2.0).clamp(0.0, 1000.0 - crop_h) as u16,
        w: crop_w as u16,
        h: crop_h as u16,
    }
    .clamped()
}

/// The editor's starting point on `source`: the first three seconds at
/// most, the centered square, nothing else.
pub fn initial_edits(source: &VideoSource) -> VideoEdits {
    VideoEdits {
        till_ms: source.duration_ms.clamp(0, MAX_DURATION_MS),
        crop: square_crop(source.width, source.height),
        ..VideoEdits::full(source.duration_ms)
    }
}

/// Keep the trimmed range inside the clip and between the shortest and the
/// longest sticker; the handle that was not moved gives way.
pub fn clamp_range(mut edits: VideoEdits, duration_ms: i64, moved_start: bool) -> VideoEdits {
    let duration = duration_ms.max(0);
    let floor = MIN_DURATION_MS.min(duration);
    if moved_start {
        edits.from_ms = edits.from_ms.clamp(0, duration.saturating_sub(floor));
        edits.till_ms = edits.till_ms.clamp(
            edits.from_ms + floor,
            (edits.from_ms + MAX_DURATION_MS).min(duration),
        );
    } else {
        edits.till_ms = edits.till_ms.clamp(floor, duration);
        edits.from_ms = edits.from_ms.clamp(
            (edits.till_ms - MAX_DURATION_MS).max(0),
            edits.till_ms - floor,
        );
    }
    edits.cover_ms = edits.cover_ms.clamp(edits.from_ms, edits.till_ms);
    edits
}

fn seconds(ms: i64) -> String {
    format!("{:.3}", ms as f64 / 1000.0)
}

/// ffmpeg arguments that encode the edited clip as a VP9 WEBM sticker with
/// alpha at `side`×`side`, with no sound and at most [`FPS_LIMIT`] frames a
/// second (`CreateVp9WebmEncoder`: `crf`, `auto-alt-ref 0`, no bitrate).
pub fn webm_args(
    input: &str,
    output: &str,
    edits: &VideoEdits,
    source: &VideoSource,
    side: u32,
    crf: u32,
) -> Vec<String> {
    let side = (side.max(2) & !1).to_string();
    let mut args: Vec<String> = ["-y", "-ss", &seconds(edits.from_ms), "-t"]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect();
    args.push(seconds(edits.length_ms().max(1)));
    args.extend(["-i".to_string(), input.to_string(), "-an".to_string()]);
    let mut filters = crate::video_edit::turn_filters(edits.turns, edits.flipped);
    if let Some(crop) = edits.crop {
        let (turned_w, turned_h) = turned_size(source.width, source.height, edits.turns);
        let (x, y, w, h) = crop.pixels(turned_w, turned_h);
        filters.push(format!("crop={w}:{h}:{x}:{y}"));
    }
    filters.push(format!("scale={side}:{side}:flags=lanczos"));
    filters.push(format!("fps=min(source_fps\\,{FPS_LIMIT})"));
    filters.push("format=yuva420p".into());
    args.extend(["-vf".to_string(), filters.join(",")]);
    for arg in [
        "-c:v",
        "libvpx-vp9",
        "-pix_fmt",
        "yuva420p",
        "-b:v",
        "0",
        "-crf",
        &crf.clamp(0, 63).to_string(),
        "-auto-alt-ref",
        "0",
        "-f",
        "webm",
        output,
    ] {
        args.push(arg.to_string());
    }
    args
}

/// Why a sticker could not be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// ffmpeg isn't installed; the message says how to get it.
    FfmpegMissing(String),
    /// ffmpeg refused the file, or no quality step fits the size limit.
    BadVideo,
    /// The box was closed while encoding; nothing to tell.
    Cancelled,
}

impl EncodeError {
    /// The toast for the failure; none for a cancel.
    pub fn note(&self) -> Option<String> {
        match self {
            Self::FfmpegMissing(message) => Some(message.clone()),
            Self::BadVideo => Some(BAD_VIDEO_NOTE.to_string()),
            Self::Cancelled => None,
        }
    }
}

/// The video editor's result, waiting in the "New Sticker" box for its
/// emoji and "Done" (tdesktop's `CreateMediaBox` encodes on Done too, with
/// a cancel flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoStickerJob {
    pub source: PathBuf,
    pub edits: VideoEdits,
    pub info: VideoSource,
}

/// The encoded sticker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedSticker {
    pub path: PathBuf,
    pub bytes: u64,
    /// Length in milliseconds, clamped to the sticker range as tdesktop
    /// does before uploading.
    pub duration_ms: i64,
}

fn work_dir() -> PathBuf {
    crate::scratch::temp_work_dir("quill-sticker-video")
}

fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}

/// Encode `source_path` with `edits` for a set of `kind`, walking the
/// quality ladder until the file is small enough (`RunWebmSticker`).
/// `cancelled` is read between quality steps; a step already running
/// finishes. Blocks; call it off the UI thread.
pub fn encode_sticker(
    source_path: &Path,
    edits: &VideoEdits,
    source: &VideoSource,
    kind: SetKind,
    cancelled: &AtomicBool,
) -> Result<EncodedSticker, EncodeError> {
    if !crate::media_tools::is_installed("ffmpeg") {
        return Err(EncodeError::FfmpegMissing(
            crate::media_tools::ffmpeg_missing_message("Video stickers"),
        ));
    }
    // The ffmpeg Quill ships has no VP9 encoder (libvpx); one on PATH may.
    let Some(program) = crate::video_encoder::program_with_encoder("libvpx-vp9") else {
        return Err(EncodeError::FfmpegMissing(
            "Video stickers need an ffmpeg that can write VP9 (libvpx), and the one found can't. \
             Install ffmpeg, for example `brew install ffmpeg` or `sudo apt install ffmpeg`."
                .to_string(),
        ));
    };
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|_| EncodeError::BadVideo)?;
    let output = dir.join(format!("sticker-{}.webm", stamp()));
    let max_bytes = max_webm_bytes(kind);
    let input = source_path.to_string_lossy();
    let out = output.to_string_lossy();
    let mut last = Err(EncodeError::BadVideo);
    for crf in CRF_LADDER {
        if cancelled.load(Ordering::Relaxed) {
            last = Err(EncodeError::Cancelled);
            break;
        }
        let args = webm_args(&input, &out, edits, source, kind.image_side(), crf);
        let mut command = crate::video_encoder::quiet_command_for(&program);
        command.args(&args);
        let limit = crate::child_timeout::transcode_limit(edits.length_ms() as f64 / 1000.0);
        let ran = crate::child_timeout::output_within(&mut command, limit)
            .is_ok_and(|result| result.status.success());
        let bytes = ran
            .then(|| std::fs::metadata(&output).ok().map(|meta| meta.len()))
            .flatten();
        match bytes {
            Some(bytes) if bytes > 0 && bytes <= max_bytes => {
                return Ok(EncodedSticker {
                    path: output,
                    bytes,
                    duration_ms: edits.length_ms().clamp(1, MAX_DURATION_MS),
                });
            }
            Some(_) => last = Err(EncodeError::BadVideo),
            // ffmpeg itself failed: a later step won't help.
            None => break,
        }
    }
    let _ = std::fs::remove_file(&output);
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(width: u32, height: u32, duration_ms: i64) -> VideoSource {
        VideoSource {
            duration_ms,
            width,
            height,
            has_audio: true,
            ..Default::default()
        }
    }

    #[test]
    fn limits_follow_tdesktop() {
        assert_eq!(max_webm_bytes(SetKind::Stickers), 262_144);
        assert_eq!(max_webm_bytes(SetKind::Emoji), 65_536);
        assert_eq!(CRF_LADDER[0], 28);
        assert!(accepts_source(&source(640, 480, 200)));
        assert!(
            accepts_source(&source(640, 480, 120)),
            "short clips are taken whole"
        );
        assert!(!accepts_source(&source(640, 480, 0)));
        assert!(!accepts_source(&source(0, 480, 2000)));
        assert_eq!(EncodeError::Cancelled.note(), None);
        assert_eq!(
            EncodeError::BadVideo.note().as_deref(),
            Some(BAD_VIDEO_NOTE)
        );
    }

    #[test]
    fn the_initial_edits_take_three_seconds_and_the_centered_square() {
        let edits = initial_edits(&source(640, 480, 10_000));
        assert_eq!((edits.from_ms, edits.till_ms), (0, 3000));
        let crop = edits.crop.expect("not square");
        assert_eq!((crop.w, crop.h), (750, 1000));
        assert_eq!(crop.x, 125);
        assert_eq!(initial_edits(&source(480, 480, 1000)).crop, None);
        assert_eq!(initial_edits(&source(480, 480, 1000)).till_ms, 1000);
    }

    #[test]
    fn the_range_stays_between_the_shortest_and_longest_sticker() {
        let edits = VideoEdits {
            from_ms: 1000,
            till_ms: 6000,
            ..VideoEdits::full(10_000)
        };
        let moved_end = clamp_range(edits, 10_000, false);
        assert_eq!((moved_end.from_ms, moved_end.till_ms), (3000, 6000));
        let moved_start = clamp_range(edits, 10_000, true);
        assert_eq!((moved_start.from_ms, moved_start.till_ms), (1000, 4000));
        let tiny = clamp_range(
            VideoEdits {
                from_ms: 900,
                till_ms: 950,
                ..VideoEdits::full(1000)
            },
            1000,
            true,
        );
        assert_eq!(tiny.till_ms - tiny.from_ms, 200);
        assert!(tiny.till_ms <= 1000);
    }

    #[test]
    fn crops_are_squared_in_pixels() {
        let crop = Crop {
            x: 0,
            y: 0,
            w: 500,
            h: 1000,
        };
        // 1000×500 frame: a 500‰ width is 500 px, so the height is the
        // whole frame.
        let squared = squared_crop(crop, 1000, 500, true);
        assert_eq!((squared.w, squared.h), (500, 1000));
        // Height set to half (250 px): width follows to 250‰, centered
        // where the crop was.
        let narrowed = squared_crop(
            Crop {
                x: 0,
                y: 250,
                w: 500,
                h: 500,
            },
            1000,
            500,
            false,
        );
        assert_eq!((narrowed.w, narrowed.h), (250, 500));
        assert_eq!(narrowed.x, 125);
        assert_eq!(square_crop(0, 10), None);
    }

    #[test]
    fn webm_arguments_encode_alpha_vp9_without_sound() {
        let edits = VideoEdits {
            from_ms: 500,
            till_ms: 2500,
            crop: Some(Crop {
                x: 125,
                y: 0,
                w: 750,
                h: 1000,
            }),
            flipped: true,
            turns: 1,
            ..VideoEdits::full(10_000)
        };
        let args = webm_args(
            "in.webm",
            "out.webm",
            &edits,
            &source(640, 480, 10_000),
            100,
            34,
        );
        let joined = args.join(" ");
        assert!(joined.starts_with("-y -ss 0.500 -t 2.000 -i in.webm -an -vf "));
        let filters = args
            .iter()
            .position(|arg| arg == "-vf")
            .and_then(|at| args.get(at + 1))
            .expect("filters");
        assert!(filters.starts_with("transpose=1,hflip,crop="));
        assert!(
            filters
                .contains(",scale=100:100:flags=lanczos,fps=min(source_fps\\,30),format=yuva420p")
        );
        assert!(joined.contains(
            "-c:v libvpx-vp9 -pix_fmt yuva420p -b:v 0 -crf 34 -auto-alt-ref 0 -f webm out.webm"
        ));
        assert!(!joined.contains("-c:a"));
        let plain = webm_args(
            "a",
            "b",
            &VideoEdits::full(1000),
            &source(512, 512, 1000),
            512,
            99,
        );
        assert!(plain.join(" ").contains("-crf 63"));
        assert!(!plain.join(" ").contains("crop="));
    }

    /// This ffmpeg can encode VP9 (`-encoders` lists `libvpx-vp9`).
    fn has_vp9_encoder() -> bool {
        crate::media_tools::quiet_command("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .is_ok_and(|out| {
                out.status.success() && String::from_utf8_lossy(&out.stdout).contains("libvpx-vp9")
            })
    }

    #[test]
    fn a_cancelled_encode_does_nothing() {
        let cancelled = AtomicBool::new(true);
        let result = encode_sticker(
            Path::new("missing.webm"),
            &VideoEdits::full(1000),
            &source(100, 100, 1000),
            SetKind::Stickers,
            &cancelled,
        );
        assert!(matches!(
            result,
            Err(EncodeError::Cancelled | EncodeError::FfmpegMissing(_))
        ));
    }

    #[test]
    fn ffmpeg_encodes_a_generated_clip_into_a_small_webm_sticker() {
        // Skipped where ffmpeg can't encode VP9 (Linux CI has no ffmpeg).
        if !crate::media_tools::is_installed("ffmpeg") || !has_vp9_encoder() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("quill-sticker-video-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("source.mp4");
        let made = crate::media_tools::command("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=4:size=320x240:rate=30",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&clip)
            .output()
            .is_ok_and(|out| out.status.success());
        let Some(probed) = made
            .then(|| crate::video_edit::probe_source(&clip))
            .flatten()
        else {
            return;
        };
        let edits = initial_edits(&probed);
        let cancelled = AtomicBool::new(false);
        let encoded = encode_sticker(&clip, &edits, &probed, SetKind::Emoji, &cancelled)
            .expect("a VP9 ffmpeg encodes the clip");
        assert!(encoded.bytes <= MAX_EMOJI_WEBM_BYTES);
        assert_eq!(encoded.duration_ms, 3000);
        let result = crate::video_edit::probe_source(&encoded.path).expect("probe result");
        assert_eq!((result.width, result.height), (100, 100));
        assert!(!result.has_audio);
        assert!((2800..=3200).contains(&result.duration_ms));
        let _ = std::fs::remove_file(&encoded.path);
        let _ = std::fs::remove_dir_all(dir);
    }
}
