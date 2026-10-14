//! HDR JPEG XL through jxl-oxide: PQ and HLG files, and linear ones whose
//! intensity target is above SDR's 255 nits. jxl-oxide renders them to PQ
//! with sRGB primaries (an HDR target, so it applies no tone map of its
//! own, and colours outside sRGB come out negative instead of clipped);
//! PQ is absolute, so dividing by the SDR reference white of BT.2408
//! (203 nits) gives 1.0 = SDR white.

use super::{LinearImage, target_size};
use crate::ui::image_decode::{DecodeError, area_allowed};
use jxl_oxide::color::{ColourSpace, Primaries, TransferFunction, WhitePoint};
use jxl_oxide::{EnumColourEncoding, InitializeResult, JxlImage, RenderingIntent};
use std::io::Cursor;

/// BT.2408's reference white for HDR, in nits.
const SDR_WHITE_NITS: f32 = 203.0;

/// The intensity target JPEG XL assumes for SDR pictures.
const SDR_INTENSITY_TARGET: f32 = 255.0;

/// The most memory jxl-oxide may use for one picture (as for SDR decodes).
const MEMORY_LIMIT: usize = 1024 * 1024 * 1024;

/// The largest HDR JPEG XL decoded: jxl-oxide renders at full size, and
/// each full-size float copy is 16 bytes a pixel. Larger ones show in SDR.
const MAX_PIXELS: u64 = 2 * super::super::HDR_MAX_PIXELS;

fn failed(err: impl std::fmt::Display) -> DecodeError {
    DecodeError::Failed(err.to_string())
}

fn is_hdr(image: &JxlImage) -> bool {
    image.hdr_type().is_some()
        || image.image_header().metadata.tone_mapping.intensity_target > SDR_INTENSITY_TARGET
}

/// Whether the JPEG XL whose first bytes are `head` is HDR (header only).
pub(super) fn probe(head: &[u8]) -> bool {
    let mut uninit = JxlImage::builder().build_uninit();
    if uninit.feed_bytes(head).is_err() {
        return false;
    }
    match uninit.try_init() {
        Ok(InitializeResult::Initialized(image)) => is_hdr(&image),
        _ => false,
    }
}

/// The HDR rendition of `bytes`, long edge at most `long_edge`;
/// `Ok(None)` for an SDR file.
pub(super) fn decode(bytes: &[u8], long_edge: u32) -> Result<Option<LinearImage>, DecodeError> {
    let mut image = JxlImage::builder()
        .alloc_tracker(jxl_oxide::AllocTracker::with_limit(MEMORY_LIMIT))
        .read(Cursor::new(bytes))
        .map_err(failed)?;
    if !is_hdr(&image) {
        return Ok(None);
    }
    area_allowed(image.width(), image.height())?;
    if u64::from(image.width()) * u64::from(image.height()) > MAX_PIXELS {
        return Ok(None);
    }
    image.set_cms(jxl_oxide::Moxcms);
    image.request_color_encoding(EnumColourEncoding {
        colour_space: ColourSpace::Rgb,
        white_point: WhitePoint::D65,
        primaries: Primaries::Srgb,
        tf: TransferFunction::Pq,
        rendering_intent: RenderingIntent::Relative,
    });
    let render = image.render_frame(0).map_err(failed)?;
    // The stream applies the file's orientation.
    let mut stream = render.stream();
    let (width, height, channels) = (stream.width(), stream.height(), stream.channels());
    let channels = channels as usize;
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or(DecodeError::TooLarge)?;
    let mut samples = vec![0.0_f32; len];
    stream.write_to_buffer(&mut samples);
    drop(stream);
    drop(render);
    let to_linear = |value: f32| pq_to_nits(value) / SDR_WHITE_NITS;
    let pixels: Vec<[f32; 4]> = std::mem::take(&mut samples)
        .chunks_exact(channels.max(1))
        .map(|pixel| match *pixel {
            [gray] => [to_linear(gray), to_linear(gray), to_linear(gray), 1.0],
            [gray, alpha] => [to_linear(gray), to_linear(gray), to_linear(gray), alpha],
            [red, green, blue] => [to_linear(red), to_linear(green), to_linear(blue), 1.0],
            [red, green, blue, alpha, ..] => {
                [to_linear(red), to_linear(green), to_linear(blue), alpha]
            }
            [] => [0.0; 4],
        })
        .collect();
    let linear = LinearImage {
        width,
        height,
        pixels,
    };
    Ok(Some(scaled(linear, long_edge)))
}

/// The SMPTE ST 2084 (PQ) curve, decoded to nits. Negative values (colours
/// outside the primaries) stay negative.
pub(super) fn pq_to_nits(encoded: f32) -> f32 {
    const M1: f32 = 0.159_301_76;
    const M2: f32 = 78.843_75;
    const C1: f32 = 0.835_937_5;
    const C2: f32 = 18.851_563;
    const C3: f32 = 18.687_5;
    let power = encoded.abs().powf(1.0 / M2);
    let linear = ((power - C1).max(0.0) / (C2 - C3 * power)).powf(1.0 / M1);
    (linear * 10_000.0).copysign(encoded)
}

/// `image` shrunk so its long edge fits (straight alpha, so a box of
/// pixels averages as it should for an opaque still).
fn scaled(image: LinearImage, long_edge: u32) -> LinearImage {
    let (width, height) = (image.width, image.height);
    let (target_width, target_height) = target_size((width, height), long_edge);
    if (target_width, target_height) == (width, height) {
        return image;
    }
    let Some(buffer) = image::Rgba32FImage::from_raw(width, height, image.pixels.into_flattened())
    else {
        return LinearImage {
            width,
            height,
            pixels: Vec::new(),
        };
    };
    let resized = image::imageops::resize(
        &buffer,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    );
    let pixels = resized.into_raw().as_chunks::<4>().0.to_vec();
    LinearImage {
        width: target_width,
        height: target_height,
        pixels,
    }
}
