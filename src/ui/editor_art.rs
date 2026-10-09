//! Pictures to place on a photo in the editor: emoji drawn with the
//! system's color emoji font, and stickers decoded to their first frame.

use image::RgbaImage;
use quill::telegram::envelope::StickerFormat;
use std::path::Path;

/// The emoji drawn at `edge` points by the system (Apple Color Emoji), as
/// straight RGBA. Must run on the main thread (AppKit drawing).
#[cfg(target_os = "macos")]
pub(super) fn rasterize_emoji(emoji: &str, edge: f64) -> Option<RgbaImage> {
    use objc2::AnyThread;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSFont, NSFontAttributeName, NSImage,
        NSStringDrawing,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSSize, NSString};
    // SAFETY: plain AppKit drawing into an offscreen image on the main
    // thread; every object passed is a live, retained Objective-C object.
    let png = unsafe {
        let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(edge, edge));
        let font = NSFont::systemFontOfSize(edge * 0.82);
        let attributes: objc2::rc::Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[NSFontAttributeName], &[&*font as &AnyObject]);
        #[allow(deprecated)]
        image.lockFocus();
        NSString::from_str(emoji)
            .drawAtPoint_withAttributes(NSPoint::new(edge * 0.04, edge * 0.06), Some(&attributes));
        #[allow(deprecated)]
        image.unlockFocus();
        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?
    };
    image::load_from_memory(&png.to_vec())
        .ok()
        .map(|image| image.to_rgba8())
        .map(trim_transparent)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn rasterize_emoji(_emoji: &str, _edge: f64) -> Option<RgbaImage> {
    None
}

/// A sticker's still picture (the first frame of animated ones).
pub(super) fn sticker_pixels(path: &Path, format: StickerFormat) -> Option<RgbaImage> {
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let image = match format {
        StickerFormat::Tgs => {
            let frames =
                quill::sticker_playback::decode_tgs_sized(path, 512, 1, f64::INFINITY, &cancelled)
                    .ok()?;
            let mut bytes = frames.frames.into_iter().next()?;
            // rlottie hands out premultiplied BGRA.
            for pixel in bytes.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
                let alpha = u16::from(pixel[3]);
                #[allow(clippy::manual_checked_ops)]
                if alpha > 0 {
                    for channel in &mut pixel[..3] {
                        *channel = (u16::from(*channel) * 255 / alpha).min(255) as u8;
                    }
                }
            }
            RgbaImage::from_raw(512, 512, bytes)?
        }
        StickerFormat::Webm => {
            let dir = std::env::temp_dir().join("quill-editor-sticker");
            let child = std::sync::Arc::new(std::sync::Mutex::new(None));
            let frames =
                quill::sticker_playback::decode_webm_sized(path, &dir, 512, 1, &child, &cancelled)
                    .ok()?;
            let first = frames.frames.into_iter().next()?;
            let image = image::open(first).ok()?.to_rgba8();
            let _ = std::fs::remove_dir_all(dir);
            image
        }
        _ => image::open(path).ok()?.to_rgba8(),
    };
    Some(trim_transparent(image))
}

/// Crop away fully transparent margins, so handles hug the picture.
fn trim_transparent(image: RgbaImage) -> RgbaImage {
    let (width, height) = image.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        if pixel[3] > 8 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if right < left || bottom < top {
        return image;
    }
    image::imageops::crop_imm(&image, left, top, right - left + 1, bottom - top + 1).to_image()
}
