//! Circular sender avatar PNG for OS notifications.
//!
//! Reads the already-downloaded small chat photo, crops it to a centered
//! square, scales it to `ICON_SIZE`, masks it to a circle and caches the PNG
//! under the platform cache directory. Every failure returns `None`, so the
//! notification still shows with the app icon.

use super::QuillApp;
use gpui_kit::{AppContext, Context};
use image::{ImageFormat, RgbaImage, imageops};
use quill::ids::AccountKey;
use quill::notify::avatar::{
    ICON_SIZE, apply_circle_mask, existing_icon, icon_cache_dir, icon_cache_path,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub(in crate::ui) fn forget_cached_icons() {
    #[cfg(not(test))]
    quill::notify::avatar::clear_icons();
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

/// Where the icon for `source` is cached: `(cache directory, file)`.
fn cache_target(source: &Path) -> Option<(PathBuf, PathBuf)> {
    let meta = std::fs::metadata(source).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    let dir = icon_cache_dir()?;
    let target = icon_cache_path(&dir, source, meta.len(), mtime);
    Some((dir, target))
}

/// The icon already rendered for `source`; a stat, no decoding.
fn cached_icon_for(source: &Path) -> Option<PathBuf> {
    let (_, target) = cache_target(source)?;
    existing_icon(Some(target))
}

/// A notification held back until its avatar finishes decoding.
struct Waiting {
    account: Option<AccountKey>,
    notification: quill::notify::OsNotification,
    preview: Option<PathBuf>,
    locked: bool,
}

thread_local! {
    /// Decodes in flight on the main thread, by photo path, with the
    /// notifications waiting on each in arrival order. One decode serves
    /// every notification for that avatar.
    static DECODING: RefCell<HashMap<PathBuf, Vec<Waiting>>> = RefCell::new(HashMap::new());
}

impl QuillApp {
    /// Show a notification with the circular avatar made from `photo`. A
    /// cached icon shows at once; otherwise the photo decodes, scales and
    /// encodes on a background thread and the notification follows with
    /// the result (without an icon when that fails). `account` `None` is
    /// the account on screen; `preview` is the message's photo, shown as is.
    pub(in crate::ui) fn show_notification_with_photo(
        &mut self,
        account: Option<AccountKey>,
        notification: quill::notify::OsNotification,
        photo: Option<PathBuf>,
        preview: Option<PathBuf>,
        locked: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(photo) = photo else {
            self.show_notification_as(account.as_ref(), notification, None, preview, locked, cx);
            return;
        };
        if let Some(icon) = cached_icon_for(&photo) {
            self.show_notification_as(
                account.as_ref(),
                notification,
                Some(icon),
                preview,
                locked,
                cx,
            );
            return;
        }
        let waiting = Waiting {
            account,
            notification,
            preview,
            locked,
        };
        // A decode for this avatar is already running: queue behind it.
        let queued = DECODING.with(|map| {
            let mut map = map.try_borrow_mut().ok()?;
            match map.get_mut(&photo) {
                Some(list) => {
                    list.push(waiting);
                    None
                }
                None => {
                    map.insert(photo.clone(), Vec::new());
                    Some(waiting)
                }
            }
        });
        let Some(first) = queued else {
            return;
        };
        let key = photo.clone();
        let job = cx.background_spawn(async move { circular_icon_for(&photo) });
        Self::spawn_main(cx, async move |this, cx| {
            let icon = job.await;
            let rest = DECODING
                .with(|map| {
                    map.try_borrow_mut()
                        .ok()
                        .and_then(|mut map| map.remove(&key))
                })
                .unwrap_or_default();
            let _ = this.update(cx, |app, cx| {
                for item in std::iter::once(first).chain(rest) {
                    app.show_notification_as(
                        item.account.as_ref(),
                        item.notification,
                        icon.clone(),
                        item.preview,
                        item.locked,
                        cx,
                    );
                }
            });
        })
        .detach();
    }
}

/// Path of the circular icon for the chat photo at `source`, rendering and
/// caching it on first use. `None` when the photo is missing, unreadable or
/// the cache cannot be written.
pub(in crate::ui) fn circular_icon_for(source: &Path) -> Option<PathBuf> {
    let (dir, target) = cache_target(source)?;
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
