//! Chat backgrounds as pixels: the fills, pattern composition, blur and
//! placement Telegram Desktop uses (`ui/chat/chat_theme.cpp`,
//! `lib_ui/ui/image/image_prepare.cpp`).
//!
//! Everything here is a pure function over BGRA buffers (the layout GPUI's
//! sprite atlas takes), so the UI renders a background once per area size on
//! a worker thread and paints the result as one image, and the core tests
//! cover the maths on every platform. Decoding photos and rasterising SVG
//! patterns stay in the UI crate, which has the image libraries.

mod soft_light;

pub use soft_light::{MixTable, PatternMix, pattern_is_light};

use crate::profile_photo_gradient::small_complex_gradient;
use crate::telegram::envelope::BackgroundFill;

/// An opaque picture, BGRA, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

impl Raster {
    /// Opaque black, `width` x `height`.
    pub fn black(width: u32, height: u32) -> Self {
        let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
        for px in bgra.as_chunks_mut::<4>().0.iter_mut() {
            px[3] = 255;
        }
        Self {
            width,
            height,
            bgra,
        }
    }

    /// From RGBA bytes (alpha dropped: wallpapers are opaque, tdesktop
    /// reads them with `forceOpaque`).
    pub fn from_rgba(width: u32, height: u32, mut rgba: Vec<u8>) -> Self {
        for px in rgba.as_chunks_mut::<4>().0.iter_mut() {
            px.swap(0, 2);
            px[3] = 255;
        }
        Self {
            width,
            height,
            bgra: rgba,
        }
    }

    /// The pixel at `x`, `y` as `[b, g, r, a]`.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y as usize) * (self.width as usize) + x as usize) * 4;
        [
            self.bgra[at],
            self.bgra[at + 1],
            self.bgra[at + 2],
            self.bgra[at + 3],
        ]
    }

    /// Bytes this picture holds.
    pub fn byte_size(&self) -> usize {
        self.bgra.len()
    }
}

/// A coverage mask (0-255), `width` x `height`: a pattern's alpha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    pub width: u32,
    pub height: u32,
    pub alpha: Vec<u8>,
}

fn channels(color: u32) -> [f32; 3] {
    [
        ((color >> 16) & 0xff) as f32,
        ((color >> 8) & 0xff) as f32,
        (color & 0xff) as f32,
    ]
}

/// Where a two-colour gradient starts and ends in a unit square for
/// Telegram's `rotation_angle` (lib_ui `GenerateLinearGradient`: corner to
/// corner on the diagonals, then stretched to the area).
fn linear_ends(rotation: i32) -> ((f32, f32), (f32, f32)) {
    match rotation.clamp(0, 315) / 45 {
        0 => ((0.0, 0.0), (0.0, 1.0)),
        1 => ((1.0, 0.0), (0.0, 1.0)),
        2 => ((1.0, 0.0), (0.0, 0.0)),
        3 => ((1.0, 1.0), (0.0, 0.0)),
        4 => ((0.0, 1.0), (0.0, 0.0)),
        5 => ((0.0, 1.0), (1.0, 0.0)),
        6 => ((0.0, 0.0), (1.0, 0.0)),
        _ => ((0.0, 0.0), (1.0, 1.0)),
    }
}

/// How a fill colours a point: `u`, `v` run 0-1 across the area.
#[derive(Clone, Debug, PartialEq)]
pub enum FillSampler {
    Solid([f32; 3]),
    Linear {
        start: (f32, f32),
        delta: (f32, f32),
        inv_len2: f32,
        stops: Vec<[f32; 3]>,
    },
    /// The 64 x 64 freeform picture, sampled bilinearly (Qt's smooth
    /// scaling of `GenerateComplexGradient`).
    Freeform {
        side: u32,
        rgb: Vec<[f32; 3]>,
    },
}

impl FillSampler {
    /// tdesktop `Images::GenerateGradient`: one colour fills, two blend
    /// linearly along the rotation, three or four make the freeform
    /// gradient (rotation 0, the wallpaper's resting phase).
    pub fn new(fill: &BackgroundFill) -> Self {
        Self::rotated(fill, 0)
    }

    /// [`Self::new`] after `rotation` steps of tdesktop's rotation on
    /// send. Only freeform gradients rotate.
    pub fn rotated(fill: &BackgroundFill, rotation: u16) -> Self {
        match fill {
            BackgroundFill::Solid(color) => Self::Solid(channels(*color)),
            BackgroundFill::Gradient { top, bottom, angle } => {
                Self::linear(&[*top, *bottom], *angle)
            }
            BackgroundFill::Freeform(colors) => match colors.len() {
                0 => Self::Solid([0.0; 3]),
                1 => Self::Solid(channels(colors[0])),
                2 => Self::linear(colors, 0),
                _ => {
                    let (phase, progress) = freeform_phase(rotation);
                    let small = small_complex_gradient(colors, phase, progress);
                    let rgb = small
                        .rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|p| [f32::from(p[0]), f32::from(p[1]), f32::from(p[2])])
                        .collect();
                    Self::Freeform {
                        side: small.side,
                        rgb,
                    }
                }
            },
        }
    }

    /// A vertical multi-stop gradient, top to bottom (tdesktop's outgoing
    /// bubble fill, `PrepareBubblesBackground`).
    pub fn vertical(colors: &[u32]) -> Self {
        match colors {
            [] => Self::Solid([0.0; 3]),
            [one] => Self::Solid(channels(*one)),
            _ => Self::linear(colors, 0),
        }
    }

    fn linear(colors: &[u32], rotation: i32) -> Self {
        let (start, end) = linear_ends(rotation);
        let delta = (end.0 - start.0, end.1 - start.1);
        let len2 = delta.0 * delta.0 + delta.1 * delta.1;
        Self::Linear {
            start,
            delta,
            inv_len2: if len2 > 0.0 { 1.0 / len2 } else { 0.0 },
            stops: colors.iter().map(|c| channels(*c)).collect(),
        }
    }

    /// The colour at `u`, `v`, channels 0-255 as `[r, g, b]`.
    pub fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        match self {
            Self::Solid(rgb) => *rgb,
            Self::Linear {
                start,
                delta,
                inv_len2,
                stops,
            } => {
                let t = (((u - start.0) * delta.0 + (v - start.1) * delta.1) * inv_len2)
                    .clamp(0.0, 1.0);
                along(stops, t)
            }
            Self::Freeform { side, rgb } => {
                let last = (*side - 1) as f32;
                let fx = (u * *side as f32 - 0.5).clamp(0.0, last);
                let fy = (v * *side as f32 - 0.5).clamp(0.0, last);
                let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
                let (x1, y1) = ((x0 + 1).min(*side - 1), (y0 + 1).min(*side - 1));
                let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                let at = |x: u32, y: u32| rgb[(y * *side + x) as usize];
                let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
                std::array::from_fn(|i| {
                    let top = a[i] + (b[i] - a[i]) * tx;
                    let bottom = c[i] + (d[i] - c[i]) * tx;
                    top + (bottom - top) * ty
                })
            }
        }
    }

    /// Fill `row` (BGRA, `width` pixels) with row `y` of a `height`-tall
    /// area.
    fn fill_row(&self, row: &mut [u8], y: u32, height: u32) {
        let width = (row.len() / 4) as u32;
        let v = (y as f32 + 0.5) / height.max(1) as f32;
        for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let u = (x as f32 + 0.5) / width.max(1) as f32;
            let [r, g, b] = self.sample(u, v);
            px[0] = b.round() as u8;
            px[1] = g.round() as u8;
            px[2] = r.round() as u8;
            px[3] = 255;
        }
    }
}

/// One send's step of the freeform rotation, in tdesktop's doubled units
/// (`ChatTheme::generateNextBackgroundRotation`, `kAddRotationDoubled`).
pub const ROTATION_STEP: u16 = 720 - 45;

/// The rotation after one more send.
pub fn next_rotation(rotation: u16) -> u16 {
    (rotation + ROTATION_STEP) % 720
}

/// The gradient phase (rotation / 45) and point progress for a doubled
/// rotation (tdesktop `ComputeRealRotation` / `ComputeRealProgress`): odd
/// steps sit halfway between two phases.
pub fn freeform_phase(rotation: u16) -> (usize, f32) {
    let doubled = i32::from(rotation % 720);
    let (degrees, progress) = if doubled % 2 == 1 {
        (((doubled - 45) / 2).rem_euclid(360), 0.5)
    } else {
        ((doubled / 2) % 360, 1.0)
    };
    ((degrees.clamp(0, 315) / 45) as usize, progress)
}

/// The colour `t` (0-1) along evenly spaced `stops`.
fn along(stops: &[[f32; 3]], t: f32) -> [f32; 3] {
    match stops {
        [] => [0.0; 3],
        [one] => *one,
        _ => {
            let at = t * (stops.len() - 1) as f32;
            let index = (at.floor() as usize).min(stops.len() - 2);
            let frac = at - index as f32;
            let (a, b) = (stops[index], stops[index + 1]);
            std::array::from_fn(|i| a[i] + (b[i] - a[i]) * frac)
        }
    }
}

/// The colour at `t` (0-1) of a vertical gradient over `colors`, as
/// 0xRRGGBB.
pub fn vertical_color_at(colors: &[u32], t: f32) -> u32 {
    let stops: Vec<[f32; 3]> = colors.iter().map(|c| channels(*c)).collect();
    let [r, g, b] = along(&stops, t.clamp(0.0, 1.0));
    ((r.round() as u32) << 16) | ((g.round() as u32) << 8) | b.round() as u32
}

/// The fill alone over a `width` x `height` area.
pub fn compose_fill(fill: &FillSampler, width: u32, height: u32) -> Raster {
    let mut out = Raster::black(width, height);
    let stride = width as usize * 4;
    if stride == 0 {
        return out;
    }
    for (y, row) in out.bgra.chunks_exact_mut(stride).enumerate() {
        fill.fill_row(row, y as u32, height);
    }
    out
}

/// The size a pattern tile is drawn at for an area `area_height` tall:
/// scaled to fit an `area_height` square, keeping its aspect (tdesktop
/// scales the prepared pattern to the area height, `KeepAspectRatio`).
pub fn pattern_tile_size(pattern_width: f32, pattern_height: f32, area_height: u32) -> (u32, u32) {
    if pattern_width <= 0.0 || pattern_height <= 0.0 || area_height == 0 {
        return (1, 1);
    }
    let edge = area_height as f32;
    let scale = (edge / pattern_width).min(edge / pattern_height);
    (
        ((pattern_width * scale).round() as u32).max(1),
        ((pattern_height * scale).round() as u32).max(1),
    )
}

/// Columns of tiles `tile` wide across `width`: odd, so one tile sits in
/// the centre and the others mirror around it (tdesktop `cols`).
pub fn pattern_columns(width: u32, tile: u32) -> u32 {
    if tile == 0 {
        return 1;
    }
    let cx = width.div_ceil(tile).max(1);
    (cx / 2) * 2 + 1
}

/// A pattern over a fill (tdesktop `CacheBackground` for patterns): the
/// tile repeats in odd columns centred across the width and in rows down
/// the height, mixed into the fill by `table`.
pub fn compose_pattern(
    fill: &FillSampler,
    table: &MixTable,
    tile: &Mask,
    width: u32,
    height: u32,
) -> Raster {
    let mut out = compose_fill(fill, width, height);
    let (tw, th) = (tile.width.max(1), tile.height.max(1));
    if tile.alpha.len() < (tw as usize) * (th as usize) {
        return out;
    }
    let cols = pattern_columns(width, tw);
    // tdesktop's `xshift` (integer division, so it rounds toward zero).
    let shift = (i64::from(width) - i64::from(cols) * i64::from(tw)) / 2;
    let stride = width as usize * 4;
    if stride == 0 {
        return out;
    }
    let offset = (-shift).rem_euclid(i64::from(tw)) as u32;
    for (y, row) in out.bgra.chunks_exact_mut(stride).enumerate() {
        let ty = (y as u32 % th) as usize;
        let tile_row = &tile.alpha[ty * tw as usize..(ty + 1) * tw as usize];
        let mut tx = offset;
        for px in row.as_chunks_mut::<4>().0.iter_mut() {
            let coverage = tile_row[tx as usize];
            px[0] = table.mix(px[0], coverage);
            px[1] = table.mix(px[1], coverage);
            px[2] = table.mix(px[2], coverage);
            tx += 1;
            if tx == tw {
                tx = 0;
            }
        }
    }
    out
}

/// A rectangle in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// The part of an image that covers an area and where it lands, centred
/// (tdesktop `ComputeChatBackgroundRects`). Returns `(from, to)`: `from`
/// in image pixels, `to` in area pixels.
pub fn cover_rects(
    area_width: u32,
    area_height: u32,
    image_width: u32,
    image_height: u32,
) -> (PixelRect, PixelRect) {
    let (fw, fh) = (f64::from(area_width), f64::from(area_height));
    let (iw, ih) = (
        i64::from(image_width.max(1)),
        i64::from(image_height.max(1)),
    );
    if iw * i64::from(area_height) > ih * i64::from(area_width) {
        let pxsize = fh / ih as f64;
        let mut take = (fw / pxsize).ceil() as i64;
        if take > iw {
            take = iw;
        } else if iw % 2 != take % 2 {
            take += 1;
        }
        (
            PixelRect {
                x: ((iw - take) / 2) as i32,
                y: 0,
                width: take as u32,
                height: ih as u32,
            },
            PixelRect {
                x: ((fw - take as f64 * pxsize) / 2.0) as i32,
                y: 0,
                width: (take as f64 * pxsize).ceil() as u32,
                height: area_height,
            },
        )
    } else {
        let pxsize = fw / iw as f64;
        let mut take = (fh / pxsize).ceil() as i64;
        if take > ih {
            take = ih;
        } else if ih % 2 != take % 2 {
            take += 1;
        }
        (
            PixelRect {
                x: 0,
                y: ((ih - take) / 2) as i32,
                width: iw as u32,
                height: take as u32,
            },
            PixelRect {
                x: 0,
                y: ((fh - take as f64 * pxsize) / 2.0) as i32,
                width: area_width,
                height: (take as f64 * pxsize).ceil() as u32,
            },
        )
    }
}

/// The largest side a wallpaper photo keeps (tdesktop `kMaxSize`).
pub const MAX_IMAGE_SIDE: u32 = 2960;
/// Blurred photos are first fitted in this square (tdesktop
/// `PrepareBlurredBackground`), then blurred with [`BLUR_RADIUS`].
pub const BLUR_SIDE: u32 = 900;
pub const BLUR_RADIUS: u32 = 24;

/// The centre strip of an extremely long image, at most 40:1
/// (tdesktop `PreprocessBackgroundImage`); `None` keeps it whole.
pub fn long_image_crop(width: u32, height: u32) -> Option<PixelRect> {
    if width > height.saturating_mul(40) {
        let w = height * 40;
        Some(PixelRect {
            x: ((width - w) / 2) as i32,
            y: 0,
            width: w,
            height,
        })
    } else if height > width.saturating_mul(40) {
        let h = width * 40;
        Some(PixelRect {
            x: 0,
            y: ((height - h) / 2) as i32,
            width,
            height: h,
        })
    } else {
        None
    }
}

/// `width` x `height` scaled down (never up) to fit a `side` square.
pub fn fit_within(width: u32, height: u32, side: u32) -> (u32, u32) {
    if width <= side && height <= side {
        return (width, height);
    }
    let scale = (side as f64 / f64::from(width.max(1))).min(side as f64 / f64::from(height.max(1)));
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

/// Blur in place with a triangle kernel of `radius` (two box passes of
/// half the radius per axis): the weights of the stack blur lib_ui's
/// `BlurLargeImage` uses. Edges repeat the border pixel.
pub fn blur(raster: &mut Raster, radius: u32) {
    let half = radius / 2;
    if half == 0 || raster.width == 0 || raster.height == 0 {
        return;
    }
    let (w, h) = (raster.width as usize, raster.height as usize);
    let mut line = Vec::new();
    for _ in 0..2 {
        for y in 0..h {
            box_line(&mut raster.bgra, y * w * 4, 4, w, half as usize, &mut line);
        }
        for x in 0..w {
            box_line(&mut raster.bgra, x * 4, w * 4, h, half as usize, &mut line);
        }
    }
}

/// One box pass over `count` pixels starting at `start`, `step` bytes
/// apart.
fn box_line(
    data: &mut [u8],
    start: usize,
    step: usize,
    count: usize,
    radius: usize,
    line: &mut Vec<[u32; 3]>,
) {
    line.clear();
    line.extend((0..count).map(|i| {
        let at = start + i * step;
        [
            u32::from(data[at]),
            u32::from(data[at + 1]),
            u32::from(data[at + 2]),
        ]
    }));
    let span = (radius * 2 + 1) as u32;
    let get = |i: isize| line[i.clamp(0, count as isize - 1) as usize];
    let mut sum = [0u32; 3];
    for i in -(radius as isize)..=(radius as isize) {
        let p = get(i);
        for c in 0..3 {
            sum[c] += p[c];
        }
    }
    for i in 0..count {
        let at = start + i * step;
        for c in 0..3 {
            data[at + c] = ((sum[c] + span / 2) / span) as u8;
        }
        let (out, inn) = (
            get(i as isize - radius as isize),
            get(i as isize + radius as isize + 1),
        );
        for c in 0..3 {
            sum[c] = sum[c] + inn[c] - out[c];
        }
    }
}

/// `source` repeated from the top-left corner over `width` x `height`, at
/// one image pixel per device pixel (tdesktop's "Tile background").
pub fn tile(source: &Raster, width: u32, height: u32) -> Raster {
    let mut out = Raster::black(width, height);
    let (sw, sh) = (source.width as usize, source.height as usize);
    if sw == 0 || sh == 0 {
        return out;
    }
    let stride = width as usize * 4;
    for (y, row) in out.bgra.chunks_exact_mut(stride.max(4)).enumerate() {
        let src_row = &source.bgra[(y % sh) * sw * 4..(y % sh + 1) * sw * 4];
        for (chunk, src) in row.chunks_mut(sw * 4).zip(std::iter::repeat(src_row)) {
            chunk.copy_from_slice(&src[..chunk.len()]);
        }
    }
    out
}

/// Copy `source` into `target` with its top-left corner at `x`, `y`
/// (clipped).
pub fn blit(target: &mut Raster, source: &Raster, x: i32, y: i32) {
    for sy in 0..source.height as i32 {
        let ty = y + sy;
        if ty < 0 || ty >= target.height as i32 {
            continue;
        }
        let sx0 = (-x).max(0);
        let sx1 = (source.width as i32).min(target.width as i32 - x);
        if sx1 <= sx0 {
            continue;
        }
        let src = ((sy as usize) * source.width as usize + sx0 as usize) * 4;
        let dst = ((ty as usize) * target.width as usize + (x + sx0) as usize) * 4;
        let len = (sx1 - sx0) as usize * 4;
        target.bgra[dst..dst + len].copy_from_slice(&source.bgra[src..src + len]);
    }
}

#[cfg(test)]
mod tests;
