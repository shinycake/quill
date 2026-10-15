//! Quarter turns and mirroring for the pictures of the native video player.
//!
//! The viewer's Rotate turns the picture clockwise in 90 degree steps; the
//! video editor also mirrors it left to right after the turns. The
//! decoded-image path turns `RenderImage`s; the macOS AVPlayer path hands
//! back GPU pixel buffers (bi-planar YUV 4:2:0) that GPUI draws as they are,
//! so the buffer itself is turned here: the luma plane and the interleaved
//! chroma plane, each as a plain transpose, into a new buffer of the turned
//! size.
#![deny(clippy::indexing_slicing)]
// The buffer turns serve the macOS AVPlayer path only.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

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
/// `turns` clockwise quarter turns, then mirror it left to right when
/// `mirror` is set. Strides are in elements and may exceed the width; the
/// destination must hold the turned plane. Pixels that would land outside
/// it are skipped.
pub(super) fn turn_plane<T: Copy>(
    src: &[T],
    (w, h): (usize, usize),
    src_stride: usize,
    dst: &mut [T],
    dst_stride: usize,
    turns: u8,
    mirror: bool,
) {
    let (turned_w, _) = turned_size(w, h, turns);
    for y in 0..h {
        let Some(row) = src.get(y * src_stride..y * src_stride + w) else {
            return;
        };
        for (x, &value) in row.iter().enumerate() {
            let (mut dx, dy) = turned_position((x, y), (w, h), turns);
            if mirror {
                dx = turned_w - 1 - dx;
            }
            if let Some(slot) = dst.get_mut(dy * dst_stride + dx) {
                *slot = value;
            }
        }
    }
}

/// Turn (and mirror) a decoded BGRA frame. Pixels move as four-byte
/// units, so channel order does not matter.
pub(super) fn turn_render_image(
    frame: &gpui_kit::RenderImage,
    turns: u8,
    mirror: bool,
) -> Option<std::sync::Arc<gpui_kit::RenderImage>> {
    let size = frame.size(0);
    let (w, h) = (size.width.0 as usize, size.height.0 as usize);
    let (pixels, _) = frame.as_bytes(0)?.as_chunks::<4>();
    if pixels.len() < w * h {
        return None;
    }
    let (tw, th) = turned_size(w, h, turns);
    let mut turned = vec![[0u8; 4]; tw * th];
    turn_plane(pixels, (w, h), w, &mut turned, tw, turns, mirror);
    let image = image::RgbaImage::from_raw(tw as u32, th as u32, turned.into_flattened())?;
    Some(std::sync::Arc::new(gpui_kit::RenderImage::new(
        smallvec::SmallVec::from_buf([image::Frame::new(image)]),
    )))
}

/// Turn an AVPlayer frame (`420YpCbCr8BiPlanarFullRange`). `None` for any
/// other format or when the new buffer can't be made.
#[cfg(target_os = "macos")]
pub(super) fn turn_surface(
    src: &objc2_core_video::CVPixelBuffer,
    turns: u8,
    mirror: bool,
) -> Option<objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>> {
    use objc2_core_video::{
        CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
        CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType,
        CVPixelBufferGetPlaneCount, CVPixelBufferGetWidth, CVPixelBufferGetWidthOfPlane,
        CVPixelBufferLockFlags, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    };

    let format = CVPixelBufferGetPixelFormatType(src);
    if format != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        || CVPixelBufferGetPlaneCount(src) != 2
    {
        return None;
    }
    let (w, h) = (CVPixelBufferGetWidth(src), CVPixelBufferGetHeight(src));
    let (dw, dh) = turned_size(w, h, turns);
    let dst = new_pixel_buffer(format, dw, dh)?;
    if !lock(src, CVPixelBufferLockFlags::ReadOnly) {
        return None;
    }
    if !lock(&dst, CVPixelBufferLockFlags::empty()) {
        unlock(src, CVPixelBufferLockFlags::ReadOnly);
        return None;
    }
    for plane in 0..2 {
        let (pw, ph) = (
            CVPixelBufferGetWidthOfPlane(src, plane),
            CVPixelBufferGetHeightOfPlane(src, plane),
        );
        let (tw, th) = turned_size(pw, ph, turns);
        let (src_stride, dst_stride) = (
            CVPixelBufferGetBytesPerRowOfPlane(src, plane),
            CVPixelBufferGetBytesPerRowOfPlane(&dst, plane),
        );
        let src_base = CVPixelBufferGetBaseAddressOfPlane(src, plane).cast::<u8>();
        let dst_base = CVPixelBufferGetBaseAddressOfPlane(&dst, plane).cast::<u8>();
        if src_base.is_null() || dst_base.is_null() {
            continue;
        }
        // SAFETY: both buffers are locked, so each plane's base address is
        // valid for `bytes_per_row * height` bytes, and the planes are not
        // shared between the two buffers. Chroma is read as `[u8; 2]`
        // pairs, which have no alignment need.
        unsafe {
            if plane == 0 {
                let from = std::slice::from_raw_parts(src_base, src_stride * ph);
                let to = std::slice::from_raw_parts_mut(dst_base, dst_stride * th);
                turn_plane(from, (pw, ph), src_stride, to, dst_stride, turns, mirror);
            } else {
                let from =
                    std::slice::from_raw_parts(src_base.cast::<[u8; 2]>(), src_stride / 2 * ph);
                let to =
                    std::slice::from_raw_parts_mut(dst_base.cast::<[u8; 2]>(), dst_stride / 2 * th);
                turn_plane(
                    from,
                    (pw, ph),
                    src_stride / 2,
                    to,
                    dst_stride / 2,
                    turns,
                    mirror,
                );
            }
        }
        debug_assert!(tw <= dst_stride);
    }
    unlock(&dst, CVPixelBufferLockFlags::empty());
    unlock(src, CVPixelBufferLockFlags::ReadOnly);
    Some(dst)
}

/// A new IOSurface-backed, Metal-compatible pixel buffer (what GPUI's
/// surface path draws), or `None` when CoreVideo can't make one.
#[cfg(target_os = "macos")]
fn new_pixel_buffer(
    format: u32,
    width: usize,
    height: usize,
) -> Option<objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>> {
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
    use objc2_core_video::{
        CVPixelBufferCreate, kCVPixelBufferIOSurfacePropertiesKey,
        kCVPixelBufferMetalCompatibilityKey, kCVReturnSuccess,
    };

    let surface_props = CFDictionary::<CFType, CFType>::empty();
    // SAFETY: CoreVideo's attribute keys are constant CFStrings that live
    // for the whole process.
    let keys: [&CFString; 2] = unsafe {
        [
            kCVPixelBufferIOSurfacePropertiesKey,
            kCVPixelBufferMetalCompatibilityKey,
        ]
    };
    let values: [&CFType; 2] = [&surface_props, CFBoolean::new(true)];
    let options = CFDictionary::from_slices(&keys, &values);
    let mut raw = std::ptr::null_mut();
    // SAFETY: `options` maps CoreVideo's CFString keys to CF values of the
    // types they take, and `raw` is a valid out pointer.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            width,
            height,
            format,
            Some(options.as_opaque()),
            std::ptr::NonNull::from(&mut raw),
        )
    };
    if status != kCVReturnSuccess {
        return None;
    }
    // SAFETY: on success `raw` holds an owned (+1) pixel buffer.
    std::ptr::NonNull::new(raw).map(|raw| unsafe { CFRetained::from_raw(raw) })
}

/// Lock a pixel buffer's base address; `false` when CoreVideo refuses.
#[cfg(target_os = "macos")]
fn lock(
    buffer: &objc2_core_video::CVPixelBuffer,
    flags: objc2_core_video::CVPixelBufferLockFlags,
) -> bool {
    // SAFETY: locking a live buffer; the caller unlocks it with the same
    // flags.
    let status = unsafe { objc2_core_video::CVPixelBufferLockBaseAddress(buffer, flags) };
    status == objc2_core_video::kCVReturnSuccess
}

/// Undo a successful [`lock`] with the same flags.
#[cfg(target_os = "macos")]
fn unlock(
    buffer: &objc2_core_video::CVPixelBuffer,
    flags: objc2_core_video::CVPixelBufferLockFlags,
) {
    // SAFETY: the buffer was locked with these flags by `lock`.
    unsafe { objc2_core_video::CVPixelBufferUnlockBaseAddress(buffer, flags) };
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
        turn_plane(&src, (3, 2), 3, &mut once, 2, 1, false);
        // 3x2 turned clockwise: rows [3,0] and [4,1] and [5,2].
        assert_eq!(once, vec![3, 0, 4, 1, 5, 2]);
        let mut back = src.clone();
        let mut cur = src.clone();
        let (mut w, mut h) = (3, 2);
        for _ in 0..4 {
            let (nw, nh) = turned_size(w, h, 1);
            let mut next = vec![0u8; nw * nh];
            turn_plane(&cur, (w, h), w, &mut next, nw, 1, false);
            cur = next;
            (w, h) = (nw, nh);
        }
        back.truncate(6);
        assert_eq!(cur, back);
    }

    #[test]
    fn a_mirror_follows_the_turn() {
        // 2x1 [1 2] turned clockwise is the column [1; 2]; mirrored it
        // stays, being one wide. Unturned, the mirror swaps the pixels.
        let src = [1u8, 2];
        let mut dst = [0u8; 2];
        turn_plane(&src, (2, 1), 2, &mut dst, 2, 0, true);
        assert_eq!(dst, [2, 1]);
        // 2x2 [1 2; 3 4] turned clockwise is [3 1; 4 2], mirrored [1 3; 2 4].
        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];
        turn_plane(&src, (2, 2), 2, &mut dst, 2, 1, true);
        assert_eq!(dst, [1, 3, 2, 4]);
    }

    #[test]
    fn strides_wider_than_the_picture_are_skipped() {
        // 2x2 picture inside rows of 4.
        let src = [1u8, 2, 9, 9, 3, 4, 9, 9];
        let mut dst = [0u8; 6];
        turn_plane(&src, (2, 2), 4, &mut dst, 3, 1, false);
        assert_eq!(&dst[0..2], &[3, 1]);
        assert_eq!(&dst[3..5], &[4, 2]);
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod surface_tests {
    use super::{lock, new_pixel_buffer, turn_surface, unlock};
    use objc2_core_video::{
        CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
        CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetWidth,
        CVPixelBufferGetWidthOfPlane, CVPixelBufferLockFlags,
    };

    fn plane(buffer: &CVPixelBuffer, index: usize, pairs: bool) -> Vec<Vec<u8>> {
        let (w, h) = (
            CVPixelBufferGetWidthOfPlane(buffer, index),
            CVPixelBufferGetHeightOfPlane(buffer, index),
        );
        let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
        let unit = if pairs { 2 } else { 1 };
        // The caller has locked the buffer.
        let base = CVPixelBufferGetBaseAddressOfPlane(buffer, index).cast::<u8>();
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
        let src = new_pixel_buffer(format, 4, 2).unwrap();
        assert!(lock(&src, CVPixelBufferLockFlags::empty()));
        for index in 0..2 {
            let stride = CVPixelBufferGetBytesPerRowOfPlane(&src, index);
            let base = CVPixelBufferGetBaseAddressOfPlane(&src, index).cast::<u8>();
            let (w, h) = (
                CVPixelBufferGetWidthOfPlane(&src, index),
                CVPixelBufferGetHeightOfPlane(&src, index),
            );
            let unit = if index == 0 { 1 } else { 2 };
            for y in 0..h {
                for x in 0..w * unit {
                    // SAFETY: locked; every written byte is inside its plane
                    // row.
                    unsafe { *base.add(y * stride + x) = (y * 16 + x) as u8 };
                }
            }
        }
        unlock(&src, CVPixelBufferLockFlags::empty());
        let turned = turn_surface(&src, 1, false).expect("turns a bi-planar buffer");
        assert_eq!(
            (
                CVPixelBufferGetWidth(&turned),
                CVPixelBufferGetHeight(&turned)
            ),
            (2, 4)
        );
        assert!(lock(&turned, CVPixelBufferLockFlags::ReadOnly));
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
        unlock(&turned, CVPixelBufferLockFlags::ReadOnly);
    }
}
