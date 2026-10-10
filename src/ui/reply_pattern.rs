//! The custom emoji repeated behind a reply strip, tinted in the strip's
//! color (Telegram Desktop `FillBackgroundEmoji`, `ValidateBackgroundEmoji`
//! in `history_view_reply.cpp`). The emoji's first frame is the mask: its
//! alpha is kept and its color replaced, then three copies at 12, 16 and
//! 20 px are placed along the right edge at fixed offsets and opacities.

use gpui_kit::*;
use image::RgbaImage;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Logical sides of the three copies.
const SIZES: [u32; 3] = [12, 16, 20];

/// Physical pixels per logical pixel the copies are made at; GPUI scales
/// them down on a 1x display.
const SCALE: u32 = 2;

/// One glyph of the pattern: its left edge `x` px from the strip's right
/// edge, its `y` from the top, which size it uses and its opacity
/// (Desktop's table, without the gift variant).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Glyph {
    pub x: i32,
    pub y: i32,
    pub size: usize,
    pub opacity: f32,
}

const fn glyph(x: i32, y: i32, size: usize, opacity: f32) -> Glyph {
    Glyph {
        x,
        y,
        size,
        opacity,
    }
}

const PATTERN: [Glyph; 9] = [
    glyph(28, 4, 2, 0.32),
    glyph(51, 15, 1, 0.32),
    glyph(64, -2, 0, 0.28),
    glyph(87, 11, 1, 0.24),
    glyph(125, -2, 2, 0.16),
    glyph(28, 31, 1, 0.24),
    glyph(72, 33, 2, 0.2),
    glyph(46, 52, 1, 0.24),
    glyph(24, 55, 2, 0.18),
];

/// A quote strip shifts the pattern left by the quote icon's width and
/// adds two copies on the left.
const QUOTE_SHIFT: i32 = 12;
const QUOTE_EXTRA: [Glyph; 2] = [glyph(4, 23, 1, 0.28), glyph(0, 48, 0, 0.24)];

/// The glyphs to draw, at their positions from the strip's right edge.
pub(super) fn glyphs(quote: bool) -> Vec<Glyph> {
    let shift = if quote { QUOTE_SHIFT } else { 0 };
    let mut out: Vec<Glyph> = PATTERN
        .iter()
        .map(|g| Glyph {
            x: g.x + shift,
            ..*g
        })
        .collect();
    if quote {
        out.extend(QUOTE_EXTRA);
    }
    out
}

/// `mask` with its color replaced by `rgb` (its alpha kept), scaled to a
/// square of `edge` px.
pub(super) fn tinted(mask: &RgbaImage, rgb: [u8; 3], edge: u32) -> RgbaImage {
    let mut out = if mask.dimensions() == (edge, edge) {
        mask.clone()
    } else {
        image::imageops::resize(mask, edge, edge, image::imageops::FilterType::Triangle)
    };
    for pixel in out.pixels_mut() {
        pixel.0[..3].copy_from_slice(&rgb);
    }
    out
}

/// Tinted copies of one emoji, smallest first.
pub(super) type Copies = [Arc<RenderImage>; 3];

type Key = (PathBuf, [u8; 3]);

thread_local! {
    /// Most recently used last. A pattern is a handful of emoji and
    /// colors, so a short list is plenty.
    static CACHE: RefCell<Vec<(Key, Copies)>> = const { RefCell::new(Vec::new()) };
}

const CACHE_LEN: usize = 16;

/// The three tinted copies of the emoji at `path` in `rgb`, made on first
/// use. `None` when the file cannot be decoded.
pub(super) fn copies(path: &Path, rgb: [u8; 3]) -> Option<Copies> {
    let key: Key = (path.to_path_buf(), rgb);
    let hit = CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let at = cache.iter().position(|(k, _)| *k == key)?;
        let entry = cache.remove(at);
        let found = entry.1.clone();
        cache.push(entry);
        Some(found)
    });
    if hit.is_some() {
        return hit;
    }
    let mask = image::open(path).ok()?.into_rgba8();
    let make = |size: u32| {
        let mut image = tinted(&mask, rgb, size * SCALE);
        // GPUI's sprites are BGRA.
        for pixel in image.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
            image,
        )])))
    };
    let made: Copies = [make(SIZES[0]), make(SIZES[1]), make(SIZES[2])];
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.push((key, made.clone()));
        if cache.len() > CACHE_LEN {
            let (_, old) = cache.remove(0);
            super::image_budget::retire_all(old);
        }
    });
    Some(made)
}

/// The pattern as a layer filling its (clipping, relative) parent.
pub(super) fn layer(copies: &Copies, quote: bool) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .children(glyphs(quote).into_iter().map(|g| {
            let side = SIZES[g.size];
            img(copies[g.size].clone())
                .absolute()
                .top(px(g.y as f32))
                .right(px((g.x - side as i32) as f32))
                .w(px(side as f32))
                .h(px(side as f32))
                .opacity(g.opacity)
        }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{glyphs, tinted};
    use image::{Rgba, RgbaImage};

    #[test]
    fn plain_strips_get_nine_glyphs_and_quotes_get_eleven_shifted_ones() {
        let plain = glyphs(false);
        assert_eq!(plain.len(), 9);
        assert_eq!((plain[0].x, plain[0].y), (28, 4));
        let quote = glyphs(true);
        assert_eq!(quote.len(), 11);
        assert_eq!((quote[0].x, quote[0].y), (40, 4));
        // The two extras sit on the left and are not shifted.
        assert_eq!((quote[9].x, quote[10].x), (4, 0));
    }

    #[test]
    fn every_glyph_names_one_of_the_three_sizes_and_is_faint() {
        for g in glyphs(true) {
            assert!(g.size < 3);
            assert!((0.1..=0.35).contains(&g.opacity));
        }
    }

    #[test]
    fn tinting_keeps_alpha_and_replaces_color() {
        let mut mask = RgbaImage::new(2, 2);
        mask.put_pixel(0, 0, Rgba([255, 255, 255, 255]));
        mask.put_pixel(1, 0, Rgba([10, 20, 30, 128]));
        mask.put_pixel(0, 1, Rgba([0, 0, 0, 0]));
        let out = tinted(&mask, [200, 100, 50], 2);
        assert_eq!(out.get_pixel(0, 0).0, [200, 100, 50, 255]);
        assert_eq!(out.get_pixel(1, 0).0, [200, 100, 50, 128]);
        assert_eq!(out.get_pixel(0, 1).0[3], 0);
    }

    #[test]
    fn tinting_scales_to_the_requested_square() {
        let mask = RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 255]));
        let out = tinted(&mask, [9, 9, 9], 24);
        assert_eq!(out.dimensions(), (24, 24));
        assert_eq!(out.get_pixel(12, 12).0, [9, 9, 9, 255]);
    }
}
