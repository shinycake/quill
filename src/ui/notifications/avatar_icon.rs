//! Circular sender avatar PNG for OS notifications.
//!
//! Reads the already-downloaded small chat photo, crops it to a centered
//! square, scales it to `ICON_SIZE`, masks it to a circle and caches the PNG
//! under the platform cache directory. Every failure returns `None`, so the
//! notification still shows with the app icon.

use image::{ImageFormat, RgbaImage, imageops};
use quill::notify::avatar::{ICON_SIZE, apply_circle_mask, existing_icon, icon_cache_path};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

fn cache_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "shinycake", quill::settings::APP_DIR_NAME)
        .map(|dirs| dirs.cache_dir().join("notify-avatars"))
}

/// Decode `source`, crop to a centered square, scale and mask to a circle.
fn render_circular(source: &Path) -> Option<RgbaImage> {
    let decoded = image::open(source).ok()?.to_rgba8();
    let (w, h) = decoded.dimensions();
    let side = w.min(h);
    if side == 0 {
        return None;
    }
    let square =
        imageops::crop_imm(&decoded, (w - side) / 2, (h - side) / 2, side, side).to_image();
    let mut icon = imageops::resize(
        &square,
        ICON_SIZE,
        ICON_SIZE,
        imageops::FilterType::Triangle,
    );
    apply_circle_mask(icon.as_mut(), ICON_SIZE);
    Some(icon)
}

/// Path of the circular icon for the chat photo at `source`, rendering and
/// caching it on first use. `None` when the photo is missing, unreadable or
/// the cache cannot be written.
pub(in crate::ui) fn circular_icon_for(source: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(source).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let dir = cache_dir()?;
    let target = icon_cache_path(&dir, source, meta.len(), mtime);
    if let Some(hit) = existing_icon(Some(target.clone())) {
        return Some(hit);
    }
    let icon = render_circular(source)?;
    std::fs::create_dir_all(&dir).ok()?;
    // Write beside the target and rename so a concurrent notify-send never
    // reads a half-written PNG.
    let partial = target.with_extension("png.part");
    icon.save_with_format(&partial, ImageFormat::Png).ok()?;
    std::fs::rename(&partial, &target).ok()?;
    Some(target)
}

#[cfg(test)]
mod tests {
    use super::{ICON_SIZE, circular_icon_for, render_circular};
    use image::RgbaImage;
    use std::path::Path;

    #[test]
    fn renders_a_round_icon_from_a_rectangular_photo() {
        let dir = std::env::temp_dir().join(format!("quill-avatar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("photo.png");
        RgbaImage::from_pixel(60, 40, image::Rgba([200, 30, 30, 255]))
            .save(&src)
            .unwrap();
        let icon = render_circular(&src).unwrap();
        assert_eq!(icon.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(icon.get_pixel(0, 0).0[3], 0);
        assert_eq!(icon.get_pixel(ICON_SIZE / 2, ICON_SIZE / 2).0[3], 255);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_photo_gives_no_icon() {
        assert!(circular_icon_for(Path::new("/definitely/not/a/photo.jpg")).is_none());
    }
}
