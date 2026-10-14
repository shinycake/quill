//! Demo capture only: draws every bar into a PNG, since the Macs Quill is
//! built on have no Touch Bar and Xcode no longer simulates one.
//! `QUILL_TOUCH_BAR_PREVIEW=/path/out.png quill --screenshot-demo <kind>`.

use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceNameDarkAqua, NSBitmapImageFileType,
    NSColor, NSImage, NSImageView, NSView,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize};
use std::path::PathBuf;

/// Space around and between the rows.
const GAP: f64 = 10.;
const HEIGHT: f64 = 30.;

/// Where to write the preview, when asked for.
pub(super) fn requested() -> Option<PathBuf> {
    std::env::var_os("QUILL_TOUCH_BAR_PREVIEW").map(PathBuf::from)
}

/// Lay `rows` out on black, as on the Touch Bar, and write a PNG.
pub(super) fn write(
    rows: Vec<Vec<Retained<NSView>>>,
    path: &std::path::Path,
    mtm: MainThreadMarker,
) {
    let mut placed = Vec::new();
    let mut width: f64 = 0.;
    for (row, views) in rows.iter().rev().enumerate() {
        let y = GAP + row as f64 * (HEIGHT + GAP);
        let mut x = GAP;
        for view in views {
            let fitting = view.fittingSize();
            let mut size = view.frame().size;
            if size.width < 1. {
                size.width = if fitting.width >= 1. {
                    fitting.width
                } else {
                    600.
                };
            }
            size.height = HEIGHT;
            view.setFrame(NSRect::new(NSPoint::new(x, y), size));
            x += size.width + GAP;
            placed.push(view.clone());
        }
        width = width.max(x);
    }
    let height = GAP + rows.len() as f64 * (HEIGHT + GAP);
    let bounds = NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, height));
    let container = NSView::initWithFrame(NSView::alloc(mtm), bounds);
    // SAFETY: an AppKit appearance name constant.
    let dark = NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua });
    container.setAppearance(dark.as_deref());
    let black = NSImage::imageWithSize_flipped_drawingHandler(
        bounds.size,
        false,
        &block2::RcBlock::new(|rect: NSRect| -> Bool {
            NSColor::blackColor().setFill();
            objc2_app_kit::NSRectFill(rect);
            Bool::YES
        }),
    );
    let background = NSImageView::imageViewWithImage(&black, mtm);
    background.setFrame(bounds);
    container.addSubview(&background);
    for view in &placed {
        container.addSubview(view);
    }
    container.layoutSubtreeIfNeeded();
    let Some(bitmap) = container.bitmapImageRepForCachingDisplayInRect(bounds) else {
        return;
    };
    container.cacheDisplayInRect_toBitmapImageRep(bounds, &bitmap);
    // SAFETY: an empty properties dictionary is valid for PNG.
    let data = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    };
    if let Some(data) = data {
        let _ = std::fs::write(path, data.to_vec());
    }
}
