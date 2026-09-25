//! History playback for `messageAnimation` (GIF-style).
//!
//! tdesktop shows the JPEG thumbnail until the clip is local, then loops it
//! (`history/view/media` GIF). Unigram does the same with TDLib `animation`.
//! Still images display directly. MPEG-4 (`video/mp4`, the saved-GIF format)
//! frames come from `ffmpeg` when it is installed. Quill does not vendor a
//! decoder and does not call Tenor.

use std::path::{Path, PathBuf};
use std::process::Command;

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
        let dir = std::env::temp_dir().join(format!("quill-gif-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("still.png");
        std::fs::write(&png, b"not-a-real-png").unwrap();
        let frames = playback_frames(&png, "image/png", &dir.join("cache")).unwrap();
        assert_eq!(frames, vec![png]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
