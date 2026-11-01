//! Sender avatar for OS notifications (tdesktop shows the chat userpic in
//! the toast: `Window::Notifications::Manager` hands the cached userpic
//! file to libnotify `image-path`, the Windows `appLogoOverride` and the
//! macOS notification attachment).
//!
//! This file holds the pure parts: the cache file name and the circular
//! mask. Decoding and PNG encoding live in `ui::notifications::avatar_icon`
//! because the `image` crate is only linked with the `ui` feature.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Side of the cached circular icon in pixels. Notification daemons draw
/// icons at 48 to 64 logical pixels; 128 stays sharp on HiDPI.
pub const ICON_SIZE: u32 = 128;

pub const ICON_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Debug, Clone, Copy)]
pub enum IconSweep {
    WrittenBefore(SystemTime),
    Everything,
}

pub fn icon_cache_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "shinycake", crate::settings::APP_DIR_NAME)
        .map(|dirs| dirs.cache_dir().join("notify-avatars"))
}

/// Cache file name for a source photo. TDLib file paths are unique per file
/// id, so the path plus the file length and mtime identifies the content;
/// a re-downloaded or changed photo gets a fresh icon.
pub fn icon_cache_name(source: &Path, len: u64, mtime_secs: u64) -> String {
    // FNV-1a: stable across runs (unlike `DefaultHasher`) and good enough
    // for a cache key.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    feed(source.to_string_lossy().as_bytes());
    feed(&len.to_le_bytes());
    feed(&mtime_secs.to_le_bytes());
    format!("{hash:016x}.png")
}

/// Full cache path for a source photo inside `cache_dir`.
pub fn icon_cache_path(cache_dir: &Path, source: &Path, len: u64, mtime_secs: u64) -> PathBuf {
    cache_dir.join(icon_cache_name(source, len, mtime_secs))
}

/// Make a square RGBA8 buffer circular: alpha outside the inscribed circle
/// becomes 0, with a one pixel antialiased edge. No-op for a buffer whose
/// length does not match `size * size * 4`.
pub fn apply_circle_mask(rgba: &mut [u8], size: u32) {
    let side = size as usize;
    if side == 0 || rgba.len() != side * side * 4 {
        return;
    }
    let center = size as f32 / 2.0;
    for y in 0..side {
        for x in 0..side {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            // Full coverage inside `center - 1`, none beyond `center`.
            let coverage = (center - distance).clamp(0.0, 1.0);
            if let Some(alpha) = rgba.get_mut((y * side + x) * 4 + 3) {
                *alpha = (f32::from(*alpha) * coverage).round() as u8;
            }
        }
    }
}

/// Keep the icon only when its file exists; otherwise the platform shows
/// the app icon.
pub fn existing_icon(candidate: Option<PathBuf>) -> Option<PathBuf> {
    candidate.filter(|path| path.is_file())
}

fn is_icon_name(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_suffix(".png"))
        .is_some_and(|stem| stem.len() == 16 && stem.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn sweep_icons(dir: &Path, sweep: IconSweep) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let expired = || match sweep {
            IconSweep::Everything => true,
            IconSweep::WrittenBefore(cutoff) => entry
                .metadata()
                .and_then(|meta| meta.modified())
                .is_ok_and(|written| written < cutoff),
        };
        if is_icon_name(&entry.file_name()) && expired() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn sweep_in_background(sweep: IconSweep) {
    if let Some(dir) = icon_cache_dir() {
        let _ = crate::thread_guard::spawn_guarded("quill-icon-sweep", move || {
            sweep_icons(&dir, sweep);
        });
    }
}

pub fn sweep_old_icons() {
    if let Some(cutoff) = SystemTime::now().checked_sub(ICON_MAX_AGE) {
        sweep_in_background(IconSweep::WrittenBefore(cutoff));
    }
}

pub fn clear_icons() {
    sweep_in_background(IconSweep::Everything);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_aged(dir: &Path, name: &str, written: SystemTime) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        file.set_modified(written).unwrap();
        path
    }

    #[test]
    fn a_sweep_takes_old_icons_and_nothing_else() {
        let dir = std::env::temp_dir().join(format!("quill-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let now = SystemTime::now();
        let long_ago = now - ICON_MAX_AGE - Duration::from_secs(60);
        let old = write_aged(&dir, "00112233aabbccdd.png", long_ago);
        let fresh = write_aged(&dir, "ffeeddcc33221100.png", now);
        let unrelated = write_aged(&dir, "notes.png", long_ago);
        let folder = dir.join("0011223344556677.png");
        std::fs::create_dir_all(&folder).unwrap();

        sweep_icons(&dir, IconSweep::WrittenBefore(now - ICON_MAX_AGE));
        assert!(!old.exists());
        assert!(fresh.exists());
        assert!(unrelated.exists());

        sweep_icons(&dir, IconSweep::Everything);
        assert!(!fresh.exists(), "a clear takes every icon");
        assert!(unrelated.exists());
        assert!(folder.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_nothing_to_sweep() {
        sweep_icons(Path::new("/definitely/not/here"), IconSweep::Everything);
    }

    #[test]
    fn cache_name_is_stable_and_content_sensitive() {
        let p = Path::new("/a/photo.jpg");
        assert_eq!(icon_cache_name(p, 10, 5), icon_cache_name(p, 10, 5));
        assert_ne!(icon_cache_name(p, 10, 5), icon_cache_name(p, 11, 5));
        assert_ne!(icon_cache_name(p, 10, 5), icon_cache_name(p, 10, 6));
        assert_ne!(
            icon_cache_name(p, 10, 5),
            icon_cache_name(Path::new("/b/photo.jpg"), 10, 5)
        );
        assert!(is_icon_name(OsStr::new(&icon_cache_name(p, 1, 1))));
    }

    #[test]
    fn mask_clears_corners_and_keeps_center() {
        let size = 16u32;
        let mut buf = vec![255u8; (size * size * 4) as usize];
        apply_circle_mask(&mut buf, size);
        let alpha = |x: usize, y: usize| buf[(y * size as usize + x) * 4 + 3];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(15, 15), 0);
        assert_eq!(alpha(8, 8), 255);
        assert!(alpha(8, 0) > 0, "edge midpoint keeps partial coverage");
    }

    #[test]
    fn mask_ignores_a_mismatched_buffer() {
        let mut buf = vec![255u8; 10];
        apply_circle_mask(&mut buf, 4);
        assert!(buf.iter().all(|b| *b == 255));
    }

    #[test]
    fn missing_icon_file_falls_back_to_none() {
        assert_eq!(existing_icon(None), None);
        assert_eq!(
            existing_icon(Some(PathBuf::from("/definitely/not/here.png"))),
            None
        );
    }
}
