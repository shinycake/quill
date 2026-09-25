//! History playback for `messageVideo`.
//!
//! tdesktop’s video bubble (`history/view/media`) shows the JPEG thumbnail, a
//! duration, and a play control, then plays inline. Unigram does the same with
//! TDLib `video`. GPUI has no video surface, so Quill extracts a short preview
//! with `ffmpeg` — the same approach as GIF frames — and loops those frames
//! until Pause. Frames live under `quill-video-frames/{file_id}`. That cache
//! root is on the display allowlist; other temp paths stay blocked.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Parent of every ffmpeg frame directory. `media_display_roots` must include this
/// path; a random file under the system temp dir stays outside the allowlist.
pub fn video_frame_cache_root() -> PathBuf {
    std::env::temp_dir().join("quill-video-frames")
}

/// Per-file frame directory: `{video_frame_cache_root}/{file_id}`.
pub fn video_frame_cache_dir(file_id: i32) -> PathBuf {
    video_frame_cache_root().join(file_id.to_string())
}

/// Account or demo roots plus the video frame cache. Creates the cache root so
/// `sandboxed_display_path` can canonicalize it.
pub fn with_video_frame_cache(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = video_frame_cache_root();
    let _ = std::fs::create_dir_all(&root);
    roots.push(root);
    roots
}

/// Remove extracted frames for one file. Source media outside the cache is left alone.
pub fn discard_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(video_frame_cache_dir(file_id));
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
    let frames = extract_frames(src, cache_dir, start_timestamp)?;
    if frames.is_empty() {
        return Err("ffmpeg produced no frames".into());
    }
    Ok(frames)
}

fn extract_frames(
    src: &Path,
    cache_dir: &Path,
    start_timestamp: i32,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(cache_dir).map_err(|err| err.to_string())?;
    let pattern = cache_dir.join("frame-%02d.png");
    let mut command = Command::new("ffmpeg");
    command.args(["-y", "-hide_banner", "-loglevel", "error"]);
    if start_timestamp > 0 {
        command.arg("-ss").arg(start_timestamp.to_string());
    }
    let status = command
        .arg("-i")
        .arg(src)
        .args(["-vf", "fps=8,scale=240:-1", "-frames:v", "12"])
        .arg(&pattern)
        .status()
        .map_err(|err| format!("video playback needs ffmpeg ({err})"))?;
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
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn non_video_is_rejected() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("quill-video-still-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("still.png");
        std::fs::write(&png, b"not-a-real-png").unwrap();
        let err = playback_frames(&png, "image/png", &dir.join("cache"), 0).unwrap_err();
        assert!(err.contains("unsupported"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn video_frame_cache_passes_display_sandbox_random_temp_does_not() {
        use crate::local_path::sandboxed_display_path;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let account = std::env::temp_dir().join(format!("quill-video-account-{nanos}"));
        std::fs::create_dir_all(&account).unwrap();
        let roots = with_video_frame_cache(vec![account.clone()]);
        let frame = video_frame_cache_dir(44).join("frame-01.png");
        std::fs::create_dir_all(frame.parent().unwrap()).unwrap();
        std::fs::write(&frame, b"png").unwrap();
        let shown = sandboxed_display_path(frame.to_str().unwrap(), &roots);
        assert_eq!(
            shown.as_deref(),
            Some(std::fs::canonicalize(&frame).unwrap().as_path())
        );

        let stray = std::env::temp_dir().join(format!("quill-video-stray-{nanos}.png"));
        std::fs::write(&stray, b"no").unwrap();
        assert!(sandboxed_display_path(stray.to_str().unwrap(), &roots).is_none());

        let gif_layout = std::env::temp_dir().join(format!("quill-gif-frames-{nanos}"));
        std::fs::create_dir_all(&gif_layout).unwrap();
        let gif_frame = gif_layout.join("frame-01.png");
        std::fs::write(&gif_frame, b"gif").unwrap();
        assert!(sandboxed_display_path(gif_frame.to_str().unwrap(), &roots).is_none());

        discard_frame_cache(44);
        let _ = std::fs::remove_dir_all(&account);
        let _ = std::fs::remove_file(&stray);
        let _ = std::fs::remove_dir_all(&gif_layout);
    }
}
