//! Images and drawing helpers for the Touch Bar items.

use gpui_kit::Rgba;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{AnyThread, Message};
use objc2_app_kit::{
    NSBezierPath, NSBitmapImageRep, NSColor, NSCompositingOperation, NSDeviceRGBColorSpace, NSFont,
    NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext, NSImage, NSImageName,
    NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// tdesktop `kIdealIconSize`: an emoji in the scrubber.
const EMOJI_SIDE: f64 = 36.;

pub(super) fn color(c: Rgba) -> Retained<NSColor> {
    NSColor::colorWithSRGBRed_green_blue_alpha(
        f64::from(c.r),
        f64::from(c.g),
        f64::from(c.b),
        f64::from(c.a),
    )
}

/// A system image (`NSImageNameTouchBar…`), as a template.
pub(super) fn named(name: &NSImageName) -> Option<Retained<NSImage>> {
    NSImage::imageNamed(name)
}

/// An SF Symbol, `side` points tall.
pub(super) fn symbol(name: &str, side: f64) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        None,
    )?;
    let size = image.size();
    if size.height > 0. {
        image.setSize(NSSize::new(size.width * side / size.height, side));
    }
    image.setTemplate(true);
    Some(image)
}

thread_local! {
    /// Decoded pictures by file, so redraws don't read files again.
    static FILES: RefCell<HashMap<PathBuf, Option<Retained<NSImage>>>> =
        RefCell::new(HashMap::new());
}

/// The picture in `path` (a profile photo, a sticker), cached.
pub(super) fn file_image(path: &Path) -> Option<Retained<NSImage>> {
    if let Some(cached) = FILES.with(|files| {
        files
            .try_borrow()
            .ok()
            .and_then(|files| files.get(path).cloned())
    }) {
        return cached;
    }
    let image = NSImage::initWithContentsOfFile(
        NSImage::alloc(),
        &NSString::from_str(&path.to_string_lossy()),
    );
    FILES.with(|files| {
        if let Ok(mut files) = files.try_borrow_mut() {
            // A long session sees many pictures; start over now and then.
            if files.len() > 512 {
                files.clear();
            }
            files.insert(path.to_path_buf(), image.clone());
        }
    });
    image
}

/// An emoji drawn by the system's color emoji font (tdesktop
/// `CreateNSImageFromEmoji`).
pub(super) fn emoji_image(emoji: &str) -> Retained<NSImage> {
    let text = NSString::from_str(emoji);
    let handler = block2::RcBlock::new(move |rect: NSRect| -> Bool {
        let font = NSFont::systemFontOfSize(rect.size.height * 0.8);
        let attributes = attributes(&font, None);
        // SAFETY: the attributes hold a font and a color, as AppKit expects.
        let size = unsafe { text.sizeWithAttributes(Some(&attributes)) };
        let origin = NSPoint::new(
            (rect.size.width - size.width) / 2.,
            (rect.size.height - size.height) / 2.,
        );
        // SAFETY: drawing into the image's own context while AppKit runs
        // this handler on the main thread.
        unsafe { text.drawAtPoint_withAttributes(origin, Some(&attributes)) };
        Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(EMOJI_SIDE, EMOJI_SIDE),
        false,
        &handler,
    )
}

/// Font (and color) attributes for `NSString` drawing.
pub(super) fn attributes(
    font: &NSFont,
    fg: Option<&NSColor>,
) -> Retained<NSDictionary<NSString, AnyObject>> {
    // SAFETY: the attribute keys are AppKit's own constants.
    let (font_key, color_key) = unsafe { (NSFontAttributeName, NSForegroundColorAttributeName) };
    match fg {
        Some(fg) => NSDictionary::from_slices(
            &[font_key, color_key],
            &[font as &AnyObject, fg as &AnyObject],
        ),
        None => NSDictionary::from_slices(&[font_key], &[font as &AnyObject]),
    }
}

/// Fill an oval.
pub(super) fn fill_oval(rect: NSRect, fill: &NSColor) {
    fill.setFill();
    NSBezierPath::bezierPathWithOvalInRect(rect).fill();
}

/// Draw `image` into a circle at `rect`.
pub(super) fn draw_round(image: &NSImage, rect: NSRect) {
    NSGraphicsContext::saveGraphicsState_class();
    NSBezierPath::bezierPathWithOvalInRect(rect).addClip();
    image.drawInRect_fromRect_operation_fraction(
        rect,
        NSRect::ZERO,
        NSCompositingOperation::SourceOver,
        1.,
    );
    NSGraphicsContext::restoreGraphicsState_class();
}

/// Draw `text` centered in `rect`.
pub(super) fn draw_centered(text: &str, rect: NSRect, font: &NSFont, fg: &NSColor) {
    let string = NSString::from_str(text);
    let attributes = attributes(font, Some(fg));
    // SAFETY: the attributes hold a font and a color, as AppKit expects.
    let size = unsafe { string.sizeWithAttributes(Some(&attributes)) };
    let origin = NSPoint::new(
        rect.origin.x + (rect.size.width - size.width) / 2.,
        rect.origin.y + (rect.size.height - size.height) / 2.,
    );
    // SAFETY: called from a view's drawRect: on the main thread.
    unsafe { string.drawAtPoint_withAttributes(origin, Some(&attributes)) };
}

/// Draw a template symbol tinted `fg`, centered in `rect` at `side`.
pub(super) fn draw_symbol(name: &str, rect: NSRect, side: f64, fg: &NSColor) {
    let Some(image) = symbol(name, side) else {
        return;
    };
    let size = image.size();
    let target = NSRect::new(
        NSPoint::new(
            rect.origin.x + (rect.size.width - size.width) / 2.,
            rect.origin.y + (rect.size.height - size.height) / 2.,
        ),
        size,
    );
    // Tint: draw the template, then paint the color over its pixels.
    let tinted = NSImage::imageWithSize_flipped_drawingHandler(
        size,
        false,
        &block2::RcBlock::new({
            let image = image.clone();
            let fg = fg.retain();
            move |bounds: NSRect| -> Bool {
                image.drawInRect(bounds);
                fg.setFill();
                objc2_app_kit::NSRectFillUsingOperation(bounds, NSCompositingOperation::SourceAtop);
                Bool::YES
            }
        }),
    );
    tinted.drawInRect(target);
}

/// A kit icon (SVG) as a template image `side` points tall, drawn at 2x.
pub(super) fn svg_template(data: &[u8], side: f64) -> Option<Retained<NSImage>> {
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let pixels = (side * 2.).round().max(1.) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels, pixels)?;
    let size = tree.size();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            pixels as f32 / size.width(),
            pixels as f32 / size.height(),
        ),
        &mut pixmap.as_mut(),
    );
    let wide = pixels as isize;
    // SAFETY: null planes make the rep own a buffer of `wide` rows of
    // `wide * 4` bytes, premultiplied RGBA as tiny-skia renders; the copy
    // below stays inside it.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            wide,
            wide,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            wide * 4,
            32,
        )
    }?;
    let source = pixmap.data();
    let target = rep.bitmapData();
    if target.is_null()
        || rep.bytesPerRow() != wide * 4
        || source.len() != (wide * wide * 4) as usize
    {
        return None;
    }
    // SAFETY: both buffers hold exactly `source.len()` bytes (checked above).
    unsafe { std::ptr::copy_nonoverlapping(source.as_ptr(), target, source.len()) };
    let image = NSImage::initWithSize(NSImage::alloc(), NSSize::new(side, side));
    image.addRepresentation(&rep);
    image.setTemplate(true);
    Some(image)
}
