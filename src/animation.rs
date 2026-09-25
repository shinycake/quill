//! History playback for `messageAnimation` (GIF-style).
//!
//! tdesktop shows the JPEG thumbnail until the clip is local, then loops it
//! (`history/view/media` GIF). Unigram does the same with TDLib `animation`.
//! Still images display directly. MPEG-4 (`video/mp4`, the saved-GIF format)
//! frames come from `ffmpeg` when it is installed. Quill does not vendor a
//! decoder and does not call Tenor.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Parent of every ffmpeg frame directory. `media_display_roots` must include this
/// path; a random file under the system temp dir stays outside the allowlist.
pub fn gif_frame_cache_root() -> PathBuf {
    std::env::temp_dir().join("quill-gif-frames")
}

/// Per-file frame directory: `{gif_frame_cache_root}/{file_id}`.
pub fn gif_frame_cache_dir(file_id: i32) -> PathBuf {
    gif_frame_cache_root().join(file_id.to_string())
}

/// Account or demo roots plus the GIF frame cache. Creates the cache root so
/// `sandboxed_display_path` can canonicalize it.
pub fn with_gif_frame_cache(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = gif_frame_cache_root();
    let _ = std::fs::create_dir_all(&root);
    roots.push(root);
    roots
}

/// Remove extracted frames for one file. Source media outside the cache is left alone.
pub fn discard_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(gif_frame_cache_dir(file_id));
}

pub fn is_still_image(mime: &str, path: &Path) -> bool {
    let mime = mime.to_ascii_lowercase();
    if mime.starts_with("image/") && mime != "image/gif" {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

pub fn is_playable_clip(mime: &str, path: &Path) -> bool {
    let mime = mime.to_ascii_lowercase();
    if mime == "video/mp4" || mime == "image/gif" {
        return true;
    }
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "gif")
    )
}

/// Frames to cycle. A PNG/JPEG/WEBP path is one frame. GIF and MPEG-4 use
/// `ffmpeg` when present; a GIF with no ffmpeg falls back to the file itself.
pub fn playback_frames(src: &Path, mime: &str, cache_dir: &Path) -> Result<Vec<PathBuf>, String> {
    if is_playable_clip(mime, src)
        && let Ok(frames) = extract_frames(src, cache_dir)
        && !frames.is_empty()
    {
        return Ok(frames);
    }
    if is_still_image(mime, src) || is_playable_clip(mime, src) {
        return Ok(vec![src.to_path_buf()]);
    }
    Err("unsupported animation".into())
}

fn extract_frames(src: &Path, cache_dir: &Path) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(cache_dir).map_err(|err| err.to_string())?;
    let pattern = cache_dir.join("frame-%02d.png");
    let status = Command::new("ffmpeg")
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-vf", "fps=8,scale=240:-1", "-frames:v", "12"])
        .arg(&pattern)
        .status()
        .map_err(|err| format!("animation playback needs ffmpeg ({err})"))?;
    if !status.success() {
        return Err("ffmpeg could not read the animation".into());
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
    fn still_png_is_one_frame_without_ffmpeg() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("quill-gif-still-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("still.png");
        std::fs::write(&png, b"not-a-real-png").unwrap();
        let frames = playback_frames(&png, "image/png", &dir.join("cache")).unwrap();
        assert_eq!(frames, vec![png]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gif_frame_cache_passes_display_sandbox_random_temp_does_not() {
        use crate::local_path::sandboxed_display_path;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let account = std::env::temp_dir().join(format!("quill-gif-account-{nanos}"));
        std::fs::create_dir_all(&account).unwrap();
        let roots = with_gif_frame_cache(vec![account.clone()]);
        let frame = gif_frame_cache_dir(33).join("frame-01.png");
        std::fs::create_dir_all(frame.parent().unwrap()).unwrap();
        std::fs::write(&frame, b"png").unwrap();
        let shown = sandboxed_display_path(frame.to_str().unwrap(), &roots);
        assert_eq!(
            shown.as_deref(),
            Some(std::fs::canonicalize(&frame).unwrap().as_path())
        );

        let stray = std::env::temp_dir().join(format!("quill-gif-stray-{nanos}.png"));
        std::fs::write(&stray, b"no").unwrap();
        assert!(sandboxed_display_path(stray.to_str().unwrap(), &roots).is_none());

        let old_layout = std::env::temp_dir().join(format!("quill-gif-{nanos}"));
        std::fs::create_dir_all(&old_layout).unwrap();
        let old_frame = old_layout.join("frame-01.png");
        std::fs::write(&old_frame, b"old").unwrap();
        assert!(sandboxed_display_path(old_frame.to_str().unwrap(), &roots).is_none());

        discard_frame_cache(33);
        let _ = std::fs::remove_dir_all(&account);
        let _ = std::fs::remove_file(&stray);
        let _ = std::fs::remove_dir_all(&old_layout);
    }
}
