//! HDR stills decoded on the CPU, for Windows (no ImageIO there): Ultra HDR
//! / ISO 21496-1 gain-map JPEGs in pure Rust and PQ / HLG JPEG XL through
//! jxl-oxide. Both give linear extended sRGB half floats, 1.0 = SDR white,
//! at the full HDR rendition; the shader tone-maps to the display.
//!
//! HEIC and AVIF stay SDR on Windows: Microsoft's codecs decode only their
//! base picture.

mod jxl;
#[cfg(test)]
mod tests;
mod ultra_hdr;

use super::half_float::f32_to_half;
use crate::ui::image_decode::{
    DecodeError, MAX_TEXTURE_EDGE, READ_BYTES_LIMIT, Sniffed, contained, sniff,
};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

/// The brightest channel must clear SDR white by this much before a
/// decode counts as HDR (as on macOS).
const HDR_PEAK_MIN: f32 = 1.01;

/// How much of a file the probe reads: JPEG and JPEG XL keep their
/// metadata at the start.
const PROBE_BYTES: u64 = 256 * 1024;

/// The largest finite half float: brighter values would pack as infinity,
/// which the shader's tone map and the mip chain turn into NaN.
const HALF_MAX: f32 = 65_504.0;

/// A decoded HDR still: premultiplied RGBA half floats (little-endian),
/// row after row.
pub(crate) struct CpuFrame {
    pub width: u32,
    pub height: u32,
    pub bytes: Arc<[u8]>,
    /// The brightest channel, in multiples of SDR white.
    pub peak: f32,
}

/// Linear RGBA f32 pixels before packing.
pub(super) struct LinearImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 4]>,
}

impl LinearImage {
    /// Pack to half floats; `None` when nothing is above SDR white.
    fn into_frame(self) -> Option<CpuFrame> {
        let finite = |value: f32| {
            if value.is_finite() {
                value.clamp(-HALF_MAX, HALF_MAX)
            } else {
                0.0
            }
        };
        let peak = self
            .pixels
            .iter()
            .flat_map(|[red, green, blue, _]| [*red, *green, *blue])
            .map(finite)
            .fold(0.0_f32, f32::max);
        if peak < HDR_PEAK_MIN {
            return None;
        }
        // Packed straight into the shared buffer the renderer reads, so
        // the frame is never held twice.
        let len = self.pixels.len().checked_mul(8)?;
        let mut bytes = Arc::<[u8]>::new_zeroed_slice(len);
        let slots = Arc::get_mut(&mut bytes)?;
        for (slot, [red, green, blue, alpha]) in
            slots.as_chunks_mut::<8>().0.iter_mut().zip(self.pixels)
        {
            let alpha = finite(alpha).clamp(0.0, 1.0);
            let values = [red * alpha, green * alpha, blue * alpha, alpha];
            for (pair, value) in slot.as_chunks_mut::<2>().0.iter_mut().zip(values) {
                for (byte, value) in pair
                    .iter_mut()
                    .zip(f32_to_half(finite(value)).to_le_bytes())
                {
                    byte.write(value);
                }
            }
        }
        // SAFETY: every byte was written above: `slots` has exactly 8 bytes
        // per pixel and the loop writes all 8 of each.
        let bytes = unsafe { bytes.assume_init() };
        Some(CpuFrame {
            width: self.width,
            height: self.height,
            bytes,
            peak,
        })
    }
}

/// Whether the file at `path` is a gain-map JPEG or an HDR JPEG XL
/// (metadata only); `None` when it can't be read.
pub(crate) fn probe(path: &Path) -> Option<bool> {
    let file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut head = Vec::new();
    file.take(PROBE_BYTES).read_to_end(&mut head).ok()?;
    let found = contained(|| {
        Ok(match sniff(&head, Some(path)) {
            Sniffed::Raster(image::ImageFormat::Jpeg) => ultra_hdr::probe(&head, len),
            Sniffed::JpegXl => jxl::probe(&head),
            _ => false,
        })
    });
    found.ok()
}

/// Decode `path` to its HDR rendition, long edge at most `long_edge`.
/// `Ok(None)` when it has nothing above SDR white.
pub(crate) fn decode(path: &Path, long_edge: u32) -> Result<Option<CpuFrame>, DecodeError> {
    let meta = std::fs::metadata(path).map_err(|err| DecodeError::Failed(err.to_string()))?;
    if !meta.is_file() {
        return Err(DecodeError::Unsupported);
    }
    if meta.len() > READ_BYTES_LIMIT as u64 {
        return Err(DecodeError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|err| DecodeError::Failed(err.to_string()))?;
    let image = contained(|| match sniff(&bytes, Some(path)) {
        Sniffed::Raster(image::ImageFormat::Jpeg) => ultra_hdr::decode(&bytes, long_edge),
        Sniffed::JpegXl => jxl::decode(&bytes, long_edge),
        _ => Ok(None),
    })?;
    Ok(image.and_then(LinearImage::into_frame))
}

/// The size to decode a `(width, height)` picture at: long edge at most
/// `long_edge` (never larger than the picture), within the texture edge and
/// the HDR pixel cap.
pub(super) fn target_size((width, height): (u32, u32), long_edge: u32) -> (u32, u32) {
    let natural = width.max(height).max(1);
    let area = u64::from(width) * u64::from(height);
    let cap = super::HDR_MAX_PIXELS as f64;
    let by_area = if area as f64 > cap {
        (f64::from(natural) * (cap / area as f64).sqrt()).floor() as u32
    } else {
        natural
    };
    let edge = long_edge
        .min(by_area)
        .min(MAX_TEXTURE_EDGE)
        .clamp(1, natural);
    if edge == natural {
        return (width, height);
    }
    let scale = f64::from(edge) / f64::from(natural);
    let fit = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
    (fit(width), fit(height))
}

/// The sRGB transfer curve, decoded.
pub(super) fn srgb_to_linear(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}
