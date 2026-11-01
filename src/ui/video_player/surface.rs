//! Surface pictures, per platform: an AVPlayer pixel buffer on macOS, and
//! on Windows and Linux an NV12 picture from the FFmpeg decoder that the
//! vendored DirectX or wgpu renderer uploads as two textures and converts
//! to RGB on the GPU (`docs/decisions/video-player-nv12.md`).
//!
//! The NV12 helpers build on every platform (a `SurfaceFrame` is plain
//! data), so their tests run on any machine; macOS doesn't ask the decoder
//! for NV12 ([`NV12_SURFACES`]).
#![deny(clippy::indexing_slicing)]
// The NV12 frame helpers serve the Windows and Linux path.
#![cfg_attr(target_os = "macos", allow(dead_code))]

use super::rotate;
use gpui_kit::{RenderImage, SurfaceFormat, SurfaceFrame};
use quill::video_decode::{Pixels, VideoFrame};
use std::sync::Arc;

/// Whether this platform's renderer draws NV12 surfaces: all but macOS.
/// Each window's renderer then says per frame whether it drew it
/// ([`draw_state`]); a refused frame is shown as an image.
pub(super) const NV12_SURFACES: bool = cfg!(not(target_os = "macos"));

/// The decoder's colour description as the GPUI core's.
pub(super) fn gpui_color(color: quill::video_decode::YuvColor) -> gpui_kit::YuvColor {
    use quill::video_decode::YuvMatrix as Core;
    gpui_kit::YuvColor {
        matrix: match color.matrix {
            Core::Bt601 => gpui_kit::YuvMatrix::Bt601,
            Core::Bt709 => gpui_kit::YuvMatrix::Bt709,
        },
        full_range: color.full_range,
    }
}

/// The GPUI core's colour description as the decoder's.
fn core_color(color: gpui_kit::YuvColor) -> quill::video_decode::YuvColor {
    use quill::video_decode::YuvMatrix as Core;
    quill::video_decode::YuvColor {
        matrix: match color.matrix {
            gpui_kit::YuvMatrix::Bt601 => Core::Bt601,
            gpui_kit::YuvMatrix::Bt709 => Core::Bt709,
        },
        full_range: color.full_range,
    }
}

/// A decoded NV12 picture as a surface frame (its bytes are shared, not
/// copied). The picture comes back when it isn't NV12, or its bytes don't
/// make a valid frame of its size (the caller shows it as an image).
pub(super) fn surface_frame(frame: VideoFrame) -> Result<Arc<SurfaceFrame>, VideoFrame> {
    let Pixels::Nv12 { bytes, color } = &frame.pixels else {
        return Err(frame);
    };
    match SurfaceFrame::new(
        frame.width,
        frame.height,
        SurfaceFormat::Nv12,
        bytes.clone(),
    ) {
        Ok(surface) => Ok(Arc::new(surface.with_yuv(gpui_color(*color)))),
        Err(_) => Err(frame),
    }
}

/// Turn (and mirror) an NV12 frame: the luma plane and the interleaved
/// chroma plane, each as a plain transpose, into a new frame of the
/// turned size. `None` for another format or a malformed frame.
pub(super) fn turn_frame(
    frame: &SurfaceFrame,
    turns: u8,
    mirror: bool,
) -> Option<Arc<SurfaceFrame>> {
    let (luma, chroma) = frame.nv12_planes()?;
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let (tw, th) = rotate::turned_size(w, h, turns);
    let mut out = vec![0u8; luma.len().checked_add(chroma.len())?];
    let (dst_luma, dst_chroma) = out.split_at_mut_checked(luma.len())?;
    rotate::turn_plane(luma, (w, h), w, dst_luma, tw, turns, mirror);
    let (src_pairs, []) = chroma.as_chunks::<2>() else {
        return None;
    };
    let (dst_pairs, []) = dst_chroma.as_chunks_mut::<2>() else {
        return None;
    };
    rotate::turn_plane(
        src_pairs,
        (w / 2, h / 2),
        w / 2,
        dst_pairs,
        tw / 2,
        turns,
        mirror,
    );
    let turned = SurfaceFrame::new(
        u32::try_from(tw).ok()?,
        u32::try_from(th).ok()?,
        SurfaceFormat::Nv12,
        Arc::from(out),
    )
    .ok()?;
    Some(Arc::new(turned.with_yuv(frame.yuv())))
}

/// An NV12 frame converted to a BGRA image on the CPU (Copy Frame, and
/// the picture on screen when the renderer refuses it).
pub(super) fn frame_image(frame: &SurfaceFrame) -> Option<Arc<RenderImage>> {
    if frame.format() != SurfaceFormat::Nv12 {
        return None;
    }
    let bgra = quill::video_decode::nv12_to_bgra(
        frame.bytes(),
        frame.width(),
        frame.height(),
        core_color(frame.yuv()),
    )?;
    let buffer = image::RgbaImage::from_raw(frame.width(), frame.height(), bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(buffer),
    ]))))
}

/// The surface's size in pixels.
#[cfg(target_os = "macos")]
pub(super) fn surface_size(surface: &super::SurfaceHandle) -> (u32, u32) {
    let side = |pixels: usize| u32::try_from(pixels).unwrap_or(u32::MAX);
    (
        side(objc2_core_video::CVPixelBufferGetWidth(surface)),
        side(objc2_core_video::CVPixelBufferGetHeight(surface)),
    )
}

#[cfg(not(target_os = "macos"))]
pub(super) fn surface_size(surface: &super::SurfaceHandle) -> (u32, u32) {
    (surface.width(), surface.height())
}

/// The surface turned (and mirrored); `None` when it can't be.
#[cfg(target_os = "macos")]
pub(super) fn turn_surface(
    surface: &super::SurfaceHandle,
    turns: u8,
    mirror: bool,
) -> Option<super::SurfaceHandle> {
    rotate::turn_surface(surface, turns, mirror)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn turn_surface(
    surface: &super::SurfaceHandle,
    turns: u8,
    mirror: bool,
) -> Option<super::SurfaceHandle> {
    turn_frame(surface, turns, mirror)
}

/// The surface as an image to copy out: none for a GPU buffer (macOS, no
/// readback).
#[cfg(target_os = "macos")]
pub(super) fn snapshot(_surface: &super::SurfaceHandle) -> Option<Arc<RenderImage>> {
    None
}

/// The surface as an image to copy out (converted on the CPU).
#[cfg(not(target_os = "macos"))]
pub(super) fn snapshot(surface: &super::SurfaceHandle) -> Option<Arc<RenderImage>> {
    frame_image(surface)
}

/// What the renderer did with the surface. macOS pixel buffers are always
/// drawn, so they count as drawn.
#[cfg(target_os = "macos")]
pub(super) fn draw_state(_surface: &super::SurfaceHandle) -> gpui_kit::SurfaceDrawState {
    gpui_kit::SurfaceDrawState::Drawn
}

#[cfg(not(target_os = "macos"))]
pub(super) fn draw_state(surface: &super::SurfaceHandle) -> gpui_kit::SurfaceDrawState {
    surface.draw_state()
}

#[cfg(test)]
mod tests {
    use super::{frame_image, gpui_color, surface_frame, turn_frame};
    use gpui_kit::{SurfaceDrawState, SurfaceFormat, SurfaceFrame, SurfaceFrameError};
    use quill::video_decode::{Pixels, VideoFrame, YuvColor, YuvMatrix};
    use std::sync::Arc;

    fn bytes(len: usize) -> Arc<[u8]> {
        Arc::from(vec![0u8; len])
    }

    #[test]
    fn nv12_frames_must_be_even_and_exactly_long() {
        assert_eq!(SurfaceFormat::Nv12.byte_len(4, 2), Ok(12));
        assert_eq!(
            SurfaceFormat::Nv12.byte_len(1920, 1080),
            Ok(1920 * 1080 * 3 / 2)
        );
        for (w, h) in [(3, 2), (4, 3), (1, 1), (1919, 1080)] {
            assert_eq!(
                SurfaceFormat::Nv12.byte_len(w, h),
                Err(SurfaceFrameError::OddSize {
                    width: w,
                    height: h
                })
            );
            assert!(SurfaceFrame::new(w, h, SurfaceFormat::Nv12, bytes(64)).is_err());
        }
        assert_eq!(
            SurfaceFormat::Nv12.byte_len(0, 2),
            Err(SurfaceFrameError::Empty)
        );
        assert!(SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, bytes(12)).is_ok());
        for short_or_long in [0, 8, 11, 13, 16] {
            assert_eq!(
                SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, bytes(short_or_long)).err(),
                Some(SurfaceFrameError::Length {
                    expected: 12,
                    actual: short_or_long
                })
            );
        }
        assert_eq!(
            SurfaceFormat::Nv12
                .byte_len(u32::MAX - 1, u32::MAX - 1)
                .ok(),
            ((u32::MAX - 1) as usize)
                .checked_mul((u32::MAX - 1) as usize)
                .and_then(|luma| luma.checked_add(luma / 2))
        );
    }

    #[test]
    fn rgba16_frames_keep_their_length_rule() {
        assert_eq!(SurfaceFormat::Rgba16Float.byte_len(3, 1), Ok(24));
        // Odd sides are fine for RGBA.
        assert!(SurfaceFrame::new(3, 1, SurfaceFormat::Rgba16Float, bytes(24)).is_ok());
        assert!(SurfaceFrame::new(3, 1, SurfaceFormat::Rgba16Float, bytes(23)).is_err());
        assert_eq!(
            SurfaceFormat::Rgba16Float.byte_len(2, 0),
            Err(SurfaceFrameError::Empty)
        );
    }

    #[test]
    fn a_frame_remembers_what_the_renderer_did_with_it() {
        let frame = SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, bytes(12)).expect("valid");
        assert_eq!(frame.draw_state(), SurfaceDrawState::Pending);
        frame.mark_drawn();
        assert_eq!(frame.draw_state(), SurfaceDrawState::Drawn);
        // Another window refusing it later still counts.
        frame.mark_refused();
        assert_eq!(frame.draw_state(), SurfaceDrawState::Refused);
        // ... and a refusal is not undone by a draw elsewhere: the
        // producer replaces the frame.
        frame.mark_drawn();
        assert_eq!(frame.draw_state(), SurfaceDrawState::Refused);
    }

    #[test]
    fn nv12_planes_split_luma_and_chroma() {
        let pixels: Vec<u8> = (0..12).collect();
        let frame = SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, Arc::from(pixels)).unwrap();
        let (luma, chroma) = frame.nv12_planes().unwrap();
        assert_eq!(luma, [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(chroma, [8, 9, 10, 11]);
        let hdr = SurfaceFrame::new(1, 1, SurfaceFormat::Rgba16Float, bytes(8)).unwrap();
        assert!(hdr.nv12_planes().is_none());
    }

    /// The renderer uploads the core's rows; the CPU fallback computes its
    /// own. They must be the same numbers for all four combinations.
    #[test]
    fn gpu_and_cpu_matrices_agree() {
        for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
            for full_range in [false, true] {
                let color = YuvColor { matrix, full_range };
                assert_eq!(gpui_color(color).rgb_rows(), color.rgb_rows(), "{color:?}");
            }
        }
    }

    fn apply(color: gpui_kit::YuvColor, yuv: [u8; 3]) -> [u8; 3] {
        let [y, cb, cr] = yuv.map(|v| f32::from(v) / 255.0);
        color.rgb_rows().map(|row| {
            let v = row[0] * y + row[1] * cb + row[2] * cr + row[3];
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        })
    }

    fn near(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2)
    }

    #[test]
    fn matrix_and_range_pick_the_right_colors() {
        let bt601 = gpui_kit::YuvColor {
            matrix: gpui_kit::YuvMatrix::Bt601,
            full_range: false,
        };
        let bt709 = gpui_kit::YuvColor {
            matrix: gpui_kit::YuvMatrix::Bt709,
            full_range: false,
        };
        let full709 = gpui_kit::YuvColor {
            matrix: gpui_kit::YuvMatrix::Bt709,
            full_range: true,
        };
        // Limited range: 16 is black, 235 white.
        assert_eq!(apply(bt709, [16, 128, 128]), [0, 0, 0]);
        assert_eq!(apply(bt709, [235, 128, 128]), [255, 255, 255]);
        // Full range: 0 is black, 255 white; 16 is a dark grey.
        assert_eq!(apply(full709, [0, 128, 128]), [0, 0, 0]);
        assert_eq!(apply(full709, [255, 128, 128]), [255, 255, 255]);
        assert_eq!(apply(full709, [16, 128, 128]), [16, 16, 16]);
        // Pure red in each matrix (ITU code values).
        assert!(near(apply(bt601, [81, 90, 240]), [255, 0, 0]));
        assert!(near(apply(bt709, [63, 102, 240]), [255, 0, 0]));
        // The BT.709 red read with the BT.601 matrix is visibly off.
        assert!(!near(apply(bt601, [63, 102, 240]), [255, 0, 0]));
        // Pure green and blue (BT.709).
        assert!(near(apply(bt709, [173, 42, 26]), [0, 255, 0]));
        assert!(near(apply(bt709, [32, 240, 118]), [0, 0, 255]));
    }

    fn nv12_frame(width: u32, height: u32, bytes: Vec<u8>, color: YuvColor) -> VideoFrame {
        VideoFrame {
            pts: 0.0,
            width,
            height,
            pixels: Pixels::Nv12 {
                bytes: Arc::from(bytes),
                color,
            },
            convert_us: 0,
        }
    }

    #[test]
    fn decoded_nv12_becomes_a_surface_and_bgra_does_not() {
        let color = YuvColor {
            matrix: YuvMatrix::Bt601,
            full_range: true,
        };
        let frame = surface_frame(nv12_frame(4, 2, vec![7; 12], color)).unwrap();
        assert_eq!((frame.width(), frame.height()), (4, 2));
        assert_eq!(frame.format(), SurfaceFormat::Nv12);
        assert_eq!(frame.yuv(), gpui_color(color));
        // A BGRA picture, or NV12 bytes that don't fit the size, come back.
        let bgra = VideoFrame::bgra(0.0, 1, 1, vec![0; 4]);
        assert_eq!(surface_frame(bgra.clone()).err(), Some(bgra));
        let short = nv12_frame(4, 2, vec![0; 11], color);
        assert_eq!(surface_frame(short.clone()).err(), Some(short));
    }

    #[test]
    fn a_turn_moves_both_planes() {
        // 4x2 luma A..H, chroma pairs (p, q) for the two 2x2 blocks.
        let pixels = vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 20, 21];
        let frame = SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, Arc::from(pixels)).unwrap();
        let turned = turn_frame(&frame, 1, false).unwrap();
        assert_eq!((turned.width(), turned.height()), (2, 4));
        let (luma, chroma) = turned.nv12_planes().unwrap();
        // Clockwise: the bottom row becomes the left column.
        assert_eq!(luma, [5, 1, 6, 2, 7, 3, 8, 4]);
        // The left block goes on top.
        assert_eq!(chroma, [10, 11, 20, 21]);
        let back = turn_frame(&turned, 3, false).unwrap();
        assert_eq!(back.bytes(), frame.bytes());
        let mirrored = turn_frame(&frame, 0, true).unwrap();
        assert_eq!(mirrored.nv12_planes().unwrap().0, [4, 3, 2, 1, 8, 7, 6, 5]);
    }

    #[test]
    fn a_copied_frame_converts_to_bgra() {
        // Limited BT.709: white luma on the left block, black on the right,
        // neutral chroma.
        let pixels = vec![235, 235, 16, 16, 235, 235, 16, 16, 128, 128, 128, 128];
        let frame = SurfaceFrame::new(4, 2, SurfaceFormat::Nv12, Arc::from(pixels)).unwrap();
        let image = frame_image(&frame).unwrap();
        let bytes = image.as_bytes(0).unwrap();
        assert_eq!(bytes.len(), 4 * 2 * 4);
        assert_eq!(bytes.get(..4), Some(&[255, 255, 255, 255][..]));
        assert_eq!(bytes.get(8..12), Some(&[0, 0, 0, 255][..]));
    }
}
