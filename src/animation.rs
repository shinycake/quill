//! History playback for `messageAnimation` (GIF-style).
//!
//! tdesktop shows the JPEG thumbnail until the clip is local, then loops it
//! (`history/view/media` GIF). Unigram does the same with TDLib `animation`.
//! Still images display directly. MPEG-4 (`video/mp4`, the saved-GIF format)
//! frames come from `ffmpeg` when it is installed. Quill does not vendor a
//! decoder and does not call Tenor.

use std::path::{Path, PathBuf};

/// Parent of every ffmpeg frame directory: `{media_cache_base}/gif-frames`,
/// created 0700 — decrypted frames are never world-readable. `roots` passed
/// to `sandboxed_display_path` must include this path; a random file under
/// the system temp dir stays outside the allowlist.
pub fn gif_frame_cache_root() -> PathBuf {
    crate::local_path::media_cache_base().join("gif-frames")
}

/// Per-file frame directory: `{gif_frame_cache_root}/{file_id}`.
pub fn gif_frame_cache_dir(file_id: i32) -> PathBuf {
    gif_frame_cache_root().join(file_id.to_string())
}

/// Account or demo roots plus the GIF frame cache. Creates the cache root
/// (0700, symlink-safe) so `sandboxed_display_path` can canonicalize it.
pub fn with_gif_frame_cache(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let root = gif_frame_cache_root();
    let _ = crate::local_path::secure_create_dir(&root);
    roots.push(root);
    roots
}

/// Remove extracted frames for one file. Source media outside the cache is left alone.
pub fn discard_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(gif_frame_cache_dir(file_id));
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

/// Full GIF/MP4 loop frames, using the shared cancellable ffmpeg pipeline.
/// Sampling drops below 24 fps for long clips to bound the cache at 600 frames.
pub fn full_playback_frames_cancelable(
    src: &Path,
    mime: &str,
    cache_dir: &Path,
    slot: &std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<crate::video::ViewerFrames, String> {
    if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("GIF extraction cancelled".into());
    }
    if !is_playable_clip(mime, src) {
        return Err("unsupported animation".into());
    }
    let duration = crate::video::probe_with_ffprobe(src, false)
        .ok_or_else(|| "could not probe GIF duration".to_string())?
        .duration;
    if duration <= 0 || duration > 60_000 {
        return Err("GIF duration is outside the supported range".into());
    }
    // ponytail: 600 sampled frames; a streaming decoder is needed for high frame rates on long clips.
    // Round down to ffmpeg's two-decimal rate; never truncate a loop by rounding up.
    let fps = ((600.0 / (f64::from(duration) + 1.0)).min(24.0) * 100.0).floor() / 100.0;
    if fps < 0.01 {
        return Err("GIF duration is outside the supported range".into());
    }
    let frames = crate::video::extract_frames(
        src,
        cache_dir,
        0,
        fps,
        240,
        600,
        Some((slot, cancelled)),
        None,
    )?;
    if frames.is_empty() {
        return Err("ffmpeg produced no GIF frames".into());
    }
    Ok(crate::video::ViewerFrames { frames, fps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn gif_frame_cache_passes_display_sandbox_random_temp_does_not() {
        use crate::local_path::sandboxed_display_path;
        // Creates `{temp}/quill-media-cache/{account}/gif-frames`.
        let _guard = crate::local_path::lock_shared_media_cache();

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

    #[test]
    #[ignore = "requires ffmpeg and ffprobe; run explicitly for native decoder evidence"]
    fn full_loop_covers_the_whole_gif_and_mp4_and_honors_cancel() {
        use std::sync::{Arc, Mutex, atomic::AtomicBool};
        let dir = std::env::temp_dir().join(format!("quill-gif-full-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mp4 = dir.join("clip.mp4");
        assert!(
            Command::new("ffmpeg")
                .args([
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=32x64:rate=24:duration=3",
                    "-c:v",
                    "mpeg4"
                ])
                .arg(&mp4)
                .status()
                .unwrap()
                .success()
        );
        let gif = dir.join("clip.gif");
        assert!(
            Command::new("ffmpeg")
                .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
                .arg(&mp4)
                .arg(&gif)
                .status()
                .unwrap()
                .success()
        );
        let slot = Arc::new(Mutex::new(None));
        let cancel = AtomicBool::new(false);
        for (index, path, mime) in [(0, &gif, "image/gif"), (1, &mp4, "video/mp4")] {
            let result = full_playback_frames_cancelable(
                path,
                mime,
                &dir.join(index.to_string()),
                &slot,
                &cancel,
            )
            .unwrap();
            assert!(
                result.frames.len() >= 60 && result.frames.len() <= 80,
                "full 3s clip, not twelve-frame preview: {}",
                result.frames.len()
            );
            assert_eq!(result.fps, 24.0);
            let png = std::fs::read(&result.frames[0]).unwrap();
            assert!(u32::from_be_bytes(png[16..20].try_into().unwrap()) <= 240);
            assert!(u32::from_be_bytes(png[20..24].try_into().unwrap()) <= 240);
            assert_ne!(
                std::fs::read(&result.frames[0]).unwrap(),
                std::fs::read(result.frames.last().unwrap()).unwrap()
            );
            assert!(slot.lock().unwrap().is_none());
        }
        cancel.store(true, std::sync::atomic::Ordering::SeqCst);
        let cancelled_cache = dir.join("cancelled");
        assert!(
            full_playback_frames_cancelable(&gif, "image/gif", &cancelled_cache, &slot, &cancel)
                .is_err()
        );
        assert!(!cancelled_cache.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
