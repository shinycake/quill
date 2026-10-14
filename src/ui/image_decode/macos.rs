//! ImageIO: HEIC / HEIF, AVIF (macOS 13+), JPEG XL (macOS 14+), camera RAW,
//! PSD and the other formats macOS reads. Pixels come back drawn into an
//! sRGB bitmap, so the file's colour profile (Display P3 from an iPhone,
//! Adobe RGB, wide-gamut AVIF) is converted by ColorSync, and the
//! thumbnail call applies the EXIF / HEIF orientation. Gain-map files give
//! their SDR base image here.
//!
//! Only APIs available since macOS 10.x are called: the HDR decode options
//! are macOS 14+ symbols and would keep Quill from launching on older
//! systems if linked here.

use super::{DecodeError, DecodeSize, area_allowed, unpremultiply};
use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect,
    CGSize,
};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, kCGColorSpaceSRGB,
};
use objc2_image_io::{
    CGImageSource, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceShouldCacheImmediately, kCGImageSourceThumbnailMaxPixelSize,
};

pub(super) fn decode(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    let data = CFData::from_bytes(bytes);
    // SAFETY: `data` is a live CFData; no options dictionary is passed.
    let source =
        unsafe { CGImageSource::with_data(&data, None) }.ok_or(DecodeError::Unsupported)?;
    // SAFETY: `source` is a live image source.
    if unsafe { source.count() } == 0 {
        return Err(DecodeError::Unsupported);
    }
    // SAFETY: as above.
    let index = unsafe { source.primary_image_index() };
    // SAFETY: as above; no options dictionary is passed.
    let properties =
        unsafe { source.properties_at_index(index, None) }.ok_or(DecodeError::Unsupported)?;
    // SAFETY: ImageIO's property dictionaries have CFString keys.
    let properties = unsafe { properties.cast_unchecked::<CFString, CFType>() };
    // SAFETY: reading framework constants.
    let (width_key, height_key) =
        unsafe { (kCGImagePropertyPixelWidth, kCGImagePropertyPixelHeight) };
    let width = pixel_count(properties, width_key);
    let height = pixel_count(properties, height_key);
    area_allowed(width, height)?;
    let long_edge = size.long_edge((width, height));
    let max_pixels = CFNumber::new_i64(i64::from(long_edge));
    let yes = CFBoolean::new(true);
    // SAFETY: reading framework constants.
    let keys: [&CFString; 4] = unsafe {
        [
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceShouldCacheImmediately,
            kCGImageSourceThumbnailMaxPixelSize,
        ]
    };
    let values: [&CFType; 4] = [
        yes.as_ref(),
        yes.as_ref(),
        yes.as_ref(),
        max_pixels.as_ref(),
    ];
    let options = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    // SAFETY: `options` has the CFString keys and CFBoolean / CFNumber values
    // the thumbnail call documents.
    let image = unsafe { source.thumbnail_at_index(index, Some(options.as_opaque())) }
        .ok_or_else(|| DecodeError::Failed("ImageIO could not decode the picture".into()))?;
    draw_srgb(&image)
}

fn pixel_count(properties: &CFDictionary<CFString, CFType>, key: &CFString) -> u32 {
    properties
        .get(key)
        .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i64))
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

/// Draw `image` into an 8-bit sRGB RGBA bitmap (ColorSync converts from the
/// image's colour space) and return its straight-alpha pixels.
fn draw_srgb(image: &CFRetained<CGImage>) -> Result<image::RgbaImage, DecodeError> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    let (Ok(w32), Ok(h32)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err(DecodeError::TooLarge);
    };
    area_allowed(w32, h32)?;
    let stride = width.checked_mul(4).ok_or(DecodeError::TooLarge)?;
    let len = stride.checked_mul(height).ok_or(DecodeError::TooLarge)?;
    let mut pixels = vec![0_u8; len];
    // SAFETY: reading a framework constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))
        .ok_or_else(|| DecodeError::Failed("no sRGB colour space".into()))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    {
        // SAFETY: `pixels` holds `stride * height` bytes and outlives the
        // context, which is dropped at the end of this block.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                stride,
                Some(&space),
                info,
            )
        }
        .ok_or_else(|| DecodeError::Failed("no bitmap context".into()))?;
        let rect = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(width as f64, height as f64),
        );
        CGContext::draw_image(Some(&context), rect, Some(image));
    }
    unpremultiply(&mut pixels);
    image::RgbaImage::from_raw(w32, h32, pixels)
        .ok_or_else(|| DecodeError::Failed("bitmap size mismatch".into()))
}
