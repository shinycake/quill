//! Packed 32-bit pictures (BGRA from Windows.Graphics.Capture and X11,
//! BGRx or RGBx from PipeWire) to the call's I420 frames and the chooser's
//! RGBA thumbnails, plus the frame pacing captures that deliver at the
//! display's rate need.
//!
//! Plain Rust with no OS calls, so it is unit-tested on every platform.
#![deny(clippy::indexing_slicing)]

use super::screen::{Thumbnail, fit_capture_size};
use std::time::{Duration, Instant};

/// Where red, green and blue sit in a packed 4-byte pixel. The fourth
/// byte (alpha or padding) is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackedOrder {
    /// B, G, R, A or x: `BGRA` / `BGRx` (little-endian `XRGB8888`).
    Bgra,
    /// R, G, B, A or x: `RGBA` / `RGBx` (little-endian `XBGR8888`).
    Rgba,
}

/// One pixel, or `None` past the end of the buffer.
fn pixel<const R: usize, const B: usize>(
    src: &[u8],
    stride: usize,
    x: usize,
    y: usize,
) -> Option<(i32, i32, i32)> {
    let at = y.checked_mul(stride)?.checked_add(x.checked_mul(4)?)?;
    row_pixel::<R, B>(src, at)
}

/// BT.601 video range, the integer form libyuv uses.
fn luma((r, g, b): (i32, i32, i32)) -> u8 {
    (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8
}

fn chroma((r, g, b): (i32, i32, i32)) -> (u8, u8) {
    let u = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
    (u.clamp(0, 255) as u8, v.clamp(0, 255) as u8)
}

/// A `src_w` x `src_h` BGRA picture (rows `stride` bytes apart) scaled to
/// `dst_w` x `dst_h` (nearest pixel) as I420: Y, then U, then V, each
/// chroma sample the average of its 2 x 2 block. `dst_w` and `dst_h` must
/// be even; `None` when the sizes or the buffer do not fit.
pub fn bgra_to_i420(
    src: &[u8],
    stride: usize,
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Option<Vec<u8>> {
    packed_to_i420(
        src,
        PackedOrder::Bgra,
        stride,
        (src_w, src_h),
        (dst_w, dst_h),
    )
}

/// [`bgra_to_i420`] for either byte order; sizes are `(width, height)`.
pub fn packed_to_i420(
    src: &[u8],
    order: PackedOrder,
    stride: usize,
    (src_w, src_h): (usize, usize),
    (dst_w, dst_h): (usize, usize),
) -> Option<Vec<u8>> {
    match order {
        PackedOrder::Bgra => packed_i420::<2, 0>(src, stride, src_w, src_h, dst_w, dst_h),
        PackedOrder::Rgba => packed_i420::<0, 2>(src, stride, src_w, src_h, dst_w, dst_h),
    }
}

/// The conversion, with red at byte `R` and blue at byte `B` of a pixel
/// (green is always byte 1), so the inner loop has no branch.
fn packed_i420<const R: usize, const B: usize>(
    src: &[u8],
    stride: usize,
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
) -> Option<Vec<u8>> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return None;
    }
    if !dst_w.is_multiple_of(2) || !dst_h.is_multiple_of(2) || stride < src_w.checked_mul(4)? {
        return None;
    }
    let (cw, ch) = (dst_w / 2, dst_h / 2);
    let row_bytes = src_w.checked_mul(4)?;
    // Byte offset in a source row of each output column, worked out once.
    let columns: Vec<usize> = (0..dst_w).map(|x| x * src_w / dst_w * 4).collect();
    let source_row = |y: usize| {
        let start = (y * src_h / dst_h).checked_mul(stride)?;
        src.get(start..start.checked_add(row_bytes)?)
    };
    let mut out = vec![0u8; dst_w * dst_h + 2 * cw * ch];
    let (y_plane, chroma_planes) = out.split_at_mut(dst_w * dst_h);
    let (u_plane, v_plane) = chroma_planes.split_at_mut(cw * ch);
    // Two output rows at a time: their four pixels per chroma sample are
    // read once, for both the luma and the chroma.
    let rows = y_plane.chunks_exact_mut(dst_w * 2).zip(
        u_plane
            .chunks_exact_mut(cw)
            .zip(v_plane.chunks_exact_mut(cw)),
    );
    for (cy, (luma_rows, (u_row, v_row))) in rows.enumerate() {
        let (top_out, bottom_out) = luma_rows.split_at_mut(dst_w);
        let top = source_row(cy * 2)?;
        let bottom = source_row(cy * 2 + 1)?;
        let blocks = top_out
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(bottom_out.as_chunks_mut::<2>().0.iter_mut())
            .zip(columns.as_chunks::<2>().0)
            .zip(u_row.iter_mut().zip(v_row.iter_mut()));
        for (((top_pair, bottom_pair), xs), (u, v)) in blocks {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for (out_row, src_row) in [(top_pair, top), (bottom_pair, bottom)] {
                for (out, &x) in out_row.iter_mut().zip(xs) {
                    let rgb = row_pixel::<R, B>(src_row, x)?;
                    *out = luma(rgb);
                    r += rgb.0;
                    g += rgb.1;
                    b += rgb.2;
                }
            }
            (*u, *v) = chroma((r / 4, g / 4, b / 4));
        }
    }
    Some(out)
}

/// The pixel at byte `x` of a row, as RGB.
fn row_pixel<const R: usize, const B: usize>(row: &[u8], x: usize) -> Option<(i32, i32, i32)> {
    let px = row.get(x..x.checked_add(4)?)?.first_chunk::<4>()?;
    let (r, g, b) = (*px.get(R)?, px[1], *px.get(B)?);
    Some((i32::from(r), i32::from(g), i32::from(b)))
}

/// A BGRA picture fitted into `max` as an opaque RGBA thumbnail.
pub fn bgra_thumbnail(
    src: &[u8],
    stride: usize,
    src_w: usize,
    src_h: usize,
    max: (u32, u32),
) -> Option<Thumbnail> {
    packed_thumbnail(src, PackedOrder::Bgra, stride, (src_w, src_h), max)
}

/// [`bgra_thumbnail`] for either byte order.
pub fn packed_thumbnail(
    src: &[u8],
    order: PackedOrder,
    stride: usize,
    (src_w, src_h): (usize, usize),
    max: (u32, u32),
) -> Option<Thumbnail> {
    match order {
        PackedOrder::Bgra => thumbnail::<2, 0>(src, stride, src_w, src_h, max),
        PackedOrder::Rgba => thumbnail::<0, 2>(src, stride, src_w, src_h, max),
    }
}

fn thumbnail<const R: usize, const B: usize>(
    src: &[u8],
    stride: usize,
    src_w: usize,
    src_h: usize,
    max: (u32, u32),
) -> Option<Thumbnail> {
    if src_w == 0 || src_h == 0 || stride < src_w.checked_mul(4)? {
        return None;
    }
    let (width, height) = fit_capture_size(
        u32::try_from(src_w).ok()?,
        u32::try_from(src_h).ok()?,
        max.0,
        max.1,
    );
    let (ow, oh) = (width as usize, height as usize);
    let mut rgba = Vec::with_capacity(ow * oh * 4);
    for y in 0..oh {
        for x in 0..ow {
            let (r, g, b) = pixel::<R, B>(src, stride, x * src_w / ow, y * src_h / oh)?;
            rgba.extend_from_slice(&[r as u8, g as u8, b as u8, 255]);
        }
    }
    Some(Thumbnail {
        width,
        height,
        rgba,
    })
}

/// Lets through at most one frame per `1 / fps`.
pub struct FramePacer {
    interval: Duration,
    last: Option<Instant>,
}

impl FramePacer {
    pub fn new(fps: u8) -> Self {
        Self {
            interval: Duration::from_secs(1) / u32::from(fps.max(1)),
            last: None,
        }
    }

    /// Whether a frame arriving at `now` should be sent.
    pub fn due(&mut self, now: Instant) -> bool {
        // A little slack, so 60 Hz frames make an even 15 fps.
        let slack = self.interval / 8;
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) + slack >= self.interval);
        if due {
            self.last = Some(now);
        }
        due
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A `w` x `h` BGRA picture of one colour, with `pad` spare bytes per row.
    fn solid(w: usize, h: usize, pad: usize, [r, g, b]: [u8; 3]) -> (Vec<u8>, usize) {
        let stride = w * 4 + pad;
        let mut out = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let at = y * stride + x * 4;
                out[at..at + 4].copy_from_slice(&[b, g, r, 255]);
            }
        }
        (out, stride)
    }

    #[test]
    fn colours_land_on_bt601_video_range() {
        let (white, stride) = solid(4, 4, 8, [255, 255, 255]);
        let i420 = bgra_to_i420(&white, stride, 4, 4, 4, 4).expect("i420");
        assert_eq!(i420.len(), 16 + 4 + 4);
        assert!(i420[..16].iter().all(|&y| y == 235));
        assert!(i420[16..].iter().all(|&c| c == 128));
        let (black, stride) = solid(4, 4, 0, [0, 0, 0]);
        let i420 = bgra_to_i420(&black, stride, 4, 4, 4, 4).expect("i420");
        assert!(i420[..16].iter().all(|&y| y == 16));
        let (red, stride) = solid(2, 2, 0, [255, 0, 0]);
        let i420 = bgra_to_i420(&red, stride, 2, 2, 2, 2).expect("i420");
        assert_eq!(&i420[..1], &[82]);
        assert_eq!(&i420[4..], &[90, 240]);
    }

    #[test]
    fn a_large_picture_scales_down_to_the_frame_size() {
        let (blue, stride) = solid(64, 32, 0, [0, 0, 255]);
        let i420 = bgra_to_i420(&blue, stride, 64, 32, 16, 8).expect("i420");
        assert_eq!(i420.len(), 16 * 8 * 3 / 2);
        assert!(i420[..128].iter().all(|&y| y == 41));
    }

    #[test]
    fn bad_sizes_and_short_buffers_are_refused() {
        let (picture, stride) = solid(4, 4, 0, [1, 2, 3]);
        assert!(bgra_to_i420(&picture, stride, 4, 4, 3, 4).is_none());
        assert!(bgra_to_i420(&picture, stride, 4, 4, 0, 4).is_none());
        assert!(bgra_to_i420(&picture, 8, 4, 4, 4, 4).is_none());
        assert!(bgra_to_i420(&picture[..20], stride, 4, 4, 4, 4).is_none());
        assert!(bgra_thumbnail(&picture[..20], stride, 4, 4, (4, 4)).is_none());
    }

    #[test]
    fn thumbnails_are_rgba_and_fit() {
        let (green, stride) = solid(400, 200, 12, [10, 200, 30]);
        let thumb = bgra_thumbnail(&green, stride, 400, 200, (100, 100)).expect("thumb");
        assert_eq!((thumb.width, thumb.height), (100, 50));
        assert_eq!(&thumb.rgba[..4], &[10, 200, 30, 255]);
        assert_eq!(thumb.rgba.len(), 100 * 50 * 4);
    }

    #[test]
    fn rgba_order_reads_red_first() {
        // A red BGRA picture read as RGBA is blue, and the other way round.
        let (red, stride) = solid(2, 2, 0, [255, 0, 0]);
        let as_rgba = packed_to_i420(&red, PackedOrder::Rgba, stride, (2, 2), (2, 2)).unwrap();
        let (blue, stride) = solid(2, 2, 0, [0, 0, 255]);
        let as_bgra = packed_to_i420(&blue, PackedOrder::Bgra, stride, (2, 2), (2, 2)).unwrap();
        assert_eq!(as_rgba, as_bgra);
        let thumb = packed_thumbnail(&red, PackedOrder::Rgba, stride, (2, 2), (2, 2)).unwrap();
        assert_eq!(&thumb.rgba[..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn the_pacer_turns_sixty_hertz_into_fifteen_fps() {
        let mut pacer = FramePacer::new(15);
        let start = Instant::now();
        let tick = Duration::from_micros(16_667);
        let sent = (0..60u32).filter(|i| pacer.due(start + tick * *i)).count();
        assert_eq!(sent, 15);
    }
}
