//! macOS menu-bar icon with an unread counter, drawn the way Telegram
//! Desktop does it (`platform/mac/tray_mac.mm` `UpdateIcon` /
//! `PlaceCounter`):
//!
//! - no unread: the monochrome glyph as a template image, so AppKit tints it
//!   for the menu bar;
//! - unread: a regular image with the glyph in the menu bar's foreground
//!   (white on a dark bar, 70% black on a light one, read from the status
//!   button's effective appearance) and a red rounded counter at the
//!   bottom-right with white digits, antialiased by AppKit. "..NN" past 99
//!   and a grey counter when every unread chat is muted, as in tdesktop.
//!
//! Drawn at 44x44 px (22 pt at 2x, tdesktop's `side`).

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAppearanceCustomization, NSBezierPath, NSBitmapImageRep, NSColor, NSDeviceRGBColorSpace,
    NSFont, NSFontAttributeName, NSForegroundColorAttributeName, NSGraphicsContext, NSStatusItem,
    NSStringDrawing,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// Canvas edge in pixels (22 pt menu-bar icon at 2x).
pub const SIDE: u32 = 44;

/// tdesktop `st::trayCounterBg` / `st::trayCounterBgMute`.
const COUNTER_BG: (f64, f64, f64) = (
    0xF2 as f64 / 255.0,
    0x3C as f64 / 255.0,
    0x34 as f64 / 255.0,
);
const COUNTER_BG_MUTED: (f64, f64, f64) = (
    0x88 as f64 / 255.0,
    0x88 as f64 / 255.0,
    0x88 as f64 / 255.0,
);

/// Whether the status item sits on a dark menu bar (tdesktop reads the
/// button's `effectiveAppearance` name).
pub fn menu_bar_is_dark(item: &NSStatusItem) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return true;
    };
    let Some(button) = item.button(mtm) else {
        return true;
    };
    let name = button.effectiveAppearance().name();
    name.to_string().to_lowercase().contains("dark")
}

/// Counter text, tdesktop style: the number below 100, "..NN" above.
pub fn counter_text(count: u32) -> String {
    if count < 100 {
        count.to_string()
    } else {
        format!("..{:02}", count % 100)
    }
}

/// The glyph mask (64x64) area-resampled to `SIDE` and colored for the bar.
fn glyph(mask: &[u8], dark: bool) -> Vec<u8> {
    let src = crate::tray::ICON_SIZE as usize;
    let dst = SIDE as usize;
    let scale = src as f64 / dst as f64;
    let mut out = vec![0u8; dst * dst * 4];
    let (rgb, max_alpha) = if dark { (255u8, 255.0) } else { (0u8, 180.0) };
    for y in 0..dst {
        for x in 0..dst {
            // Average the source pixels this destination pixel covers.
            let (x0, x1) = (x as f64 * scale, (x + 1) as f64 * scale);
            let (y0, y1) = (y as f64 * scale, (y + 1) as f64 * scale);
            let mut sum = 0.0;
            let mut area = 0.0;
            let mut sy = y0.floor() as usize;
            while (sy as f64) < y1 && sy < src {
                let wy = (y1.min(sy as f64 + 1.0) - y0.max(sy as f64)).max(0.0);
                let mut sx = x0.floor() as usize;
                while (sx as f64) < x1 && sx < src {
                    let wx = (x1.min(sx as f64 + 1.0) - x0.max(sx as f64)).max(0.0);
                    sum += f64::from(mask[sy * src + sx]) * wx * wy;
                    area += wx * wy;
                    sx += 1;
                }
                sy += 1;
            }
            let coverage = if area > 0.0 { sum / area / 255.0 } else { 0.0 };
            let i = (y * dst + x) * 4;
            out[i] = rgb;
            out[i + 1] = rgb;
            out[i + 2] = rgb;
            out[i + 3] = (coverage * max_alpha).round() as u8;
        }
    }
    out
}

/// The counter pill + digits drawn by AppKit into a transparent `SIDE`²
/// premultiplied RGBA bitmap (top row first). `None` if AppKit refuses.
fn counter_layer(text: &str, muted: bool) -> Option<Vec<u8>> {
    let side = SIDE as isize;
    // SAFETY: an offscreen NSBitmapImageRep owned here, drawn into through
    // its own graphics context on the main thread; the bitmap buffer is
    // `side * side * 4` bytes and read only after drawing finished.
    unsafe {
        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            side,
            side,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            side * 4,
            32,
        )?;
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&rep)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));

        // tdesktop PlaceCounter at size 44: skip 2, font 16, d 6/5, r 9/11.
        let skip = 2.0;
        let font = NSFont::systemFontOfSize(16.0);
        let white = NSColor::whiteColor();
        let attributes: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
            &[NSFontAttributeName, NSForegroundColorAttributeName],
            &[&*font as &AnyObject, &*white as &AnyObject],
        );
        let string = NSString::from_str(text);
        let size: NSSize = string.sizeWithAttributes(Some(&attributes));
        let text_w = size.width.ceil();
        let height = (font.ascender() - font.descender()).ceil();
        let single = text.chars().count() < 2;
        let (d, r): (f64, f64) = if single { (6.0, 9.0) } else { (5.0, 11.0) };
        let width = text_w + d * 2.0;
        let x = f64::from(SIDE) - width - skip;
        let rect = NSRect::new(NSPoint::new(x, skip), NSSize::new(width, height));
        let (cr, cg, cb) = if muted { COUNTER_BG_MUTED } else { COUNTER_BG };
        NSColor::colorWithSRGBRed_green_blue_alpha(cr, cg, cb, 1.0).setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            rect,
            r.min(height / 2.0),
            r.min(height / 2.0),
        )
        .fill();
        string.drawAtPoint_withAttributes(
            NSPoint::new(
                x + (width - size.width) / 2.0,
                skip + (height - size.height) / 2.0,
            ),
            Some(&attributes),
        );

        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();
        let data = rep.bitmapData();
        if data.is_null() {
            return None;
        }
        Some(std::slice::from_raw_parts(data, (SIDE * SIDE * 4) as usize).to_vec())
    }
}

/// Composite the premultiplied counter layer over the straight-alpha glyph,
/// clearing a 2 px gap around the counter like the template variant.
fn composite(glyph: &mut [u8], counter: &[u8]) {
    let side = SIDE as usize;
    // Gap: any glyph pixel within 2 px of an opaque counter pixel is cleared.
    let mut near = vec![false; side * side];
    for y in 0..side {
        for x in 0..side {
            if counter[(y * side + x) * 4 + 3] > 96 {
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if (0..side as i32).contains(&nx) && (0..side as i32).contains(&ny) {
                            near[ny as usize * side + nx as usize] = true;
                        }
                    }
                }
            }
        }
    }
    for i in 0..side * side {
        let p = i * 4;
        if near[i] {
            glyph[p + 3] = 0;
        }
        let ca = f64::from(counter[p + 3]) / 255.0;
        if ca <= 0.0 {
            continue;
        }
        let ga = f64::from(glyph[p + 3]) / 255.0;
        let out_a = ca + ga * (1.0 - ca);
        for c in 0..3 {
            // Counter is premultiplied; glyph is straight.
            let premul = f64::from(counter[p + c]) / 255.0
                + f64::from(glyph[p + c]) / 255.0 * ga * (1.0 - ca);
            glyph[p + c] = ((premul / out_a).clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        glyph[p + 3] = (out_a * 255.0).round() as u8;
    }
}

/// The menu-bar icon for `unread` (> 0) as straight RGBA, `SIDE`².
pub fn render(mask: &[u8], unread: u32, muted: bool, dark: bool) -> (Vec<u8>, u32, u32) {
    let mut pixels = glyph(mask, dark);
    if unread > 0
        && let Some(counter) = counter_layer(&counter_text(unread), muted)
    {
        composite(&mut pixels, &counter);
    }
    (pixels, SIDE, SIDE)
}

#[cfg(test)]
mod tests {
    use super::counter_text;

    #[test]
    fn counter_text_matches_tdesktop() {
        assert_eq!(counter_text(7), "7");
        assert_eq!(counter_text(99), "99");
        assert_eq!(counter_text(123), "..23");
        assert_eq!(counter_text(1005), "..05");
    }
}
