//! Extracted-frame cache for webm stickers (`sticker_playback`): the frame
//! directory root and its display allowlist entry. GIF files decode in
//! process (`ui/gif_clip`) and MP4 GIFs play on the shared player, so no
//! GIF frames are extracted any more.
#![deny(clippy::indexing_slicing)]

use std::path::PathBuf;

/// Parent of every extracted frame directory: `{media_cache_base}/gif-frames`,
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
    crate::local_path::ensure_private_dir(&root);
    roots.push(root);
    roots
}

/// Remove extracted frames for one file. Source media outside the cache is left alone.
pub fn discard_frame_cache(file_id: i32) {
    let _ = std::fs::remove_dir_all(gif_frame_cache_dir(file_id));
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
