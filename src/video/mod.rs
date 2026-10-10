//! History playback for `messageVideo`.
//!
//! tdesktop’s video bubble (`history/view/media`) shows the JPEG thumbnail, a
//! duration, and a play control, then plays inline. Unigram does the same with
//! TDLib `video`. GPUI has no video surface, so Quill extracts a short preview
//! with `ffmpeg` — the same approach as GIF frames — and loops those frames
//! until Pause. Frames live under `quill-media-cache/{account}/video-frames/{file_id}`.
//! That cache root is on the display allowlist; other temp paths stay blocked.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Parent of every ffmpeg frame directory: `{media_cache_base}/video-frames`,
/// created 0700. That cache root is on the display allowlist; other temp
/// paths stay blocked.
pub fn video_frame_cache_root() -> PathBuf {
    crate::local_path::media_cache_base().join("video-frames")
}

/// Per-file frame directory: `{video_frame_cache_root}/{file_id}`.
pub fn video_frame_cache_dir(file_id: i32) -> PathBuf {
    video_frame_cache_root().join(file_id.to_string())
}

/// Parent of every viewer frame directory: `{media_cache_base}/viewer-frames`,
/// created 0700. The media viewer extracts full-clip frames at viewer
/// resolution, separate from the row preview's 12-frame cache, so the two
/// never share a directory.
pub fn viewer_frame_cache_root() -> PathBuf {
    crate::local_path::media_cache_base().join("viewer-frames")
}

/// Per-file viewer frame directory: `{viewer_frame_cache_root}/{file_id}`.
pub fn viewer_frame_cache_dir(file_id: i32) -> PathBuf {
    viewer_frame_cache_root().join(file_id.to_string())
}

/// Account or demo roots plus the viewer frame cache. Creates the cache root
/// (0700, symlink-safe) so `sandboxed_display_path` can canonicalize it.
pub fn with_viewer_frame_cache(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = viewer_frame_cache_root();
    crate::local_path::ensure_private_dir(&root);
    roots.push(root);
    roots
}

/// Remove extracted viewer frames for one file.
pub fn discard_viewer_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(viewer_frame_cache_dir(file_id));
}

/// Account or demo roots plus the video frame cache. Creates the cache root
/// (0700, symlink-safe) so `sandboxed_display_path` can canonicalize it.
pub fn with_video_frame_cache(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = video_frame_cache_root();
    crate::local_path::ensure_private_dir(&root);
    roots.push(root);
    roots
}

/// Remove extracted frames for one file. Source media outside the cache is left alone.
pub fn discard_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(video_frame_cache_dir(file_id));
}

/// Dimensions and streaming hint for `inputVideo` (TDLib 1.8.67).
///
/// tdesktop probes with ffmpeg before `documentAttributeVideo`. Duration, width,
/// and height are required ints. `supports_streaming` is the sender hint levlam
/// describes: a `moov` atom at the start or end of an MPEG-4 file. Thumbnail
/// upload is skipped (`inputVideo.thumbnail` null); TDLib generates one for
/// small clips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoProbe {
    pub duration: i32,
    pub width: i32,
    pub height: i32,
    pub supports_streaming: bool,
}

/// Square side and duration for `inputVideoNote` (TDLib 1.8.67).
///
/// `length` is width and height. The schema requires it positive and at most
/// 640. `duration` is seconds in 0–60. A non-square clip is not a round note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoNoteProbe {
    pub duration: i32,
    pub length: i32,
}

/// JPEG still for `inputVideoNote.thumbnail`. Width and height are the scaled
/// frame (schema: usually not above 320). Missing `ffmpeg` skips the upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoNoteThumbnail {
    pub path: PathBuf,
    pub width: i32,
    pub height: i32,
}

/// Probe a user-picked round clip. Rejects landscape/portrait video, a side
/// above 640, and a duration above 60. Does not re-encode.
pub fn probe_local_video_note(path: &Path) -> Result<VideoNoteProbe, String> {
    let probe = probe_local_video(path)?;
    if probe.width != probe.height {
        return Err("video note must be square".into());
    }
    if !(1..=640).contains(&probe.width) {
        return Err("video note length must be 1-640".into());
    }
    if !(0..=60).contains(&probe.duration) {
        return Err("video note duration must be 0-60".into());
    }
    Ok(VideoNoteProbe {
        duration: probe.duration,
        length: probe.width,
    })
}

/// Parent of video-note thumbnail scratch: `{media_cache_base}/video-note-thumbs`,
/// created 0700.
fn video_note_thumbnail_dir() -> PathBuf {
    crate::local_path::media_cache_base().join("video-note-thumbs")
}

/// First frame as JPEG, 240×240. `None` when ffmpeg is missing or the file
/// cannot be read — the schema says pass null to skip thumbnail uploading.
pub fn write_video_note_thumbnail(src: &Path) -> Option<VideoNoteThumbnail> {
    // Same shared `{temp}/quill-media-cache` tree the viewer-cache sweep
    // test deletes. Hold the lock for the whole write so a parallel sweep
    // cannot remove the directory mid-ffmpeg or recreate it under the
    // sweep test's absence check.
    #[cfg(test)]
    let _guard = crate::local_path::lock_shared_media_cache();
    let dir = video_note_thumbnail_dir();
    crate::local_path::secure_create_dir(&dir).ok()?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let dest = dir.join(format!("{}-{nanos}.jpg", std::process::id()));
    let status = crate::media_tools::command("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-frames:v", "1", "-vf", "scale=240:240", "-q:v", "5"])
        .arg(&dest)
        .status()
        .ok()?;
    if !status.success() || !dest.is_file() {
        let _ = std::fs::remove_file(&dest);
        return None;
    }
    // Owner-only thumbnail (defense in depth; the 0700 parent is the barrier).
    let _ = crate::local_path::restrict_file(&dest);
    Some(VideoNoteThumbnail {
        path: dest,
        width: 240,
        height: 240,
    })
}

/// Read duration, width, and height from a user-picked local video.
/// Prefers `ffprobe` (same ffmpeg family as playback). Stream duration is used
/// when it is a number; `N/A` or a missing stream duration falls back to
/// `format.duration`. MPEG-4 `mvhd` / `tkhd` boxes are only a last resort.
pub fn probe_local_video(path: &Path) -> Result<VideoProbe, String> {
    if !is_playable_video("", path) {
        return Err("unsupported video".into());
    }
    let supports_streaming = mpeg4_has_moov(path);
    if let Some(probe) = probe_with_ffprobe(path, supports_streaming) {
        return Ok(probe);
    }
    if let Some(probe) = probe_mp4_boxes(path, supports_streaming) {
        return Ok(probe);
    }
    Err("could not read video duration or size".into())
}

pub(crate) fn probe_with_ffprobe(path: &Path, supports_streaming: bool) -> Option<VideoProbe> {
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
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    probe_from_ffprobe_json(&value, supports_streaming)
}

/// Width and height come from the first video stream. Duration prefers that
/// stream, then `format.duration` when the stream value is missing or `N/A`
/// (common for some WebM and Matroska files).
fn probe_from_ffprobe_json(
    value: &serde_json::Value,
    supports_streaming: bool,
) -> Option<VideoProbe> {
    let stream = value.get("streams")?.get(0)?;
    let width = stream.get("width")?.as_i64()?;
    let height = stream.get("height")?.as_i64()?;
    let duration = json_seconds(stream.get("duration")).or_else(|| {
        json_seconds(
            value
                .get("format")
                .and_then(|format| format.get("duration")),
        )
    })?;
    finish_probe(duration, width, height, supports_streaming)
}

fn json_seconds(value: Option<&serde_json::Value>) -> Option<f64> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    if let Some(number) = value.as_f64() {
        return number.is_finite().then_some(number);
    }
    let text = value.as_str()?.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("N/A") {
        return None;
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}

fn probe_mp4_boxes(path: &Path, supports_streaming: bool) -> Option<VideoProbe> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > 32 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mut offset = 0usize;
    let mut movie: Option<(u32, u32)> = None;
    let mut track: Option<(i64, i64)> = None;
    while offset + 8 <= bytes.len() {
        let size = u32::from_be_bytes(bytes[offset..offset + 4].try_into().ok()?) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        if size < 8 || offset + size > bytes.len() {
            break;
        }
        if kind == b"moov" {
            walk_moov(&bytes[offset + 8..offset + size], &mut movie, &mut track);
        }
        offset += size;
    }
    let (timescale, duration) = movie?;
    let (width, height) = track?;
    if timescale == 0 {
        return None;
    }
    let seconds = duration as f64 / f64::from(timescale);
    finish_probe(seconds, width, height, supports_streaming)
}

fn walk_moov(data: &[u8], movie: &mut Option<(u32, u32)>, track: &mut Option<(i64, i64)>) {
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let size =
            u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap_or([0; 4])) as usize;
        if size < 8 || offset + size > data.len() {
            break;
        }
        let kind = &data[offset + 4..offset + 8];
        let body = &data[offset + 8..offset + size];
        if kind == b"mvhd" {
            *movie = parse_mvhd(body);
        } else if kind == b"trak"
            && track.is_none()
            && let Some(size) = find_tkhd(body)
        {
            *track = Some(size);
        }
        offset += size;
    }
}

fn find_tkhd(data: &[u8]) -> Option<(i64, i64)> {
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let size = u32::from_be_bytes(data[offset..offset + 4].try_into().ok()?) as usize;
        if size < 8 || offset + size > data.len() {
            break;
        }
        let kind = &data[offset + 4..offset + 8];
        let body = &data[offset + 8..offset + size];
        if kind == b"tkhd" {
            return parse_tkhd(body);
        }
        if (kind == b"mdia" || kind == b"minf" || kind == b"stbl")
            && let Some(found) = find_tkhd(body)
        {
            return Some(found);
        }
        offset += size;
    }
    None
}

fn parse_mvhd(body: &[u8]) -> Option<(u32, u32)> {
    let version = *body.first()?;
    if version == 0 && body.len() >= 20 {
        let timescale = u32::from_be_bytes(body[12..16].try_into().ok()?);
        let duration = u32::from_be_bytes(body[16..20].try_into().ok()?);
        Some((timescale, duration))
    } else if version == 1 && body.len() >= 32 {
        let timescale = u32::from_be_bytes(body[20..24].try_into().ok()?);
        let duration = u64::from_be_bytes(body[24..32].try_into().ok()?) as u32;
        Some((timescale, duration))
    } else {
        None
    }
}

fn parse_tkhd(body: &[u8]) -> Option<(i64, i64)> {
    let version = *body.first()?;
    let (width, height) = if version == 0 && body.len() >= 84 {
        (&body[76..80], &body[80..84])
    } else if version == 1 && body.len() >= 96 {
        (&body[88..92], &body[92..96])
    } else {
        return None;
    };
    let width = i32::from_be_bytes(width.try_into().ok()?) as i64 >> 16;
    let height = i32::from_be_bytes(height.try_into().ok()?) as i64 >> 16;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((width, height))
}

fn finish_probe(
    seconds: f64,
    width: i64,
    height: i64,
    supports_streaming: bool,
) -> Option<VideoProbe> {
    if !(width > 0 && height > 0 && width <= i32::MAX as i64 && height <= i32::MAX as i64) {
        return None;
    }
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    let rounded = seconds.round() as i64;
    let duration = if seconds > 0.0 && rounded == 0 {
        1
    } else {
        i32::try_from(rounded).unwrap_or(i32::MAX)
    };
    Some(VideoProbe {
        duration,
        width: width as i32,
        height: height as i32,
        supports_streaming,
    })
}

/// True when an MPEG-4 `moov` box is present. That is the layout official
/// clients mark with `supports_streaming` (moov at the start or the end).
fn mpeg4_has_moov(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(len) = file.metadata().map(|meta| meta.len()) else {
        return false;
    };
    let mut offset = 0u64;
    let mut header = [0u8; 8];
    while offset + 8 <= len {
        if file.seek(SeekFrom::Start(offset)).is_err() || file.read_exact(&mut header).is_err() {
            return false;
        }
        let size32 = u32::from_be_bytes(header[0..4].try_into().unwrap_or([0; 4]));
        let size = if size32 == 1 {
            let mut ext = [0u8; 8];
            if file.read_exact(&mut ext).is_err() {
                return false;
            }
            u64::from_be_bytes(ext)
        } else if size32 == 0 {
            len - offset
        } else {
            u64::from(size32)
        };
        if size < 8 || offset.saturating_add(size) > len {
            return false;
        }
        if &header[4..8] == b"moov" {
            return true;
        }
        offset += size;
    }
    false
}

pub fn is_playable_video(mime: &str, path: &Path) -> bool {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("video/") {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "webm" | "mkv")
    )
}

/// Frames to cycle. `start_timestamp` is `messageVideo.start_timestamp` (seconds).
/// A missing `ffmpeg` or an unreadable file is an error.
pub fn playback_frames(
    src: &Path,
    mime: &str,
    cache_dir: &Path,
    start_timestamp: i32,
) -> Result<Vec<PathBuf>, String> {
    if !is_playable_video(mime, src) {
        return Err("unsupported video".into());
    }
    let frames = extract_frames(src, cache_dir, start_timestamp, 8.0, 240, 12, None, None)?;
    if frames.is_empty() {
        return Err("ffmpeg produced no frames".into());
    }
    Ok(frames)
}

/// Full-clip frames for the media viewer (parity slice 5).
///
/// The row preview extracts 12 frames at 8 fps; the viewer needs the whole
/// clip so decoded frames render in-place. Frame rate adapts to `duration_secs`
/// to bound the cache: 8 fps for clips up to 75 s, then fewer fps to stay
/// under `VIEWER_MAX_FRAMES` (600). `fps` is the actual extraction rate, used
/// to map the playback clock to a frame index.
#[derive(Debug)]
pub struct ViewerFrames {
    pub frames: Vec<PathBuf>,
    pub fps: f64,
}

/// Maximum viewer frames per clip (75 s at 8 fps). Longer clips get a lower
/// fps so the full duration stays covered.
pub const VIEWER_MAX_FRAMES: i32 = 600;
/// Viewer extraction frame rate for clips within the frame budget.
pub const VIEWER_FPS: f64 = 8.0;
/// Viewer frame width in px (matches the viewer's fixed frame).
pub const VIEWER_FRAME_WIDTH: i32 = 720;

pub fn viewer_playback_frames(
    src: &Path,
    mime: &str,
    cache_dir: &Path,
    start_timestamp: i32,
    duration_secs: i32,
) -> Result<ViewerFrames, String> {
    if !is_playable_video(mime, src) {
        return Err("unsupported video".into());
    }
    let fps = viewer_fps_for_duration(duration_secs);
    let frames = extract_frames(
        src,
        cache_dir,
        start_timestamp,
        fps,
        VIEWER_FRAME_WIDTH,
        VIEWER_MAX_FRAMES,
        None,
        None,
    )?;
    if frames.is_empty() {
        return Err("ffmpeg produced no frames".into());
    }
    Ok(ViewerFrames { frames, fps })
}

/// Like `viewer_playback_frames`, but the running ffmpeg child is published
/// into `child_slot` while it runs (cleared when it exits), so the UI can
/// kill an extraction the user abandoned (viewer closed/stepped) instead of
/// letting it run to completion on a discarded cache dir. `cancelled` is a
/// shared flag the UI sets when the run is abandoned: it is checked before
/// spawning and right after publishing the child, closing the race where
/// the UI kills between extraction start and the child being published —
/// in that window the worker kills its own freshly spawned child.
pub fn viewer_playback_frames_cancelable(
    src: &Path,
    mime: &str,
    cache_dir: &Path,
    start_timestamp: i32,
    duration_secs: i32,
    child_slot: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<ViewerFrames, String> {
    if !is_playable_video(mime, src) {
        return Err("unsupported video".into());
    }
    let fps = viewer_fps_for_duration(duration_secs);
    let frames = extract_frames(
        src,
        cache_dir,
        start_timestamp,
        fps,
        VIEWER_FRAME_WIDTH,
        VIEWER_MAX_FRAMES,
        Some((child_slot, cancelled)),
        None,
    )?;
    if frames.is_empty() {
        return Err("ffmpeg produced no frames".into());
    }
    Ok(ViewerFrames { frames, fps })
}

/// Extraction fps for a clip: 8 fps up to 75 s, then fewer fps to stay
/// under `VIEWER_MAX_FRAMES` (600). Minimum 1 fps.
fn viewer_fps_for_duration(duration_secs: i32) -> f64 {
    let fps = if duration_secs > 0 {
        (f64::from(VIEWER_MAX_FRAMES) / f64::from(duration_secs)).min(VIEWER_FPS)
    } else {
        VIEWER_FPS
    };
    fps.max(1.0)
}

fn wait_for_cancelable_child(
    command: &mut Command,
    slot: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<std::process::ExitStatus, String> {
    use std::sync::atomic::Ordering;
    if cancelled.load(Ordering::SeqCst) {
        return Err("ffmpeg extraction was cancelled".to_string());
    }
    let child = command
        .spawn()
        .map_err(|err| format!("video playback needs ffmpeg ({err})"))?;
    // Publish the running child so the UI can kill an abandoned
    // extraction (viewer closed/stepped). The worker polls
    // `try_wait` instead of blocking in `wait` so it never holds
    // the slot lock while the UI killer needs it.
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(child);
    }
    if cancelled.load(Ordering::SeqCst) {
        // The UI abandoned the run between spawn and publish and
        // already took the (empty) slot: kill our own child instead
        // of orphaning it.
        if let Ok(mut guard) = slot.lock()
            && let Some(mut child) = guard.take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        return Err("ffmpeg extraction was cancelled".to_string());
    }
    loop {
        let mut guard = slot
            .lock()
            .map_err(|_| "extraction slot poisoned".to_string())?;
        let Some(child) = guard.as_mut() else {
            // Slot emptied by the UI killer: it took the child and
            // is killing/reaping it — report cancellation. (The UI
            // drops stale completions by epoch instead of showing
            // an error.)
            return Err("ffmpeg extraction was cancelled".to_string());
        };
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            guard.take();
            return Ok(status);
        }
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_frames(
    src: &Path,
    cache_dir: &Path,
    start_timestamp: i32,
    fps: f64,
    width: i32,
    max_frames: i32,
    child_slot: Option<(
        &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
        &std::sync::atomic::AtomicBool,
    )>,
    input_decoder: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    crate::local_path::secure_create_dir(cache_dir).map_err(|err| err.to_string())?;
    let pattern = cache_dir.join("frame-%03d.png");
    let mut command = crate::media_tools::command("ffmpeg");
    command.args(["-y", "-hide_banner", "-loglevel", "error"]);
    if start_timestamp > 0 {
        command.arg("-ss").arg(start_timestamp.to_string());
    }
    if let Some(decoder) = input_decoder {
        command.args(["-c:v", decoder]);
    }
    command
        .arg("-i")
        .arg(src)
        .args([
            "-vf",
            &format!("fps={fps:.2},scale={width}:{width}:force_original_aspect_ratio=decrease"),
            "-frames:v",
            &max_frames.to_string(),
        ])
        .arg(&pattern)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let status = match child_slot {
        Some((slot, cancelled)) => wait_for_cancelable_child(&mut command, slot, cancelled)?,
        None => command
            .status()
            .map_err(|err| format!("video playback needs ffmpeg ({err})"))?,
    };
    if !status.success() {
        return Err("ffmpeg could not read the video".into());
    }
    let mut frames: Vec<PathBuf> = std::fs::read_dir(cache_dir)
        .map_err(|err| err.to_string())?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect();
    frames.sort();
    // Owner-only frame files (defense in depth; the 0700 parent is the barrier).
    for frame in &frames {
        let _ = crate::local_path::restrict_file(frame);
    }
    Ok(frames)
}

/// TGX `RecordAudioVideoController`: round video-note capture resolution —
/// 480px with "Record HQ Round Videos" on, 280px otherwise.
pub fn round_video_size(hq: bool) -> i32 {
    if hq { 480 } else { 280 }
}

/// Finished camera capture ready for `inputMessageVideoNote`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoNoteDraft {
    /// Square, transcoded MP4 ready to send.
    pub path: PathBuf,
    pub duration_secs: i32,
    /// Square side in pixels (`inputVideoNote.length`).
    pub length: i32,
}

/// Side of the live camera preview frames, in pixels (BGRA).
pub const ROUND_PREVIEW_SIDE: u32 = 360;

/// tdesktop `RoundVideoRecorder` `kMaxDuration`.
pub const ROUND_MAX_SECS: i32 = 60;

/// The newest camera preview frame: a sequence number and BGRA pixels.
pub type PreviewFrame = Arc<Mutex<(u64, Vec<u8>)>>;

/// In-progress round video-note camera capture (tdesktop
/// `RoundVideoRecorder`): ffmpeg records the default camera and
/// microphone, center-crops to a square and mirrors it (selfie view, as
/// tdesktop sends it), encodes H.264 + AAC, and streams small BGRA
/// frames back for the live round preview.
pub struct VideoNoteCapture {
    path: PathBuf,
    log: PathBuf,
    started: std::time::Instant,
    child: Option<std::process::Child>,
    preview: PreviewFrame,
    size: i32,
}

impl VideoNoteCapture {
    /// Start recording. Fails when there is no camera or ffmpeg is missing.
    pub fn start(hq: bool) -> Result<Self, String> {
        if !crate::media_tools::is_installed("ffmpeg") {
            return Err(crate::media_tools::ffmpeg_missing_message("Video messages"));
        }
        let input = crate::media_tools::capture_input()?;
        let path = crate::voice::capture_path("video-note", "mp4");
        let log = path.with_extension("log");
        let log_file = std::fs::File::create(&log).map_err(|err| err.to_string())?;
        let size = round_video_size(hq);
        let preview = ROUND_PREVIEW_SIDE;
        let mut child = crate::media_tools::spawn_capture(
            &round_capture_args(&input, size, &path),
            Stdio::piped(),
            Stdio::from(log_file),
        )
        .map_err(|err| format!("Couldn't start ffmpeg ({err})."))?;
        let frames: PreviewFrame = Arc::default();
        if let Some(mut stdout) = child.stdout.take() {
            let frames = frames.clone();
            std::thread::spawn(move || {
                use std::io::Read;
                let mut frame = vec![0u8; (preview * preview * 4) as usize];
                while stdout.read_exact(&mut frame).is_ok() {
                    if let Ok(mut latest) = frames.lock() {
                        latest.0 += 1;
                        latest.1.clone_from(&frame);
                    }
                }
            });
        }
        Ok(Self {
            path,
            log,
            started: std::time::Instant::now(),
            child: Some(child),
            preview: frames,
            size,
        })
    }

    pub fn elapsed_secs(&self) -> i32 {
        self.started
            .elapsed()
            .as_secs()
            .min(u64::from(i32::MAX as u32)) as i32
    }

    /// Recording progress toward the 60 s limit, `0.0..=1.0`.
    pub fn progress(&self) -> f32 {
        (self.started.elapsed().as_secs_f32() / ROUND_MAX_SECS as f32).min(1.0)
    }

    /// The newest camera frame, `ROUND_PREVIEW_SIDE`² BGRA, with its
    /// sequence number (0 until the camera delivers).
    pub fn preview(&self) -> PreviewFrame {
        self.preview.clone()
    }

    /// Once ffmpeg has exited by itself: `Ok` when it reached the time
    /// limit with a clip, otherwise why it stopped (no camera access…).
    pub fn ended(&mut self) -> Option<Result<(), String>> {
        let child = self.child.as_mut()?;
        let status = child.try_wait().ok().flatten()?;
        self.child = None;
        let recorded = std::fs::metadata(&self.path).is_ok_and(|meta| meta.len() > 0);
        Some(if status.success() && recorded {
            Ok(())
        } else {
            Err(crate::media_tools::capture_failure(&self.log, true))
        })
    }

    pub fn discard(mut self) {
        self.stop_child(false);
        let _ = std::fs::remove_file(&self.path);
    }

    /// Stop recording and keep the clip when it is a valid round video.
    pub fn finish(mut self) -> Result<VideoNoteDraft, String> {
        self.stop_child(true);
        let len = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if len == 0 || !self.path.is_file() {
            let reason = crate::media_tools::capture_failure(&self.log, true);
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(&self.log);
            return Err(reason);
        }
        let _ = std::fs::remove_file(&self.log);
        let probe = probe_local_video(&self.path)
            .map_err(|err| format!("recorded clip unreadable ({err})"))?;
        if probe.width != probe.height || probe.width != self.size {
            let _ = std::fs::remove_file(&self.path);
            return Err("recorded clip is not a square round video".into());
        }
        Ok(VideoNoteDraft {
            path: std::mem::take(&mut self.path),
            duration_secs: probe.duration.clamp(1, ROUND_MAX_SECS),
            length: probe.width,
        })
    }

    fn stop_child(&mut self, graceful: bool) {
        if let Some(mut child) = self.child.take() {
            crate::media_tools::stop_capture(&mut child, graceful);
        }
    }
}

/// ffmpeg arguments for a round capture from `input`: the square,
/// mirrored `size`² clip to `path`, and `ROUND_PREVIEW_SIDE`² BGRA preview
/// frames on stdout.
fn round_capture_args(
    input: &crate::media_tools::CaptureInput,
    size: i32,
    path: &Path,
) -> Vec<String> {
    let preview = ROUND_PREVIEW_SIDE;
    let filter = format!(
        // AVFoundation reports no frame rate (a 1 MHz time base), which an
        // MP4 would take as its constant rate: `fps` pins 30.
        "[{video}]crop='min(iw,ih)':'min(iw,ih)',hflip,fps=30,split=2[rec][pv];\
         [rec]scale={size}:{size},format=yuv420p[out];\
         [pv]scale={preview}:{preview},format=bgra[preview]",
        video = input.video,
    );
    let mut args: Vec<String> = ["-y", "-hide_banner", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    // Windows stops a capture by writing `q` to ffmpeg's stdin; elsewhere
    // stdin is closed and SIGINT stops it.
    if !cfg!(windows) {
        args.push("-nostdin".into());
    }
    args.push("-t".into());
    args.push(ROUND_MAX_SECS.to_string());
    args.extend(input.args.iter().cloned());
    for arg in [
        "-filter_complex",
        &filter,
        "-map",
        "[out]",
        "-map",
        input.audio,
        "-c:v",
        "libx264",
        "-preset",
        "veryfast",
        "-crf",
        "24",
        "-c:a",
        "aac",
        "-b:a",
        "64k",
        "-ac",
        "1",
        "-movflags",
        "+faststart",
    ] {
        args.push(arg.to_string());
    }
    args.push(path.to_string_lossy().into_owned());
    for arg in ["-map", "[preview]", "-f", "rawvideo", "pipe:1"] {
        args.push(arg.to_string());
    }
    args
}

impl Drop for VideoNoteCapture {
    fn drop(&mut self) {
        if self.child.is_some() {
            self.stop_child(false);
        }
        let _ = std::fs::remove_file(&self.log);
    }
}

#[cfg(test)]
mod tests;
