//! Pure conversions for pushed frames: I420 turned or mirrored,
//! interleaved into NV12 (macOS pixel buffers, Windows surface frames),
//! expanded to BGRA (GPUI images on Linux, and the fallback elsewhere),
//! shrunk and blurred for the paused tile, and back from RGBA for
//! pictures Quill makes itself (your shared screen's preview, fixtures).
//!
//! The colour math is the one the engine's RGBA conversion used before
//! every picture became a live feed: BT.601 with full-range luma, which
//! is also the matrix GPUI's macOS surface shader applies to a full-range
//! NV12 buffer and the one Windows NV12 live frames carry, so every path
//! shows the same colours.
#![deny(clippy::indexing_slicing)]

use super::LiveRotation;
use crate::wallpaper_raster::{Raster, blur};

/// Bytes of an I420 frame: Y `w × h`, U and V `⌈w/2⌉ × ⌈h/2⌉`. Not
/// checked: with `u16` sides the sum passes `usize` only on 32-bit
/// targets, which Quill does not build for.
pub fn i420_size(width: u16, height: u16) -> usize {
    let (w, h) = (usize::from(width), usize::from(height));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    w * h + 2 * cw * ch
}

/// Borrowed I420 planes of one frame.
#[derive(Clone, Copy, Debug)]
pub struct I420<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub width: usize,
    pub height: usize,
}

impl<'a> I420<'a> {
    /// Split `planes`; `None` unless it holds exactly one frame.
    pub fn new(planes: &'a [u8], width: u16, height: u16) -> Option<Self> {
        if width == 0 || height == 0 || planes.len() != i420_size(width, height) {
            return None;
        }
        let (w, h) = (usize::from(width), usize::from(height));
        let chroma = w.div_ceil(2) * h.div_ceil(2);
        let (y, rest) = planes.split_at_checked(w * h)?;
        let (u, v) = rest.split_at_checked(chroma)?;
        Some(Self {
            y,
            u,
            v,
            width: w,
            height: h,
        })
    }

    /// The chroma planes' width and height.
    pub fn chroma_size(&self) -> (usize, usize) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }
}

/// Where the pixel at `(x, y)` of a `w × h` plane lands once turned, as
/// the engine turns RGBA (`rotate_rgba`). WebRTC's rotation is the
/// clockwise turn that shows the frame upright (libyuv's `kRotate90`;
/// tdesktop draws it with `QTransform::rotate`), so 90 moves the left
/// column to the top row.
fn turned(x: usize, y: usize, w: usize, h: usize, rotation: LiveRotation) -> (usize, usize) {
    match rotation {
        LiveRotation::R90 => (h.saturating_sub(1 + y), x),
        LiveRotation::R180 => (w.saturating_sub(1 + x), h.saturating_sub(1 + y)),
        LiveRotation::R270 => (y, w.saturating_sub(1 + x)),
        LiveRotation::None => (x, y),
    }
}

const TURN_BLOCK: usize = 8;

fn transpose_8x8_bytes([a, b, c, d, e, f, g, h]: &mut [u64; TURN_BLOCK]) {
    fn swap(low: &mut u64, high: &mut u64, shift: u32, mask: u64) {
        let moved = ((*low >> shift) ^ *high) & mask;
        *high ^= moved;
        *low ^= moved << shift;
    }
    const BYTES: u64 = 0x00ff_00ff_00ff_00ff;
    const PAIRS: u64 = 0x0000_ffff_0000_ffff;
    const QUADS: u64 = 0x0000_0000_ffff_ffff;
    swap(a, b, 8, BYTES);
    swap(c, d, 8, BYTES);
    swap(e, f, 8, BYTES);
    swap(g, h, 8, BYTES);
    swap(a, c, 16, PAIRS);
    swap(b, d, 16, PAIRS);
    swap(e, g, 16, PAIRS);
    swap(f, h, 16, PAIRS);
    swap(a, e, 32, QUADS);
    swap(b, f, 32, QUADS);
    swap(c, g, 32, QUADS);
    swap(d, h, 32, QUADS);
}

fn rotate_quarter(src: &[u8], w: usize, h: usize, rotation: LiveRotation, dst: &mut [u8]) {
    let clockwise = rotation == LiveRotation::R90;
    let (across, down) = (w - w % TURN_BLOCK, h - h % TURN_BLOCK);
    for top in (0..down).step_by(TURN_BLOCK) {
        for left in (0..across).step_by(TURN_BLOCK) {
            let mut block = [0u64; TURN_BLOCK];
            for (at, word) in block.iter_mut().enumerate() {
                let row = top + if clockwise { TURN_BLOCK - 1 - at } else { at };
                let start = row * w + left;
                if let Some((bytes, _)) = src
                    .get(start..)
                    .and_then(|rest| rest.split_first_chunk::<TURN_BLOCK>())
                {
                    *word = u64::from_le_bytes(*bytes);
                }
            }
            transpose_8x8_bytes(&mut block);
            for (at, word) in block.iter().enumerate() {
                let (row, column) = if clockwise {
                    (left + at, h - TURN_BLOCK - top)
                } else {
                    (w - 1 - left - at, top)
                };
                let start = row * h + column;
                if let Some((out, _)) = dst
                    .get_mut(start..)
                    .and_then(|rest| rest.split_first_chunk_mut::<TURN_BLOCK>())
                {
                    *out = word.to_le_bytes();
                }
            }
        }
    }
    let mut turn_pixel = |x: usize, y: usize| {
        let (dx, dy) = turned(x, y, w, h, rotation);
        if let (Some(value), Some(out)) = (src.get(y * w + x), dst.get_mut(dy * h + dx)) {
            *out = *value;
        }
    };
    for y in 0..down {
        for x in across..w {
            turn_pixel(x, y);
        }
    }
    for y in down..h {
        for x in 0..w {
            turn_pixel(x, y);
        }
    }
}

fn rotate_plane(src: &[u8], w: usize, h: usize, rotation: LiveRotation, dst: &mut [u8]) {
    match rotation {
        LiveRotation::None => {}
        LiveRotation::R90 | LiveRotation::R270 => rotate_quarter(src, w, h, rotation, dst),
        LiveRotation::R180 => {
            let rows = src.chunks_exact(w.max(1)).take(h).rev();
            for (out, row) in dst.chunks_exact_mut(w.max(1)).zip(rows) {
                out.copy_from_slice(row);
                out.reverse();
            }
        }
    }
}

/// The frame turned by `rotation` into `out` (whatever it held goes), as
/// I420 planes, and the turned width and height. `None` when there is
/// nothing to turn (or the size does not fit a frame). Odd sides keep their
/// chroma grid, so they are exact only for even sizes (which is what WebRTC
/// sends).
pub fn rotate_planes(src: &I420, rotation: LiveRotation, out: &mut Vec<u8>) -> Option<(u16, u16)> {
    if rotation == LiveRotation::None {
        return None;
    }
    let (w, h) = (src.width, src.height);
    if w == 0 || h == 0 {
        return None;
    }
    let (cw, ch) = src.chroma_size();
    let (dw, dh) = if rotation.swaps_sides() {
        (h, w)
    } else {
        (w, h)
    };
    let (out_w, out_h) = (u16::try_from(dw).ok()?, u16::try_from(dh).ok()?);
    out.resize(i420_size(out_w, out_h), 0);
    let (dcw, dch) = (dw.div_ceil(2), dh.div_ceil(2));
    let (y_out, rest) = out.split_at_mut_checked(dw * dh)?;
    let (u_out, v_out) = rest.split_at_mut_checked(dcw * dch)?;
    rotate_plane(src.y, w, h, rotation, y_out);
    rotate_plane(src.u, cw, ch, rotation, u_out);
    rotate_plane(src.v, cw, ch, rotation, v_out);
    Some((out_w, out_h))
}

/// Mirror an I420 frame left to right in place (your camera preview, as
/// tdesktop shows it). `false`, with nothing changed, unless `planes`
/// holds exactly one `width × height` frame. Odd widths keep their chroma
/// grid, as [`rotate_planes`] does.
pub fn mirror_planes(planes: &mut [u8], width: u16, height: u16) -> bool {
    if width == 0 || height == 0 || planes.len() != i420_size(width, height) {
        return false;
    }
    let (w, h) = (usize::from(width), usize::from(height));
    let cw = w.div_ceil(2);
    let Some((y, chroma)) = planes.split_at_mut_checked(w * h) else {
        return false;
    };
    for row in y.chunks_exact_mut(w) {
        row.reverse();
    }
    for row in chroma.chunks_exact_mut(cw) {
        row.reverse();
    }
    true
}

/// Mirror a BGRA raster left to right in place (the paused picture of a
/// mirrored feed).
pub fn mirror_raster(raster: &mut Raster) {
    let Ok(width) = usize::try_from(raster.width) else {
        return;
    };
    if width == 0 {
        return;
    }
    for row in raster.bgra.chunks_exact_mut(width * 4) {
        let (pixels, _) = row.as_chunks_mut::<4>();
        pixels.reverse();
    }
}

/// Copy the Y plane and interleave U and V into a bi-planar (NV12)
/// destination with the given row strides. `false` (nothing written past
/// a bound) when a destination is too small.
pub fn i420_to_nv12(
    src: &I420,
    y_dst: &mut [u8],
    y_stride: usize,
    uv_dst: &mut [u8],
    uv_stride: usize,
) -> bool {
    let (w, h) = (src.width, src.height);
    let (cw, ch) = src.chroma_size();
    if w == 0 || h == 0 || y_stride < w || uv_stride < cw * 2 {
        return false;
    }
    let y_needed = y_stride.saturating_mul(h - 1).saturating_add(w);
    let uv_needed = uv_stride.saturating_mul(ch - 1).saturating_add(cw * 2);
    if y_dst.len() < y_needed || uv_dst.len() < uv_needed {
        return false;
    }
    for (row, out) in src.y.chunks_exact(w).zip(y_dst.chunks_mut(y_stride)) {
        let Some(out) = out.get_mut(..w) else {
            return false;
        };
        out.copy_from_slice(row);
    }
    let rows = src.u.chunks_exact(cw).zip(src.v.chunks_exact(cw));
    for ((u_row, v_row), out) in rows.zip(uv_dst.chunks_mut(uv_stride)) {
        let Some(out) = out.get_mut(..cw * 2) else {
            return false;
        };
        let (pairs, _) = out.as_chunks_mut::<2>();
        for (pair, (u, v)) in pairs.iter_mut().zip(u_row.iter().zip(v_row)) {
            *pair = [*u, *v];
        }
    }
    true
}

/// Bytes of a tightly packed NV12 frame (Y `w × h`, then Cb and Cr
/// interleaved, `w × h / 2`): what a GPUI NV12 surface frame holds.
/// `None` for a zero or odd side (NV12 surfaces need even sides) or a
/// size that overflows.
pub fn nv12_packed_len(width: usize, height: usize) -> Option<usize> {
    if width == 0 || height == 0 || !width.is_multiple_of(2) || !height.is_multiple_of(2) {
        return None;
    }
    let luma = width.checked_mul(height)?;
    luma.checked_add(luma / 2)
}

/// The size a `width × height` frame packs into as NV12: an odd side
/// loses its last column or row (NV12 surfaces need even sides; the
/// dropped pixels are the only ones the last chroma sample covered, so
/// the rest stays exact). `None` below 2 × 2.
pub fn nv12_packed_size(width: usize, height: usize) -> Option<(usize, usize)> {
    let (w, h) = (width & !1, height & !1);
    (w > 0 && h > 0).then_some((w, h))
}

/// The Y plane copied and U and V interleaved, as [`i420_to_nv12`] does,
/// into one tightly packed buffer (rows of exactly the packed width, the
/// chroma plane right after the luma plane), the layout a Windows NV12
/// surface frame holds; an odd frame is cut to [`nv12_packed_size`].
/// `false`, with nothing written, unless `out` is exactly
/// [`nv12_packed_len`] of that size.
pub fn i420_to_nv12_packed(src: &I420, out: &mut [u8]) -> bool {
    let (w, h) = (src.width, src.height);
    let Some((pw, ph)) = nv12_packed_size(w, h) else {
        return false;
    };
    if nv12_packed_len(pw, ph) != Some(out.len()) {
        return false;
    }
    let (cw, _) = src.chroma_size();
    let Some((y_out, uv_out)) = out.split_at_mut_checked(pw * ph) else {
        return false;
    };
    let rows = src.y.chunks_exact(w).zip(y_out.chunks_exact_mut(pw));
    for (row, out) in rows.take(ph) {
        let Some(row) = row.get(..pw) else {
            return false;
        };
        out.copy_from_slice(row);
    }
    // `pw / 2` chroma samples a row, `ph / 2` rows: a packed row is `pw`
    // bytes of (Cb, Cr) pairs.
    let chroma = src.u.chunks_exact(cw).zip(src.v.chunks_exact(cw));
    for ((u_row, v_row), out) in chroma.zip(uv_out.chunks_exact_mut(pw)).take(ph / 2) {
        let (pairs, _) = out.as_chunks_mut::<2>();
        for (pair, (u, v)) in pairs.iter_mut().zip(u_row.iter().zip(v_row)) {
            *pair = [*u, *v];
        }
    }
    true
}

/// The per-byte terms of the engine's conversion, computed once a frame
/// with the same operations, so the result is bit-exact.
struct Terms {
    rv: [f32; 256],
    gu: [f32; 256],
    gv: [f32; 256],
    bu: [f32; 256],
}

impl Terms {
    fn new() -> Self {
        let mut terms = Self {
            rv: [0.; 256],
            gu: [0.; 256],
            gv: [0.; 256],
            bu: [0.; 256],
        };
        for (i, (((rv, gu), gv), bu)) in terms
            .rv
            .iter_mut()
            .zip(terms.gu.iter_mut())
            .zip(terms.gv.iter_mut())
            .zip(terms.bu.iter_mut())
            .enumerate()
        {
            let c = i as f32 - 128.0;
            *rv = 1.402 * c;
            *gu = 0.344_136 * c;
            *gv = 0.714_136 * c;
            *bu = 1.772 * c;
        }
        terms
    }

    /// `[b, g, r, 255]` for one pixel.
    fn bgra(&self, y: u8, u: u8, v: u8) -> [u8; 4] {
        let y = f32::from(y);
        let (u, v) = (usize::from(u), usize::from(v));
        let pick = |table: &[f32; 256], at: usize| table.get(at).copied().unwrap_or(0.);
        let r = y + pick(&self.rv, v);
        let g = y - pick(&self.gu, u) - pick(&self.gv, v);
        let b = y + pick(&self.bu, u);
        let byte = |value: f32| value.clamp(0.0, 255.0).round() as u8;
        [byte(b), byte(g), byte(r), 255]
    }
}

/// BGRA8 for GPUI's `RenderImage`, one pass. `None` for a malformed frame.
pub fn i420_to_bgra(src: &I420) -> Option<Vec<u8>> {
    let (w, h) = (src.width, src.height);
    let (cw, _) = src.chroma_size();
    if w == 0 || h == 0 {
        return None;
    }
    let terms = Terms::new();
    let mut out = vec![0u8; w.checked_mul(h)?.checked_mul(4)?];
    for (row, out_row) in out.chunks_exact_mut(w * 4).enumerate() {
        let y_row = src.y.get(row * w..row * w + w)?;
        let chroma = (row / 2) * cw;
        let u_row = src.u.get(chroma..chroma + cw)?;
        let v_row = src.v.get(chroma..chroma + cw)?;
        let (pixels, _) = out_row.as_chunks_mut::<4>();
        for (col, (pixel, y)) in pixels.iter_mut().zip(y_row).enumerate() {
            let (Some(u), Some(v)) = (u_row.get(col / 2), v_row.get(col / 2)) else {
                continue;
            };
            *pixel = terms.bgra(*y, *u, *v);
        }
    }
    Some(out)
}

/// I420 from RGBA8 (BT.601 full range, chroma averaged over each 2 × 2
/// block): pictures Quill makes itself, fed to the same player.
pub fn rgba_to_i420(rgba: &[u8], width: u16, height: u16) -> Option<Vec<u8>> {
    let (w, h) = (usize::from(width), usize::from(height));
    if w == 0 || h == 0 || rgba.len() != w * h * 4 {
        return None;
    }
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = vec![0u8; i420_size(width, height)];
    let (y_out, rest) = out.split_at_mut_checked(w * h)?;
    let (u_out, v_out) = rest.split_at_mut_checked(cw * ch)?;
    let (pixels, _) = rgba.as_chunks::<4>();
    let rgb = |x: usize, y: usize| {
        let [r, g, b, _] = pixels.get(y * w + x).copied().unwrap_or([0, 0, 0, 255]);
        (f32::from(r), f32::from(g), f32::from(b))
    };
    let byte = |value: f32| value.clamp(0.0, 255.0).round() as u8;
    for (at, out) in y_out.iter_mut().enumerate() {
        let (r, g, b) = rgb(at % w, at / w);
        *out = byte(0.299 * r + 0.587 * g + 0.114 * b);
    }
    for cy in 0..ch {
        for cx in 0..cw {
            let mut sum = (0., 0., 0.);
            let mut count = 0.;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (x, y) = (cx * 2 + dx, cy * 2 + dy);
                if x < w && y < h {
                    let (r, g, b) = rgb(x, y);
                    sum = (sum.0 + r, sum.1 + g, sum.2 + b);
                    count += 1.;
                }
            }
            let (r, g, b) = (sum.0 / count, sum.1 / count, sum.2 / count);
            let at = cy * cw + cx;
            if let (Some(u), Some(v)) = (u_out.get_mut(at), v_out.get_mut(at)) {
                *u = byte(-0.168_736 * r - 0.331_264 * g + 0.5 * b + 128.);
                *v = byte(0.5 * r - 0.418_688 * g - 0.081_312 * b + 128.);
            }
        }
    }
    Some(out)
}

/// tdesktop `kPausedVideoSize`: the paused picture fits 90 × 90.
pub const PAUSED_SIZE: u32 = 90;
/// tdesktop `kBlurRadius`.
pub const PAUSED_BLUR: u32 = 15;

/// The paused tile's picture (tdesktop `calls_group_viewport_raster.cpp`):
/// the frame turned, fitted into 90 × 90 (each pixel the average of a
/// 3 × 3 grid over its share of the frame) and blurred with radius 15, as
/// BGRA. Read straight from the planes; no full-size copy is made.
pub fn paused_raster(src: &I420, rotation: LiveRotation) -> Option<Raster> {
    let (w, h) = (src.width, src.height);
    if w == 0 || h == 0 {
        return None;
    }
    let (shown_w, shown_h) = if rotation.swaps_sides() {
        (h, w)
    } else {
        (w, h)
    };
    let side = PAUSED_SIZE as f32;
    let scale = (side / shown_w as f32).min(side / shown_h as f32);
    let fit_w = ((shown_w as f32 * scale).round() as usize).clamp(1, PAUSED_SIZE as usize);
    let fit_h = ((shown_h as f32 * scale).round() as usize).clamp(1, PAUSED_SIZE as usize);
    let (cw, _) = src.chroma_size();
    let terms = Terms::new();
    let mut bgra = Vec::with_capacity(fit_w * fit_h * 4);
    for oy in 0..fit_h {
        for ox in 0..fit_w {
            let (mut sy, mut su, mut sv, mut n) = (0u32, 0u32, 0u32, 0u32);
            for gy in 0..3 {
                for gx in 0..3 {
                    // The sample's place in the shown picture, then in the
                    // frame as received.
                    let px = ((ox * 3 + gx) * shown_w / (fit_w * 3)).min(shown_w - 1);
                    let py = ((oy * 3 + gy) * shown_h / (fit_h * 3)).min(shown_h - 1);
                    let (x, y) = source_of(px, py, w, h, rotation);
                    let chroma = (y / 2) * cw + x / 2;
                    if let (Some(yv), Some(uv), Some(vv)) =
                        (src.y.get(y * w + x), src.u.get(chroma), src.v.get(chroma))
                    {
                        sy += u32::from(*yv);
                        su += u32::from(*uv);
                        sv += u32::from(*vv);
                        n += 1;
                    }
                }
            }
            let n = n.max(1);
            let avg = |sum: u32| u8::try_from(sum / n).unwrap_or(u8::MAX);
            bgra.extend_from_slice(&terms.bgra(avg(sy), avg(su), avg(sv)));
        }
    }
    let mut raster = Raster {
        width: u32::try_from(fit_w).ok()?,
        height: u32::try_from(fit_h).ok()?,
        bgra,
    };
    blur(&mut raster, PAUSED_BLUR);
    Some(raster)
}

/// The frame pixel shown at `(x, y)` of the turned picture (the inverse
/// of [`turned`]).
fn source_of(x: usize, y: usize, w: usize, h: usize, rotation: LiveRotation) -> (usize, usize) {
    let (sx, sy) = match rotation {
        LiveRotation::R90 => (y, h.saturating_sub(1 + x)),
        LiveRotation::R180 => (w.saturating_sub(1 + x), h.saturating_sub(1 + y)),
        LiveRotation::R270 => (w.saturating_sub(1 + y), x),
        LiveRotation::None => (x, y),
    };
    (sx.min(w.saturating_sub(1)), sy.min(h.saturating_sub(1)))
}

/// A moving test picture as I420: a hue sweep across the width that
/// slides by `phase` and a brightness ramp down the height. Screenshot
/// demos and tests feed it through a `LiveSource`, so no call is needed.
pub fn gradient_frame(width: u16, height: u16, phase: u32) -> Vec<u8> {
    let (w, h) = (usize::from(width).max(1), usize::from(height).max(1));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = Vec::with_capacity(i420_size(width, height));
    for y in 0..usize::from(height) {
        let luma = 70. + 110. * (y as f32 / h as f32);
        for x in 0..usize::from(width) {
            let ripple = ((x as f32 / w as f32) * std::f32::consts::TAU * 2.).sin() * 12.;
            out.push((luma + ripple).clamp(0., 255.) as u8);
        }
    }
    let shift = phase as f32 / 90.;
    let hue = |cx: usize| (cx as f32 / cw as f32 + shift) * std::f32::consts::TAU;
    for _ in 0..usize::from(height).div_ceil(2) {
        for cx in 0..usize::from(width).div_ceil(2) {
            out.push((128. + 90. * hue(cx).cos()).clamp(0., 255.) as u8);
        }
    }
    for cy in 0..usize::from(height).div_ceil(2) {
        let fade = 1. - 0.4 * (cy as f32 / ch as f32);
        for cx in 0..usize::from(width).div_ceil(2) {
            out.push((128. + 90. * fade * hue(cx).sin()).clamp(0., 255.) as u8);
        }
    }
    out
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
