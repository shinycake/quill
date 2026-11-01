//! The decoders that are the same on every platform: the `image` crate
//! (colour-managed with moxcms), jxl-oxide for JPEG XL, resvg for SVG.

use super::{DecodeError, DecodeSize, area_allowed, contained, unpremultiply};
use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use image::{
    DynamicImage, ImageBuffer, ImageDecoder, ImageFormat, ImageReader, Pixel, metadata::Orientation,
};
use std::io::Cursor;

fn failed(err: impl std::fmt::Display) -> DecodeError {
    DecodeError::Failed(err.to_string())
}

/// PNG, JPEG, WebP, GIF (first frame), BMP, ICO, TIFF, QOI, TGA.
pub(super) fn decode_raster(
    bytes: &[u8],
    format: ImageFormat,
    size: DecodeSize,
) -> Result<image::RgbaImage, DecodeError> {
    contained(|| {
        let reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut decoder = reader.into_decoder().map_err(|err| match err {
            image::ImageError::Unsupported(_) => DecodeError::Unsupported,
            other => failed(other),
        })?;
        let (width, height) = decoder.dimensions();
        area_allowed(width, height)?;
        let icc = decoder.icc_profile().ok().flatten();
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let image = DynamicImage::from_decoder(decoder).map_err(failed)?;
        Ok(finish(image, icc.as_deref(), orientation, size))
    })
}

/// Scale, colour-manage and turn a decoded picture upright.
fn finish(
    image: DynamicImage,
    icc: Option<&[u8]>,
    orientation: Orientation,
    size: DecodeSize,
) -> image::RgbaImage {
    let target = size.fit((image.width(), image.height()));
    let mut image = if image.color().has_alpha() {
        let mut rgba = image.into_rgba8();
        if target != rgba.dimensions() {
            rgba = shrink(&rgba, target);
        }
        if let Some(icc) = icc {
            to_srgb(&mut rgba, moxcms::Layout::Rgba, icc);
        }
        DynamicImage::ImageRgba8(rgba)
    } else {
        // Shrink the three-channel picture and widen only the result (an
        // RGBA copy of a 12 Mpx photo is 48 MB).
        let mut rgb = image.into_rgb8();
        if target != rgb.dimensions() {
            rgb = shrink(&rgb, target);
        }
        if let Some(icc) = icc {
            to_srgb(&mut rgb, moxcms::Layout::Rgb, icc);
        }
        DynamicImage::ImageRgb8(rgb)
    };
    image.apply_orientation(orientation);
    image.into_rgba8()
}

fn shrink<P>(
    image: &ImageBuffer<P, Vec<u8>>,
    (width, height): (u32, u32),
) -> ImageBuffer<P, Vec<u8>>
where
    P: Pixel<Subpixel = u8> + 'static,
{
    let kind = if P::CHANNEL_COUNT == 4 {
        PixelType::U8x4
    } else {
        PixelType::U8x3
    };
    let simd = || {
        let source = ImageRef::new(image.width(), image.height(), image.as_raw(), kind).ok()?;
        let mut shrunk = Image::new(width, height, kind);
        let options = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
            .use_alpha(false);
        Resizer::new().resize(&source, &mut shrunk, &options).ok()?;
        ImageBuffer::from_raw(width, height, shrunk.into_vec())
    };
    simd().unwrap_or_else(|| {
        image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
    })
}

/// Convert pixels in the colour space of `icc` to sRGB in place. A profile
/// moxcms cannot use (damaged, or CMYK / gray for RGB pixels) leaves the
/// pixels as they are, which is what other apps show for such files too.
pub(crate) fn to_srgb(pixels: &mut [u8], layout: moxcms::Layout, icc: &[u8]) {
    let Ok(source) = moxcms::ColorProfile::new_from_slice(icc) else {
        return;
    };
    let target = moxcms::ColorProfile::new_srgb();
    let Ok(transform) =
        source.create_transform_8bit(layout, &target, layout, moxcms::TransformOptions::default())
    else {
        return;
    };
    let mut converted = vec![0_u8; pixels.len()];
    if transform.transform(pixels, &mut converted).is_ok() {
        pixels.copy_from_slice(&converted);
    }
}

/// The most memory jxl-oxide may use for one picture.
const JXL_MEMORY_LIMIT: usize = 1024 * 1024 * 1024;

/// JPEG XL through jxl-oxide, rendered straight to sRGB (it applies the
/// file's orientation itself).
pub(super) fn decode_jxl(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    contained(|| {
        // jxl-oxide works in f32 planes: cap its memory so a huge file fails
        // instead of aborting on allocation.
        let mut image = jxl_oxide::JxlImage::builder()
            .alloc_tracker(jxl_oxide::AllocTracker::with_limit(JXL_MEMORY_LIMIT))
            .read(Cursor::new(bytes))
            .map_err(failed)?;
        area_allowed(image.width(), image.height())?;
        image.set_cms(jxl_oxide::Moxcms);
        image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
            jxl_oxide::RenderingIntent::Relative,
        ));
        let render = image.render_frame(0).map_err(failed)?;
        let mut stream = render.stream();
        let (width, height, channels) = (stream.width(), stream.height(), stream.channels());
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(channels as usize))
            .ok_or(DecodeError::TooLarge)?;
        let mut samples = vec![0_u8; len];
        stream.write_to_buffer(&mut samples);
        let image = match channels {
            1 => image::GrayImage::from_raw(width, height, samples).map(DynamicImage::ImageLuma8),
            2 => image::GrayAlphaImage::from_raw(width, height, samples)
                .map(DynamicImage::ImageLumaA8),
            3 => image::RgbImage::from_raw(width, height, samples).map(DynamicImage::ImageRgb8),
            4 => image::RgbaImage::from_raw(width, height, samples).map(DynamicImage::ImageRgba8),
            _ => None,
        }
        .ok_or_else(|| failed("unexpected JPEG XL channel layout"))?;
        Ok(finish(image, None, Orientation::NoTransforms, size))
    })
}

/// SVG through resvg, at its own size (scaled to the request), without
/// loading anything the file points at.
pub(super) fn decode_svg(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    contained(|| {
        let options = resvg::usvg::Options::default();
        let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(failed)?;
        let natural = tree.size();
        let natural = (
            natural.width().ceil().max(1.0) as u32,
            natural.height().ceil().max(1.0) as u32,
        );
        area_allowed(natural.0, natural.1)?;
        let (width, height) = size.fit(natural);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
            .ok_or_else(|| failed("empty SVG canvas"))?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / tree.size().width(),
                height as f32 / tree.size().height(),
            ),
            &mut pixmap.as_mut(),
        );
        let mut pixels = pixmap.take();
        unpremultiply(&mut pixels);
        image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| failed("SVG canvas size mismatch"))
    })
}
