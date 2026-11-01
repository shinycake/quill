//! macOS HDR stills: ImageIO decodes the picture to its HDR rendition
//! (gain-map HEIC / JPEG / AVIF, PQ and HLG files) into a half-float
//! IOSurface buffer in linear extended sRGB, which the Metal renderer
//! paints with `hdr_surface_fragment`. The window's layer is switched to a
//! float, extended-range drawable while one shows.
//!
//! The decode options (`kCGImageSourceDecodeRequest`, `DecodeToHDR`) are
//! macOS 14 symbols, which is Quill's minimum system.

use super::{HDR_MAX_PIXELS, HdrPixels, Headroom};
use crate::ui::image_decode::{DecodeError, area_allowed};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGImageComponentInfo, kCGColorSpaceExtendedLinearSRGB,
};
use objc2_core_video::{
    CVAttachmentMode, CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelFormatType_64RGBAHalf, kCVReturnSuccess,
};
use objc2_image_io::{
    CGImageSource, kCGImageAuxiliaryDataTypeHDRGainMap, kCGImagePropertyPixelHeight,
    kCGImagePropertyPixelWidth, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceDecodeRequest,
    kCGImageSourceDecodeToHDR, kCGImageSourceShouldAllowFloat,
    kCGImageSourceShouldCacheImmediately, kCGImageSourceThumbnailMaxPixelSize,
};
use std::path::Path;

/// The pixel buffer attachment the renderer reads the display headroom
/// from (see `hdr_headroom` in the vendored Metal renderer).
const HEADROOM_KEY: &str = "QuillHdrHeadroom";

/// The brightest channel must clear SDR white by this much before a decode
/// counts as HDR (rounding in the half-float drawing lands near 1.0).
const HDR_PEAK_MIN: f32 = 1.01;

/// A decoded HDR still's GPU buffer.
pub(crate) struct Frame {
    buffer: CFRetained<CVPixelBuffer>,
}

// SAFETY: a CVPixelBuffer is a CoreFoundation object with atomic retain
// counts; once the decode returns, nothing writes its pixels, and setting
// an attachment goes through CVBuffer's own locking.
unsafe impl Send for Frame {}

impl Frame {
    pub(crate) fn buffer(&self) -> CFRetained<CVPixelBuffer> {
        self.buffer.clone()
    }

    pub(crate) fn set_headroom(&self, headroom: f32) {
        let key = CFString::from_static_str(HEADROOM_KEY);
        let value = CFNumber::new_f32(headroom);
        // SAFETY: the attachment is a CFNumber, which is what the renderer
        // reads under this key.
        unsafe {
            self.buffer
                .set_attachment(&key, &value, CVAttachmentMode::ShouldNotPropagate);
        }
    }
}

fn open(path: &Path) -> Result<CFRetained<CGImageSource>, DecodeError> {
    let url = CFURL::from_file_path(path).ok_or(DecodeError::Unsupported)?;
    // SAFETY: `url` is a live file URL; no options dictionary is passed.
    let source = unsafe { CGImageSource::with_url(&url, None) }.ok_or(DecodeError::Unsupported)?;
    // SAFETY: `source` is a live image source.
    if unsafe { source.count() } == 0 {
        return Err(DecodeError::Unsupported);
    }
    Ok(source)
}

/// Whether the file carries HDR data: a gain map (Apple's or ISO 21496-1)
/// or a PQ / HLG colour space. Reads metadata only; no pixels decode.
/// `None` when the file can't be opened as a picture.
pub(crate) fn probe(path: &Path) -> Option<bool> {
    let source = open(path).ok()?;
    // SAFETY: `source` is a live image source.
    let index = unsafe { source.primary_image_index() };
    // `kCGImageAuxiliaryDataTypeISOGainMap` is a macOS 15 symbol; linking
    // it would keep Quill from launching on 14, so its value is spelled
    // out (an older ImageIO answers "no such data").
    let iso_gain_map = CFString::from_static_str("kCGImageAuxiliaryDataTypeISOGainMap");
    // SAFETY: reading a framework constant.
    let gain_maps = [
        unsafe { kCGImageAuxiliaryDataTypeHDRGainMap },
        &*iso_gain_map,
    ];
    if gain_maps.into_iter().any(|kind| {
        // SAFETY: a live source and a CFString key.
        unsafe { source.auxiliary_data_info_at_index(index, kind) }.is_some()
    }) {
        return Some(true);
    }
    // Without `ShouldCacheImmediately` the image is lazy: only its header
    // and colour space are read here.
    // SAFETY: a live source; no options dictionary is passed.
    let hdr_space = unsafe { source.image_at_index(index, None) }
        .and_then(|image| CGImage::color_space(Some(&image)))
        .is_some_and(|space| space.uses_itur_2100_tf());
    Some(hdr_space)
}

/// Decode `path` to its HDR rendition with a long edge of at most
/// `long_edge` pixels (and at most `HDR_MAX_PIXELS` in all). `Ok(None)`
/// when the picture has nothing above SDR white.
pub(crate) fn decode(path: &Path, long_edge: u32) -> Result<Option<HdrPixels>, DecodeError> {
    let source = open(path)?;
    // SAFETY: `source` is a live image source.
    let index = unsafe { source.primary_image_index() };
    let (width, height) = natural_size(&source, index)?;
    area_allowed(width, height)?;
    let long_edge = capped_long_edge((width, height), long_edge);
    let max_pixels = CFNumber::new_i64(i64::from(long_edge));
    let yes = CFBoolean::new(true);
    // SAFETY: reading framework constants.
    let (keys, hdr): ([&CFString; 6], &CFString) = unsafe {
        (
            [
                kCGImageSourceCreateThumbnailFromImageAlways,
                kCGImageSourceCreateThumbnailWithTransform,
                kCGImageSourceShouldCacheImmediately,
                kCGImageSourceShouldAllowFloat,
                kCGImageSourceThumbnailMaxPixelSize,
                kCGImageSourceDecodeRequest,
            ],
            kCGImageSourceDecodeToHDR,
        )
    };
    let values: [&CFType; 6] = [
        yes.as_ref(),
        yes.as_ref(),
        yes.as_ref(),
        yes.as_ref(),
        max_pixels.as_ref(),
        hdr.as_ref(),
    ];
    let options = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: `options` has the CFString keys and CFBoolean / CFNumber /
    // CFString values the thumbnail call documents.
    let image = unsafe { source.thumbnail_at_index(index, Some(options.as_opaque())) }
        .ok_or_else(|| DecodeError::Failed("ImageIO could not decode the picture".into()))?;
    // An 8-bit result is the SDR picture: ImageIO found no HDR rendition.
    if CGImage::bits_per_component(Some(&image)) <= 8 {
        return Ok(None);
    }
    draw_half_float(&image)
}

fn natural_size(source: &CGImageSource, index: usize) -> Result<(u32, u32), DecodeError> {
    // SAFETY: a live source; no options dictionary is passed.
    let properties =
        unsafe { source.properties_at_index(index, None) }.ok_or(DecodeError::Unsupported)?;
    // SAFETY: ImageIO's property dictionaries have CFString keys.
    let properties = unsafe { properties.cast_unchecked::<CFString, CFType>() };
    // SAFETY: reading framework constants.
    let (width_key, height_key) =
        unsafe { (kCGImagePropertyPixelWidth, kCGImagePropertyPixelHeight) };
    let count = |key: &CFString| {
        properties
            .get(key)
            .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64))
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(0)
    };
    Ok((count(width_key), count(height_key)))
}

/// The long edge to decode at: the asked one, never above the picture's
/// own, and small enough that the frame stays within `HDR_MAX_PIXELS`.
pub(super) fn capped_long_edge((width, height): (u32, u32), asked: u32) -> u32 {
    let natural = width.max(height);
    let area = u64::from(width) * u64::from(height);
    // 2% under the cap: ImageIO may round the decoded size up.
    let budget = HDR_MAX_PIXELS as f64 * 0.98;
    let by_area = if area as f64 > budget {
        let scale = (budget / area as f64).sqrt();
        (f64::from(natural) * scale).floor() as u32
    } else {
        natural
    };
    asked.min(by_area).max(1)
}

/// Draw `image` into a Metal-compatible `64RGBAHalf` pixel buffer in
/// linear extended sRGB (premultiplied, as the shader expects) and find
/// its brightest channel.
fn draw_half_float(image: &CFRetained<CGImage>) -> Result<Option<HdrPixels>, DecodeError> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let (Ok(w32), Ok(h32)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err(DecodeError::TooLarge);
    };
    if u64::from(w32) * u64::from(h32) > HDR_MAX_PIXELS {
        return Err(DecodeError::TooLarge);
    }
    let buffer = pixel_buffer(width, height)?;
    // SAFETY: locking a live buffer; unlocked below with the same flags.
    if unsafe { CVPixelBufferLockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) }
        != kCVReturnSuccess
    {
        return Err(DecodeError::Failed(
            "could not lock the pixel buffer".into(),
        ));
    }
    let drawn = draw_locked(image, &buffer, width, height);
    // SAFETY: locked above with these flags.
    unsafe { CVPixelBufferUnlockBaseAddress(&buffer, CVPixelBufferLockFlags::empty()) };
    let peak = drawn?;
    if peak < HDR_PEAK_MIN {
        return Ok(None);
    }
    Ok(Some(HdrPixels {
        frame: Frame { buffer },
        size: (w32, h32),
        peak,
    }))
}

/// A `64RGBAHalf` pixel buffer backed by an IOSurface and usable by Metal
/// (what the renderer's surface path draws).
fn pixel_buffer(width: usize, height: usize) -> Result<CFRetained<CVPixelBuffer>, DecodeError> {
    let surface_props = CFDictionary::<CFType, CFType>::empty();
    // SAFETY: reading framework constants.
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
            kCVPixelFormatType_64RGBAHalf,
            Some(options.as_opaque()),
            std::ptr::NonNull::from(&mut raw),
        )
    };
    let created = std::ptr::NonNull::new(raw).filter(|_| status == kCVReturnSuccess);
    // SAFETY: on success `raw` holds an owned (+1) pixel buffer.
    created
        .map(|raw| unsafe { CFRetained::from_raw(raw) })
        .ok_or_else(|| DecodeError::Failed(format!("CVPixelBufferCreate failed ({status})")))
}

/// Draws into the locked `buffer` and returns the brightest channel.
fn draw_locked(
    image: &CFRetained<CGImage>,
    buffer: &CVPixelBuffer,
    width: usize,
    height: usize,
) -> Result<f32, DecodeError> {
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    if stride < width.saturating_mul(8) {
        return Err(DecodeError::Failed("pixel buffer rows too short".into()));
    }
    // The buffer is locked, so its base address is valid for
    // `stride * height` bytes until the caller unlocks it.
    let base = CVPixelBufferGetBaseAddress(buffer);
    if base.is_null() {
        return Err(DecodeError::Failed("pixel buffer has no memory".into()));
    }
    // SAFETY: reading a framework constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceExtendedLinearSRGB }))
        .ok_or_else(|| DecodeError::Failed("no extended linear sRGB colour space".into()))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0
        | CGImageComponentInfo::Float.0
        | CGImageByteOrderInfo::Order16Little.0;
    {
        // SAFETY: `base` holds `stride * height` bytes for the lifetime of
        // the context, which is dropped at the end of this block.
        let context =
            unsafe { CGBitmapContextCreate(base, width, height, 16, stride, Some(&space), info) }
                .ok_or_else(|| DecodeError::Failed("no half-float bitmap context".into()))?;
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(&context), rect, Some(image));
    }
    // SAFETY: as above; the context is gone, so nothing else touches the
    // memory while it is read as `u16` halves (the base address of a
    // CVPixelBuffer is at least 16-byte aligned).
    let halves = unsafe { std::slice::from_raw_parts(base.cast::<u16>(), stride / 2 * height) };
    Ok(peak_channel(halves, stride / 2, width))
}

/// The brightest red, green or blue value over `width` RGBA pixels per row
/// of `row_len` halves. Premultiplied values are compared as they are:
/// a dim translucent pixel never reads brighter than an opaque one.
/// Positive finite halves order like their bit patterns, so the scan
/// compares bits and converts once.
pub(super) fn peak_channel(halves: &[u16], row_len: usize, width: usize) -> f32 {
    // Sign clear and exponent below all-ones: positive and finite.
    const POSITIVE_FINITE_END: u16 = 0x7c00;
    let mut peak = 0_u16;
    for row in halves.chunks(row_len.max(1)) {
        for [red, green, blue, _alpha] in row.as_chunks::<4>().0.iter().take(width) {
            for &channel in [red, green, blue] {
                if channel < POSITIVE_FINITE_END && channel > peak {
                    peak = channel;
                }
            }
        }
    }
    half_to_f32(peak)
}

pub(super) use super::half_float::half_to_f32;

/// The display headroom of the screen the window is on: what it shows
/// now and what it could show.
pub(crate) fn headroom(window: &gpui_kit::Window) -> Headroom {
    with_view(window, |view| {
        let screen = view.window().and_then(|window| window.screen())?;
        Some(Headroom {
            current: screen.maximumExtendedDynamicRangeColorComponentValue() as f32,
            potential: screen.maximumPotentialExtendedDynamicRangeColorComponentValue() as f32,
        })
    })
    .unwrap_or(Headroom::SDR)
}

/// Switch the window's backing `CAMetalLayer` to (or back from) a float,
/// extended-range drawable. The renderer follows the drawable's format on
/// its next frame.
pub(crate) fn set_window_edr(window: &gpui_kit::Window, on: bool) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    // MTLPixelFormatRGBA16Float and MTLPixelFormatBGRA8Unorm.
    const RGBA16_FLOAT: usize = 115;
    const BGRA8_UNORM: usize = 80;
    with_view(window, |view| {
        // SAFETY: GPUI's view is layer-backed by its renderer's
        // CAMetalLayer; `layer` returns it unretained, valid while the view
        // lives (this call's duration).
        let layer: *mut AnyObject = unsafe { msg_send![view, layer] };
        // SAFETY: as above; a null layer is skipped.
        let layer = unsafe { layer.as_ref() }?;
        let space = if on {
            // SAFETY: reading a framework constant.
            CGColorSpace::with_name(Some(unsafe {
                objc2_core_graphics::kCGColorSpaceExtendedSRGB
            }))
        } else {
            None
        };
        let space_ptr: *const CGColorSpace = space
            .as_deref()
            .map_or(std::ptr::null(), std::ptr::from_ref);
        // SAFETY: CAMetalLayer's pixelFormat, colorspace and
        // wantsExtendedDynamicRangeContent setters; the colour space (or
        // nil, GPUI's default) is retained by the layer.
        unsafe {
            let _: () =
                msg_send![layer, setPixelFormat: if on { RGBA16_FLOAT } else { BGRA8_UNORM }];
            let _: () = msg_send![layer, setColorspace: space_ptr];
            let _: () = msg_send![layer, setWantsExtendedDynamicRangeContent: objc2::runtime::Bool::new(on)];
        }
        Some(())
    });
}

/// Whether the window's layer is in extended-range mode now. The renderer
/// turns it off on its own when it can't draw in it.
pub(crate) fn window_edr(window: &gpui_kit::Window) -> Option<bool> {
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, Bool};
    with_view(window, |view| {
        // SAFETY: as in `set_window_edr`.
        let layer: *mut AnyObject = unsafe { msg_send![view, layer] };
        // SAFETY: as above; a null layer is skipped.
        let layer = unsafe { layer.as_ref() }?;
        // SAFETY: CAMetalLayer's wantsExtendedDynamicRangeContent getter.
        let on: Bool = unsafe { msg_send![layer, wantsExtendedDynamicRangeContent] };
        Some(on.as_bool())
    })
}

fn with_view<T>(
    window: &gpui_kit::Window,
    f: impl FnOnce(&objc2_app_kit::NSView) -> Option<T>,
) -> Option<T> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI owns this NSView; its raw handle is valid during the
    // render callback this runs in.
    let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
    f(view)
}
