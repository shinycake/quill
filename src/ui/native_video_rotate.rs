//! Quarter turns for the pictures of the native video player.
//!
//! The viewer's Rotate turns the picture clockwise in 90 degree steps. The
//! decoded-image path turns `RenderImage`s; the macOS AVPlayer path hands
//! back GPU pixel buffers (bi-planar YUV 4:2:0) that GPUI draws as they are,
//! so the buffer itself is turned here: the luma plane and the interleaved
//! chroma plane, each as a plain transpose, into a new buffer of the turned
//! size.

/// Size of a `w` x `h` picture after `turns` clockwise quarter turns.
pub(super) fn turned_size(w: usize, h: usize, turns: u8) -> (usize, usize) {
    if turns % 2 == 1 { (h, w) } else { (w, h) }
}

/// Where the pixel at `(x, y)` of a `w` x `h` picture lands after `turns`
/// clockwise quarter turns.
pub(super) fn turned_position(
    (x, y): (usize, usize),
    (w, h): (usize, usize),
    turns: u8,
) -> (usize, usize) {
    match turns % 4 {
        0 => (x, y),
        1 => (h - 1 - y, x),
        2 => (w - 1 - x, h - 1 - y),
        _ => (y, w - 1 - x),
    }
}

/// Turn a plane of `T` (one pixel, or one interleaved chroma pair) by
/// `turns` clockwise quarter turns. Strides are in elements and may exceed
/// the width; the destination must hold the turned plane.
pub(super) fn turn_plane<T: Copy>(
    src: &[T],
    (w, h): (usize, usize),
    src_stride: usize,
    dst: &mut [T],
    dst_stride: usize,
    turns: u8,
) {
    for y in 0..h {
        let row = &src[y * src_stride..y * src_stride + w];
        for (x, &value) in row.iter().enumerate() {
            let (dx, dy) = turned_position((x, y), (w, h), turns);
            dst[dy * dst_stride + dx] = value;
        }
    }
}

/// Turn a decoded BGRA frame. Pixels move as four-byte units, so channel
/// order does not matter.
pub(super) fn turn_render_image(
    frame: &gpui_kit::RenderImage,
    turns: u8,
) -> Option<std::sync::Arc<gpui_kit::RenderImage>> {
    let size = frame.size(0);
    let bytes = frame.as_bytes(0)?;
    let (pixels, w, h) = quill::media_viewer::rotate_rgba_quarter_turns(
        bytes,
        size.width.0 as u32,
        size.height.0 as u32,
        turns,
    );
    let image = image::RgbaImage::from_raw(w, h, pixels)?;
    Some(std::sync::Arc::new(gpui_kit::RenderImage::new(
        smallvec::SmallVec::from_buf([image::Frame::new(image)]),
    )))
}

/// Turn an AVPlayer frame (`420YpCbCr8BiPlanarFullRange`). `None` for any
/// other format or when the new buffer can't be made.
#[cfg(target_os = "macos")]
pub(super) fn turn_surface(
    src: &core_video::pixel_buffer::CVPixelBuffer,
    turns: u8,
) -> Option<core_video::pixel_buffer::CVPixelBuffer> {
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;
    use core_video::pixel_buffer::{CVPixelBuffer, CVPixelBufferKeys, kCVPixelBufferLock_ReadOnly};

    if src.get_pixel_format() != objc2_core_video::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        || src.get_plane_count() != 2
    {
        return None;
    }
    let (w, h) = (src.get_width(), src.get_height());
    let (dw, dh) = turned_size(w, h, turns);
    let surface_props = CFDictionary::<CFString, CFType>::from_CFType_pairs(&[]);
    let options = CFDictionary::<CFString, CFType>::from_CFType_pairs(&[
        (
            CFString::from(CVPixelBufferKeys::IOSurfaceProperties),
            surface_props.as_CFType(),
        ),
        (
            CFString::from(CVPixelBufferKeys::MetalCompatibility),
            CFBoolean::true_value().as_CFType(),
        ),
    ]);
    let dst = CVPixelBuffer::new(src.get_pixel_format(), dw, dh, Some(&options)).ok()?;
    if src.lock_base_address(kCVPixelBufferLock_ReadOnly) != 0 {
        return None;
    }
    if dst.lock_base_address(0) != 0 {
        src.unlock_base_address(kCVPixelBufferLock_ReadOnly);
        return None;
    }
    // SAFETY: both buffers are locked, so each plane's base address is
    // valid for `bytes_per_row * height` bytes, and the planes are not
    // shared between the two buffers. Chroma is read as `[u8; 2]` pairs,
    // which have no alignment need.
    unsafe {
        for plane in 0..2 {
            let (pw, ph) = (
                src.get_width_of_plane(plane),
                src.get_height_of_plane(plane),
            );
            let (tw, th) = turned_size(pw, ph, turns);
            let (src_stride, dst_stride) = (
                src.get_bytes_per_row_of_plane(plane),
                dst.get_bytes_per_row_of_plane(plane),
            );
            let src_base = src.get_base_address_of_plane(plane).cast::<u8>();
            let dst_base = dst.get_base_address_of_plane(plane).cast::<u8>();
            if plane == 0 {
                let from = std::slice::from_raw_parts(src_base, src_stride * ph);
                let to = std::slice::from_raw_parts_mut(dst_base, dst_stride * th);
                turn_plane(from, (pw, ph), src_stride, to, dst_stride, turns);
            } else {
                let from =
                    std::slice::from_raw_parts(src_base.cast::<[u8; 2]>(), src_stride / 2 * ph);
                let to =
                    std::slice::from_raw_parts_mut(dst_base.cast::<[u8; 2]>(), dst_stride / 2 * th);
                turn_plane(from, (pw, ph), src_stride / 2, to, dst_stride / 2, turns);
            }
            debug_assert!(tw <= dst_stride);
        }
    }
    dst.unlock_base_address(0);
    src.unlock_base_address(kCVPixelBufferLock_ReadOnly);
    Some(dst)
}

#[cfg(test)]
mod tests {
    use super::{turn_plane, turned_position, turned_size};

    #[test]
    fn size_swaps_on_odd_turns() {
        assert_eq!(turned_size(4, 2, 0), (4, 2));
        assert_eq!(turned_size(4, 2, 1), (2, 4));
        assert_eq!(turned_size(4, 2, 2), (4, 2));
        assert_eq!(turned_size(4, 2, 3), (2, 4));
    }

    #[test]
    fn corners_follow_a_clockwise_turn() {
        // Top-left of a 4x2 picture goes to the top-right of the 2x4 result.
        assert_eq!(turned_position((0, 0), (4, 2), 1), (1, 0));
        // Top-right goes to the bottom-right.
        assert_eq!(turned_position((3, 0), (4, 2), 1), (1, 3));
        assert_eq!(turned_position((0, 0), (4, 2), 2), (3, 1));
        assert_eq!(turned_position((0, 0), (4, 2), 3), (0, 3));
    }

    #[test]
    fn four_turns_are_the_identity() {
        let src: Vec<u8> = (0..6).collect();
        let mut once = vec![0u8; 6];
        turn_plane(&src, (3, 2), 3, &mut once, 2, 1);
        // 3x2 turned clockwise: rows [3,0] and [4,1] and [5,2].
        assert_eq!(once, vec![3, 0, 4, 1, 5, 2]);
        let mut back = src.clone();
        let mut cur = src.clone();
        let (mut w, mut h) = (3, 2);
        for _ in 0..4 {
            let (nw, nh) = turned_size(w, h, 1);
            let mut next = vec![0u8; nw * nh];
            turn_plane(&cur, (w, h), w, &mut next, nw, 1);
            cur = next;
            (w, h) = (nw, nh);
        }
        back.truncate(6);
        assert_eq!(cur, back);
    }

    #[test]
    fn strides_wider_than_the_picture_are_skipped() {
        // 2x2 picture inside rows of 4.
        let src = [1u8, 2, 9, 9, 3, 4, 9, 9];
        let mut dst = [0u8; 6];
        turn_plane(&src, (2, 2), 4, &mut dst, 3, 1);
        assert_eq!(&dst[0..2], &[3, 1]);
        assert_eq!(&dst[3..5], &[4, 2]);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod surface_tests {
    use super::turn_surface;
    use core_video::pixel_buffer::{CVPixelBuffer, kCVPixelBufferLock_ReadOnly};

    fn plane(buffer: &CVPixelBuffer, index: usize, pairs: bool) -> Vec<Vec<u8>> {
        let (w, h) = (
            buffer.get_width_of_plane(index),
            buffer.get_height_of_plane(index),
        );
        let stride = buffer.get_bytes_per_row_of_plane(index);
        let unit = if pairs { 2 } else { 1 };
        // SAFETY: the caller has locked the buffer.
        let base = unsafe { buffer.get_base_address_of_plane(index) }.cast::<u8>();
        (0..h)
            .map(|y| {
                // SAFETY: the buffer is locked by the caller and a row has
                // `stride` readable bytes.
                let row = unsafe { std::slice::from_raw_parts(base.add(y * stride), stride) };
                row[..w * unit].to_vec()
            })
            .collect()
    }

    #[test]
    fn a_surface_turns_clockwise_in_both_planes() {
        let format = objc2_core_video::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
        let src = CVPixelBuffer::new(format, 4, 2, None).unwrap();
        src.lock_base_address(0);
        // SAFETY: locked; every written byte is inside its plane row.
        unsafe {
            for index in 0..2 {
                let stride = src.get_bytes_per_row_of_plane(index);
                let base = src.get_base_address_of_plane(index).cast::<u8>();
                let (w, h) = (
                    src.get_width_of_plane(index),
                    src.get_height_of_plane(index),
                );
                let unit = if index == 0 { 1 } else { 2 };
                for y in 0..h {
                    for x in 0..w * unit {
                        *base.add(y * stride + x) = (y * 16 + x) as u8;
                    }
                }
            }
        }
        src.unlock_base_address(0);
        let turned = turn_surface(&src, 1).expect("turns a bi-planar buffer");
        assert_eq!((turned.get_width(), turned.get_height()), (2, 4));
        turned.lock_base_address(kCVPixelBufferLock_ReadOnly);
        let luma = plane(&turned, 0, false);
        // Source rows [0 1 2 3] and [16 17 18 19]; clockwise: columns read
        // bottom to top become rows.
        assert_eq!(
            luma,
            vec![vec![16, 0], vec![17, 1], vec![18, 2], vec![19, 3]]
        );
        let chroma = plane(&turned, 1, true);
        // Source chroma is 2x1 pairs: [0 1][2 3]; turned it is 1x2.
        assert_eq!(chroma, vec![vec![0, 1], vec![2, 3]]);
        turned.unlock_base_address(kCVPixelBufferLock_ReadOnly);
    }
}
