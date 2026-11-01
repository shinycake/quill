//! A live frame as the picture the element draws, converted once:
//!
//! - macOS: a `CVPixelBuffer` from a `CVPixelBufferPool` (full-range
//!   NV12, the one YUV format GPUI's Metal surface path draws), filled by
//!   copying the Y plane and interleaving U and V (`i420_to_nv12`), and
//!   painted through `surface()` like AVPlayer's pictures. The pool is
//!   made again when the frame size changes.
//! - Windows and Linux: an NV12 `SurfaceFrame` packed from the planes (the
//!   same `i420_to_nv12` interleave) into a pooled buffer
//!   (`live_surface.rs`), uploaded and converted to RGB by the vendored
//!   DirectX or wgpu renderer. A frame the renderer refuses is shown as a
//!   BGRA image instead.
//! - With `QUILL_LIVE_BGRA=1` (demo-capture builds): one `i420_to_bgra`
//!   into a `RenderImage`.
//!
//! Frames the sender turned (a phone camera) are turned in the plane copy
//! first (`rotate_planes`), and a mirrored feed (your camera preview) is
//! mirrored there too (`mirror_planes`), so surfaces need no turn of their
//! own.
//! [`convert`] runs on any thread; the pool buffer is taken on the UI
//! thread ([`Surfaces::target`]) and the image is made there too
//! ([`Prepared::into_picture`]).
#![deny(clippy::indexing_slicing)]

use super::Picture;
use gpui_kit::RenderImage;
use quill::calls::live_video::LiveFrame;
use quill::calls::live_video::convert::{I420, i420_to_bgra, mirror_planes, rotate_planes};
use smallvec::SmallVec;
use std::sync::Arc;

/// Where a frame is converted into.
pub(super) enum Target {
    /// A pixel buffer fresh from the pool.
    #[cfg(target_os = "macos")]
    Surface(SendBuffer),
    /// A buffer for a packed NV12 frame (Windows, Linux), from the backend's
    /// pool; no frame holds it.
    #[cfg(any(not(target_os = "macos"), test))]
    Nv12(Arc<[u8]>),
    /// A new BGRA buffer.
    Bgra,
}

/// A converted frame, made into a [`Picture`] on the UI thread.
pub(super) enum Prepared {
    #[cfg(target_os = "macos")]
    Surface(SendBuffer),
    #[cfg(any(not(target_os = "macos"), test))]
    Nv12(super::live_surface::Nv12Picture),
    Bgra {
        bgra: Vec<u8>,
        width: u32,
        height: u32,
    },
}

/// A pooled pixel buffer on its way to the converting thread and back.
#[cfg(target_os = "macos")]
pub(super) struct SendBuffer(
    pub(super) objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>,
);

// SAFETY: a CVPixelBuffer is a CoreFoundation object whose retain and
// release are thread-safe. The buffer comes fresh from the pool, so only
// the thread holding this wrapper reads or writes its memory (between a
// lock and an unlock of its base address) until it is handed back to the
// UI thread to be drawn.
#[cfg(target_os = "macos")]
unsafe impl Send for SendBuffer {}

/// The frame's planes as shown: turned when the sender turned them, then
/// mirrored when the feed is.
fn upright<'a>(
    frame: &'a LiveFrame,
    mirrored: bool,
    scratch: &'a mut Vec<u8>,
) -> Option<(&'a [u8], u16, u16)> {
    let src = frame.i420()?;
    let turned = rotate_planes(&src, frame.rotation, scratch);
    let (width, height) = turned.unwrap_or((frame.width, frame.height));
    if mirrored {
        if turned.is_none() {
            scratch.clear();
            scratch.extend_from_slice(&frame.planes);
        }
        mirror_planes(scratch, width, height);
    }
    let planes = if turned.is_some() || mirrored {
        scratch.as_slice()
    } else {
        frame.planes.as_slice()
    };
    Some((planes, width, height))
}

/// Convert `frame` into `target`, mirrored when `mirrored`; a pixel
/// buffer or NV12 buffer that can't be filled falls back to BGRA. `None`
/// for a malformed frame.
pub(super) fn convert(
    frame: &LiveFrame,
    target: Target,
    mirrored: bool,
    scratch: &mut Vec<u8>,
) -> Option<Prepared> {
    let (planes, width, height) = upright(frame, mirrored, scratch)?;
    let src = I420::new(planes, width, height)?;
    match target {
        #[cfg(target_os = "macos")]
        Target::Surface(buffer) => {
            if fill_nv12(&buffer.0, &src) {
                return Some(Prepared::Surface(buffer));
            }
        }
        #[cfg(any(not(target_os = "macos"), test))]
        Target::Nv12(bytes) => {
            if let Some(picture) = super::live_surface::fill(&src, bytes) {
                return Some(Prepared::Nv12(picture));
            }
        }
        Target::Bgra => {}
    }
    Some(Prepared::Bgra {
        bgra: i420_to_bgra(&src)?,
        width: u32::from(width),
        height: u32::from(height),
    })
}

/// A converted frame made into what the element draws.
pub(super) struct Shown {
    pub(super) picture: Picture,
    /// The image it made, for the backend to retire.
    pub(super) image: Option<Arc<RenderImage>>,
    /// The NV12 frame and its buffer (Windows, Linux), for the backend to keep.
    #[cfg(any(not(target_os = "macos"), test))]
    pub(super) nv12: Option<super::live_surface::Nv12Shown>,
}

impl Shown {
    fn new(picture: Picture, image: Option<Arc<RenderImage>>) -> Self {
        Self {
            picture,
            image,
            #[cfg(any(not(target_os = "macos"), test))]
            nv12: None,
        }
    }
}

impl Prepared {
    /// The picture, and what the backend keeps of it.
    pub(super) fn into_picture(self) -> Option<Shown> {
        match self {
            #[cfg(target_os = "macos")]
            Prepared::Surface(buffer) => Some(Shown::new(Picture::Surface(buffer.0), None)),
            #[cfg(any(not(target_os = "macos"), test))]
            Prepared::Nv12(picture) => {
                let (picture, image, nv12) = picture.into_shown()?;
                Some(Shown {
                    picture,
                    image,
                    nv12: Some(nv12),
                })
            }
            Prepared::Bgra {
                bgra,
                width,
                height,
            } => {
                let image = render_image(bgra, width, height)?;
                Some(Shown::new(Picture::Image(image.clone()), Some(image)))
            }
        }
    }

    /// The `QUILL_TRACE_VIDEO` `VF` columns: the layout, its size and the
    /// bytes the renderer uploads.
    pub(super) fn trace(&self) -> (&'static str, u32, u32, usize) {
        match self {
            #[cfg(target_os = "macos")]
            Prepared::Surface(buffer) => {
                let (width, height) = super::surface::surface_size(&buffer.0);
                let luma = (width as usize).saturating_mul(height as usize);
                ("nv12", width, height, luma.saturating_add(luma / 2))
            }
            #[cfg(any(not(target_os = "macos"), test))]
            Prepared::Nv12(picture) => {
                let (width, height) = picture.size();
                ("nv12", width, height, picture.byte_len())
            }
            Prepared::Bgra {
                bgra,
                width,
                height,
            } => ("bgra", *width, *height, bgra.len()),
        }
    }
}

/// BGRA bytes as a GPUI image.
pub(super) fn render_image(bgra: Vec<u8>, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let image = image::RgbaImage::from_raw(width, height, bgra)?;
    let image = Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
        image,
    )])));
    #[cfg(test)]
    MADE.with(|made| {
        if let Ok(mut made) = made.try_borrow_mut() {
            made.push(Arc::downgrade(&image));
        }
    });
    Some(image)
}

#[cfg(test)]
thread_local! {
    /// Every image made on this thread, for tests of their lifetime.
    static MADE: std::cell::RefCell<Vec<std::sync::Weak<RenderImage>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The images made since the last call, for tests.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(crate) fn take_made_images() -> Vec<std::sync::Weak<RenderImage>> {
    MADE.with(|made| std::mem::take(&mut *made.borrow_mut()))
}

/// Whether every picture is BGRA: with `QUILL_LIVE_BGRA=1` in a
/// demo-capture build, to run the image path (its memory, its atlas).
pub(crate) fn bgra_only() -> bool {
    #[cfg(feature = "demo-capture")]
    {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *ON.get_or_init(|| std::env::var_os("QUILL_LIVE_BGRA").is_some()) {
            return true;
        }
    }
    false
}

/// The pixel-buffer pool (macOS); nothing elsewhere.
#[derive(Default)]
pub(super) struct Surfaces {
    #[cfg(target_os = "macos")]
    pool: Option<(
        (u32, u32),
        objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBufferPool>,
    )>,
    /// A pool that could not be made, for that size and since then: tried
    /// again after [`POOL_RETRY`], not on every frame.
    #[cfg(target_os = "macos")]
    failed: Option<((u32, u32), std::time::Instant)>,
}

/// How long a size whose pool failed converts to BGRA before trying again.
#[cfg(target_os = "macos")]
const POOL_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

impl Surfaces {
    /// What a frame shown at `width × height` converts into: a buffer
    /// from the pool (made again for a new size), else BGRA.
    #[cfg(target_os = "macos")]
    pub(super) fn target(&mut self, width: u32, height: u32) -> Target {
        if bgra_only() {
            return Target::Bgra;
        }
        let size = (width, height);
        let backing_off = self
            .failed
            .is_some_and(|(failed, at)| failed == size && at.elapsed() < POOL_RETRY);
        if !backing_off && self.pool.as_ref().is_none_or(|(made, _)| *made != size) {
            self.pool = new_pool(width, height).map(|pool| (size, pool));
            self.failed = match self.pool {
                Some(_) => None,
                None => Some((size, std::time::Instant::now())),
            };
        }
        if backing_off {
            return Target::Bgra;
        }
        self.pool
            .as_ref()
            .and_then(|(_, pool)| pool_buffer(pool))
            .map_or(Target::Bgra, |buffer| Target::Surface(SendBuffer(buffer)))
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn target(&mut self, _width: u32, _height: u32) -> Target {
        Target::Bgra
    }
}

/// A pool of IOSurface-backed, Metal-compatible full-range NV12 buffers
/// of one size (what GPUI's surface path draws).
#[cfg(target_os = "macos")]
fn new_pool(
    width: u32,
    height: u32,
) -> Option<objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBufferPool>> {
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
    use objc2_core_video::{
        CVPixelBufferPool, kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey,
        kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
        kCVPixelBufferWidthKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
    };

    let format =
        CFNumber::new_i32(i32::try_from(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange).ok()?);
    let width = CFNumber::new_i32(i32::try_from(width).ok()?);
    let height = CFNumber::new_i32(i32::try_from(height).ok()?);
    let surface_props = CFDictionary::<CFType, CFType>::empty();
    // SAFETY: CoreVideo's attribute keys are constant CFStrings that live
    // for the whole process.
    let keys: [&CFString; 5] = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey,
            kCVPixelBufferWidthKey,
            kCVPixelBufferHeightKey,
            kCVPixelBufferIOSurfacePropertiesKey,
            kCVPixelBufferMetalCompatibilityKey,
        ]
    };
    let values: [&CFType; 5] = [
        &format,
        &width,
        &height,
        &surface_props,
        CFBoolean::new(true),
    ];
    let attributes = CFDictionary::from_slices(&keys, &values);
    let mut raw = std::ptr::null_mut();
    // SAFETY: `attributes` maps CoreVideo's CFString keys to CF values of
    // the types they take, and `raw` is a valid out pointer.
    let status = unsafe {
        CVPixelBufferPool::create(
            None,
            None,
            Some(attributes.as_opaque()),
            std::ptr::NonNull::from(&mut raw),
        )
    };
    if status != kCVReturnSuccess {
        return None;
    }
    // SAFETY: on success `raw` holds an owned (+1) pool.
    std::ptr::NonNull::new(raw).map(|raw| unsafe { CFRetained::from_raw(raw) })
}

/// A buffer from `pool`, or `None` when CoreVideo has none to give.
#[cfg(target_os = "macos")]
fn pool_buffer(
    pool: &objc2_core_video::CVPixelBufferPool,
) -> Option<objc2_core_foundation::CFRetained<objc2_core_video::CVPixelBuffer>> {
    use objc2_core_foundation::CFRetained;
    use objc2_core_video::{CVPixelBufferPool, kCVReturnSuccess};

    let mut raw = std::ptr::null_mut();
    // SAFETY: `pool` is a live pool and `raw` a valid out pointer.
    let status = unsafe {
        CVPixelBufferPool::create_pixel_buffer(None, pool, std::ptr::NonNull::from(&mut raw))
    };
    if status != kCVReturnSuccess {
        return None;
    }
    // SAFETY: on success `raw` holds an owned (+1) pixel buffer.
    std::ptr::NonNull::new(raw).map(|raw| unsafe { CFRetained::from_raw(raw) })
}

/// Copy `src` into the NV12 `buffer`; `false` (nothing shown from it)
/// when the buffer can't be locked or is not the frame's size.
#[cfg(target_os = "macos")]
fn fill_nv12(buffer: &objc2_core_video::CVPixelBuffer, src: &I420) -> bool {
    use objc2_core_video::{
        CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
        CVPixelBufferGetHeight, CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPlaneCount,
        CVPixelBufferGetWidth, CVPixelBufferLockFlags,
    };
    use quill::calls::live_video::convert::i420_to_nv12;

    if CVPixelBufferGetPlaneCount(buffer) != 2
        || CVPixelBufferGetWidth(buffer) != src.width
        || CVPixelBufferGetHeight(buffer) != src.height
    {
        return false;
    }
    if !super::rotate::lock(buffer, CVPixelBufferLockFlags::empty()) {
        return false;
    }
    let plane = |index: usize| {
        let base = CVPixelBufferGetBaseAddressOfPlane(buffer, index).cast::<u8>();
        let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
        let rows = CVPixelBufferGetHeightOfPlane(buffer, index);
        (base, stride, rows)
    };
    let ((y_base, y_stride, y_rows), (uv_base, uv_stride, uv_rows)) = (plane(0), plane(1));
    let filled = if y_base.is_null() || uv_base.is_null() {
        false
    } else {
        // SAFETY: the buffer is locked, so each plane's base address is
        // valid for `bytes_per_row × height` bytes, and the two planes do
        // not overlap. `i420_to_nv12` checks every row against these
        // lengths before it writes.
        unsafe {
            let y = std::slice::from_raw_parts_mut(y_base, y_stride * y_rows);
            let uv = std::slice::from_raw_parts_mut(uv_base, uv_stride * uv_rows);
            i420_to_nv12(src, y, y_stride, uv, uv_stride)
        }
    };
    super::rotate::unlock(buffer, CVPixelBufferLockFlags::empty());
    filled
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill::calls::live_video::LiveRotation;
    use quill::calls::live_video::convert::gradient_frame;
    use std::time::Instant;

    fn frame(width: u16, height: u16, rotation: LiveRotation) -> LiveFrame {
        LiveFrame::new(
            1,
            width,
            height,
            rotation,
            gradient_frame(width, height, 0),
            Instant::now(),
        )
        .expect("frame")
    }

    #[test]
    fn a_turned_frame_is_shown_with_its_sides_swapped() {
        let Some(Prepared::Bgra {
            width,
            height,
            bgra,
        }) = convert(
            &frame(64, 36, LiveRotation::R90),
            Target::Bgra,
            false,
            &mut Vec::new(),
        )
        else {
            panic!("bgra");
        };
        assert_eq!((width, height), (36, 64));
        assert_eq!(bgra.len(), 36 * 64 * 4);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_pool_gives_full_range_nv12_of_the_frame_size_and_is_remade_on_resize() {
        use objc2_core_video::{
            CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        };
        let mut surfaces = Surfaces::default();
        let Target::Surface(buffer) = surfaces.target(64, 36) else {
            panic!("a pool buffer");
        };
        assert_eq!(
            CVPixelBufferGetPixelFormatType(&buffer.0),
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        );
        let Some(Prepared::Surface(filled)) = convert(
            &frame(64, 36, LiveRotation::None),
            Target::Surface(buffer),
            false,
            &mut Vec::new(),
        ) else {
            panic!("filled");
        };
        assert_eq!(
            (
                CVPixelBufferGetWidth(&filled.0),
                CVPixelBufferGetHeight(&filled.0)
            ),
            (64, 36)
        );
        let Target::Surface(bigger) = surfaces.target(128, 72) else {
            panic!("a pool buffer");
        };
        assert_eq!(CVPixelBufferGetWidth(&bigger.0), 128);
        // A buffer of the wrong size is not filled: BGRA instead.
        assert!(matches!(
            convert(
                &frame(64, 36, LiveRotation::None),
                Target::Surface(bigger),
                false,
                &mut Vec::new()
            ),
            Some(Prepared::Bgra { .. })
        ));
    }
}
