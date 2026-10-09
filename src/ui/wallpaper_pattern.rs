//! Pattern wallpapers: Telegram's `backgroundTypePattern` is a PNG or a TGV
//! (a gzipped subset of SVG) drawn over a fill (tdesktop
//! `ui/chat/chat_theme.cpp`: `PreparePatternImage`, `GenerateBackgroundImage`).
//!
//! The file becomes an alpha mask (resvg for SVG, the `image` crate for PNG),
//! then a square tile whose alpha folds in the pattern intensity. The tile
//! is painted over the fill, scaled to the area's height and repeated across
//! it, centered, exactly like tdesktop.
//!
//! Qt's soft-light blend has no GPUI equivalent, so a normal-mode overlay of
//! black (or white over dark fills, which is when tdesktop inverts the
//! pattern) at the pattern opacity stands in for it. A pattern flagged
//! `is_inverted` (negative intensity) shows the fill only through the
//! pattern, with black everywhere else.

use super::image_budget;
use super::lru::Lru;
use gpui_kit::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Edge of the rasterised tile in pixels. Tiles are drawn at the area's
/// height, which is rarely above this; one size keeps the cache small.
pub(super) const TILE_EDGE: u32 = 512;
/// Rasterised masks and composed tiles kept alive (each is 1 MiB).
const CACHE_TILES: usize = 6;

/// How a pattern's alpha becomes pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct PatternInk {
    /// Overlay color: 0 (black) or 255 (white).
    pub shade: u8,
    /// `is_inverted`: everything the pattern does not cover is black.
    pub inverted: bool,
    /// Pattern intensity, 0-100.
    pub intensity: u8,
}

/// Perceived brightness (HSV value) of the fill, 0.0-1.0.
fn fill_value(colors: &[u32]) -> f32 {
    if colors.is_empty() {
        return 1.0;
    }
    let n = colors.len() as f32;
    let channel = |shift: u32| {
        colors
            .iter()
            .map(|c| ((c >> shift) & 0xff) as f32)
            .sum::<f32>()
            / n
    };
    channel(16).max(channel(8)).max(channel(0)) / 255.0
}

/// tdesktop's `IsPatternInverted`: over a dark fill the (black) pattern is
/// drawn white so it stays visible.
pub(super) fn pattern_ink(fill_colors: &[u32], intensity: i32, is_inverted: bool) -> PatternInk {
    // Telegram may also send the sign in `intensity` (negative = inverted).
    let inverted = is_inverted || intensity < 0;
    let intensity = intensity.unsigned_abs().min(100) as u8;
    let dark_fill = fill_value(fill_colors) <= 0.3;
    PatternInk {
        shade: if !inverted && dark_fill { 255 } else { 0 },
        inverted,
        intensity,
    }
}

/// Alpha of the overlay tile for each pattern coverage value.
fn overlay_alpha(coverage: u8, ink: PatternInk) -> u8 {
    let cover = f32::from(coverage) / 255.0;
    let intensity = f32::from(ink.intensity) / 100.0;
    let alpha = if ink.inverted {
        // Black over the pattern at (1 - intensity), solid black elsewhere.
        1.0 - cover * intensity
    } else {
        cover * intensity
    };
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// BGRA, premultiplied, for GPUI.
pub(super) fn compose_tile(mask: &[u8], ink: PatternInk) -> Vec<u8> {
    let mut out = Vec::with_capacity(mask.len() * 4);
    for &coverage in mask {
        let alpha = overlay_alpha(coverage, ink);
        let channel = (u16::from(ink.shade) * u16::from(alpha) / 255) as u8;
        out.extend_from_slice(&[channel, channel, channel, alpha]);
    }
    out
}

/// The pattern's coverage on a square `edge` x `edge` canvas (the pattern is
/// scaled to fit, centered), from a PNG or a (gzipped) SVG.
pub(super) fn render_mask(data: &[u8], edge: u32) -> Option<Vec<u8>> {
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        let image = image::load_from_memory(data).ok()?.to_rgba8();
        let (w, h) = image.dimensions();
        let scale = (edge as f32 / w as f32).min(edge as f32 / h as f32);
        let (nw, nh) = (
            ((w as f32 * scale).round() as u32).max(1),
            ((h as f32 * scale).round() as u32).max(1),
        );
        let resized =
            image::imageops::resize(&image, nw, nh, image::imageops::FilterType::Triangle);
        let mut mask = vec![0u8; (edge * edge) as usize];
        let (ox, oy) = ((edge - nw.min(edge)) / 2, (edge - nh.min(edge)) / 2);
        for (x, y, px) in resized.enumerate_pixels() {
            if x < edge && y < edge && ox + x < edge && oy + y < edge {
                mask[((oy + y) * edge + ox + x) as usize] = px.0[3];
            }
        }
        return Some(mask);
    }
    // `Tree::from_data` unzips SVGZ / TGV itself.
    let tree = resvg::usvg::Tree::from_data(data, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return None;
    }
    let scale = (edge as f32 / size.width()).min(edge as f32 / size.height());
    let mut pixmap = resvg::tiny_skia::Pixmap::new(edge, edge)?;
    let (dx, dy) = (
        (edge as f32 - size.width() * scale) / 2.0,
        (edge as f32 - size.height() * scale) / 2.0,
    );
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    Some(pixmap.pixels().iter().map(|p| p.alpha()).collect())
}

thread_local! {
    static MASKS: RefCell<Lru<PathBuf, Option<Arc<Vec<u8>>>>> =
        RefCell::new(Lru::new(CACHE_TILES));
    static TILES: RefCell<Lru<(PathBuf, PatternInk), Arc<RenderImage>>> =
        RefCell::new(Lru::new(CACHE_TILES));
}

fn mask_for(path: &Path) -> Option<Arc<Vec<u8>>> {
    if let Some(hit) = MASKS.with(|m| m.borrow_mut().get(&path.to_path_buf())) {
        return hit;
    }
    let mask = std::fs::read(path)
        .ok()
        .and_then(|data| render_mask(&data, TILE_EDGE))
        .map(Arc::new);
    MASKS.with(|m| m.borrow_mut().insert(path.to_path_buf(), mask.clone()));
    mask
}

/// The tile for a pattern file and ink; rasterised once per (file, ink) and
/// kept in a bounded cache. `None` while the file is missing or unreadable.
pub(super) fn tile_for(path: &Path, ink: PatternInk) -> Option<Arc<RenderImage>> {
    let key = (path.to_path_buf(), ink);
    if let Some(hit) = TILES.with(|t| t.borrow_mut().get(&key)) {
        return Some(hit);
    }
    let mask = mask_for(path)?;
    let bgra = compose_tile(&mask, ink);
    let buffer = image::RgbaImage::from_raw(TILE_EDGE, TILE_EDGE, bgra)?;
    let render = Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buffer),
    ])));
    TILES.with(|t| {
        if let Some(old) = t.borrow_mut().insert(key, render.clone()) {
            image_budget::retire_all([old]);
        }
    });
    Some(render)
}

/// Tile columns across `width` for tiles `tile` wide: odd, so one tile sits
/// in the center and the rest mirror around it (tdesktop `cols`).
pub(super) fn tile_columns(width: f32, tile: f32) -> usize {
    if tile <= 0.0 {
        return 1;
    }
    let cx = (width / tile).ceil().max(1.0) as usize;
    (cx / 2) * 2 + 1
}

/// The element that paints the pattern over whatever is behind it.
pub(super) fn pattern_layer(path: PathBuf, ink: PatternInk) -> impl IntoElement {
    canvas(
        move |_, _, _| tile_for(&path, ink),
        move |bounds, tile, window, _| {
            let Some(tile) = tile else { return };
            let edge = bounds.size.height;
            if edge <= px(1.) {
                return;
            }
            let columns = tile_columns(f32::from(bounds.size.width), f32::from(edge));
            let row_width = edge * columns as f32;
            let left = bounds.origin.x + (bounds.size.width - row_width) / 2.0;
            // Rows: the tile is square and as tall as the area, so one row.
            for column in 0..columns {
                let origin = point(left + edge * column as f32, bounds.origin.y);
                let _ = window.paint_image(
                    bounds,
                    Bounds::new(origin, size(edge, edge)),
                    Corners::default(),
                    tile.clone(),
                    0,
                    false,
                );
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::{PatternInk, compose_tile, pattern_ink, render_mask, tile_columns};

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 10 10"><rect x="0" y="0" width="5" height="10"/></svg>"#;

    #[test]
    fn svg_patterns_rasterise_into_a_centered_coverage_mask() {
        let mask = render_mask(SVG.as_bytes(), 20).expect("mask");
        assert_eq!(mask.len(), 400);
        // Left half covered, right half clear.
        assert_eq!(mask[5], 255);
        assert_eq!(mask[15], 0);
    }

    #[test]
    fn gzipped_svg_is_the_tgv_format() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(SVG.as_bytes()).unwrap();
        let tgv = gz.finish().unwrap();
        assert_eq!(render_mask(&tgv, 20), render_mask(SVG.as_bytes(), 20));
    }

    #[test]
    fn png_patterns_use_their_alpha_channel() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_fn(4, 4, |x, _| {
            image::Rgba([0, 0, 0, if x < 2 { 255 } else { 0 }])
        });
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let mask = render_mask(&png, 8).expect("mask");
        assert_eq!(mask[0], 255);
        assert_eq!(mask[7], 0);
    }

    #[test]
    fn garbage_is_not_a_pattern() {
        assert!(render_mask(b"not an image", 8).is_none());
    }

    #[test]
    fn dark_fills_get_a_white_pattern_and_negative_intensity_inverts() {
        let light = pattern_ink(&[0xffffff], 40, false);
        assert_eq!(
            (light.shade, light.inverted, light.intensity),
            (0, false, 40)
        );
        let dark = pattern_ink(&[0x101010, 0x202020], 40, false);
        assert_eq!(dark.shade, 255);
        let flagged = pattern_ink(&[0x101010], 30, true);
        assert!(flagged.inverted);
        assert_eq!(flagged.shade, 0, "inverted patterns stay black");
        let signed = pattern_ink(&[0x101010], -30, false);
        assert!(signed.inverted);
        assert_eq!(signed.intensity, 30);
    }

    #[test]
    fn tiles_fold_intensity_into_alpha() {
        let ink = PatternInk {
            shade: 0,
            inverted: false,
            intensity: 50,
        };
        let tile = compose_tile(&[255, 0], ink);
        assert_eq!(&tile[..4], &[0, 0, 0, 128]);
        assert_eq!(&tile[4..], &[0, 0, 0, 0]);
        let white = compose_tile(
            &[255],
            PatternInk {
                shade: 255,
                inverted: false,
                intensity: 100,
            },
        );
        assert_eq!(white, vec![255, 255, 255, 255], "premultiplied");
        // Inverted: black where there is no pattern, lighter where there is.
        let inverted = compose_tile(
            &[255, 0],
            PatternInk {
                shade: 0,
                inverted: true,
                intensity: 40,
            },
        );
        assert_eq!(inverted[3], 153);
        assert_eq!(inverted[7], 255);
    }

    #[test]
    fn columns_are_odd_and_cover_the_width() {
        assert_eq!(tile_columns(500.0, 500.0), 1);
        assert_eq!(tile_columns(900.0, 500.0), 3);
        assert_eq!(tile_columns(1600.0, 500.0), 5);
        assert_eq!(tile_columns(100.0, 0.0), 1);
    }
}
