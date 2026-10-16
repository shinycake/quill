//! One PipeWire screen cast buffer in the negotiated format to the call's
//! I420 frame size. A buffer has one data block per plane, or one block
//! holding every plane back to back; a stride of 0 means the producer
//! left it out and rows are tightly packed.
//!
//! Plain Rust, tested on every platform; `screen_linux.rs` feeds it.
#![deny(clippy::indexing_slicing)]

use super::screen_bgra::{PixelOrder, packed_to_i420};
use super::screen_yuv::{Plane, i420_scaled, nv12_scaled};
use super::spa_pod::{NegotiatedFormat, VideoFormat};

/// One data block: its bytes and row stride (0 when not given).
pub type Block<'a> = (&'a [u8], usize);

fn stride_or(stride: usize, packed: usize) -> usize {
    if stride == 0 { packed } else { stride }
}

/// The `index`th plane of a planar picture whose Y plane is `height`
/// rows of `y_stride`: its own block when the buffer has one per plane,
/// else the bytes after the planes before it.
fn plane<'a>(
    blocks: &[Block<'a>],
    index: usize,
    offset: usize,
    packed_stride: usize,
) -> Option<Plane<'a>> {
    if blocks.len() > index {
        let (bytes, stride) = *blocks.get(index)?;
        return Some(Plane::new(bytes, stride_or(stride, packed_stride)));
    }
    let (bytes, _) = *blocks.first()?;
    Some(Plane::new(bytes.get(offset..)?, packed_stride))
}

/// The buffer as an I420 frame of `dst` size (even), or `None` when the
/// blocks are too short for the format.
pub fn buffer_to_i420(
    format: NegotiatedFormat,
    blocks: &[Block<'_>],
    dst: (usize, usize),
) -> Option<Vec<u8>> {
    let (w, h) = (
        usize::try_from(format.width).ok()?,
        usize::try_from(format.height).ok()?,
    );
    let (first, first_stride) = *blocks.first()?;
    match format.format {
        VideoFormat::Bgrx | VideoFormat::Bgra | VideoFormat::Rgbx | VideoFormat::Rgba => {
            let order = if matches!(format.format, VideoFormat::Rgbx | VideoFormat::Rgba) {
                PixelOrder::Rgb
            } else {
                PixelOrder::Bgr
            };
            let stride = stride_or(first_stride, w.checked_mul(4)?);
            packed_to_i420(first, order, stride, (w, h), dst)
        }
        VideoFormat::I420 => {
            let y_stride = stride_or(first_stride, w);
            let c_stride = if first_stride == 0 {
                w.div_ceil(2)
            } else {
                y_stride / 2
            };
            let y_size = y_stride.checked_mul(h)?;
            let c_size = c_stride.checked_mul(h.div_ceil(2))?;
            let y = Plane::new(first, y_stride);
            let u = plane(blocks, 1, y_size, c_stride)?;
            let v = plane(blocks, 2, y_size.checked_add(c_size)?, c_stride)?;
            i420_scaled([y, u, v], (w, h), dst)
        }
        VideoFormat::Nv12 => {
            let y_stride = stride_or(first_stride, w);
            let uv_stride = if first_stride == 0 {
                w.div_ceil(2) * 2
            } else {
                y_stride
            };
            let y = Plane::new(first, y_stride);
            let uv = plane(blocks, 1, y_stride.checked_mul(h)?, uv_stride)?;
            nv12_scaled(y, uv, (w, h), dst)
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn format(format: VideoFormat, width: u32, height: u32) -> NegotiatedFormat {
        NegotiatedFormat {
            format,
            width,
            height,
        }
    }

    #[test]
    fn packed_rgbx_converts_with_its_stride() {
        // 2 x 2 red RGBx with 4 bytes of row padding.
        let mut bytes = vec![0u8; 12 * 2];
        for row in 0..2 {
            for x in 0..2 {
                bytes[row * 12 + x * 4..row * 12 + x * 4 + 4].copy_from_slice(&[255, 0, 0, 0]);
            }
        }
        let i420 =
            buffer_to_i420(format(VideoFormat::Rgbx, 2, 2), &[(&bytes, 12)], (2, 2)).expect("i420");
        assert_eq!(i420, vec![82, 82, 82, 82, 90, 240]);
    }

    #[test]
    fn i420_in_one_block_or_three() {
        let one: Vec<u8> = (0..6).collect();
        let packed = buffer_to_i420(format(VideoFormat::I420, 2, 2), &[(&one, 0)], (2, 2));
        assert_eq!(packed, Some(one.clone()));
        let (y, u, v) = ([0u8, 1, 0xee, 2, 3, 0xee], [4u8, 0xee], [5u8, 0xee]);
        let split = buffer_to_i420(
            format(VideoFormat::I420, 2, 2),
            &[(&y, 3), (&u, 2), (&v, 2)],
            (2, 2),
        );
        assert_eq!(split, Some(one));
    }

    #[test]
    fn nv12_in_one_block() {
        let bytes = [0u8, 1, 2, 3, 40, 50];
        let i420 = buffer_to_i420(format(VideoFormat::Nv12, 2, 2), &[(&bytes, 0)], (2, 2));
        assert_eq!(i420, Some(vec![0, 1, 2, 3, 40, 50]));
    }

    #[test]
    fn short_or_missing_blocks_are_refused() {
        assert_eq!(
            buffer_to_i420(format(VideoFormat::Bgrx, 2, 2), &[], (2, 2)),
            None
        );
        let short = [0u8; 4];
        assert_eq!(
            buffer_to_i420(format(VideoFormat::Bgrx, 2, 2), &[(&short, 0)], (2, 2)),
            None
        );
        assert_eq!(
            buffer_to_i420(format(VideoFormat::I420, 2, 2), &[(&short, 0)], (2, 2)),
            None
        );
    }
}
