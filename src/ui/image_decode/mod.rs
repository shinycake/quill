//! Decoding image files for display: photos, avatars, and the pictures
//! people send as files (tdesktop previews every format its
//! `core/mime_type.cpp` `DetectNameType` calls an image, through Qt's image
//! plugins plus the bundled libheif, libavif and libjxl).
//!
//! Every decoder hands back the same thing: straight-alpha RGBA in sRGB,
//! turned upright (EXIF / HEIF orientation applied) and no larger than the
//! request asks for. Colour profiles are honoured, so a Display P3 or
//! Adobe RGB file looks as it does in other apps instead of washed out.
//!
//! Which decoder runs:
//! - PNG, JPEG, WebP, GIF, BMP, ICO, TIFF, QOI, TGA: the `image` crate on
//!   every platform, colour-managed with moxcms (`rust_codecs`).
//! - JPEG XL: ImageIO on macOS 14+, jxl-oxide everywhere else (and as the
//!   fallback on older macOS).
//! - HEIC / HEIF / AVIF: ImageIO on macOS; Windows Imaging Component on
//!   Windows (needs Microsoft's free HEIF / AV1 extensions, which Windows 11
//!   ships with); the system libheif on Linux, loaded at run time.
//! - SVG: resvg, without external resources.
//! - Anything else ImageIO reads (DNG and other camera RAW, PSD, ICNS…) on
//!   macOS.
//!
//! When no decoder can read a file the caller shows the document's
//! thumbnail and offers to open it in another app (tdesktop does the same
//! when `QImageReader` cannot read it).
//!
//! HDR content (gain maps, PQ / HLG) decodes to its SDR rendition here,
//! which is what every SDR screen shows.

use std::path::Path;

mod rust_codecs;
#[cfg(any(windows, test))]
pub(crate) use rust_codecs::to_srgb;

#[cfg(not(windows))]
mod libheif;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod wic;

#[cfg(test)]
mod tests;

/// tdesktop `Images::kReadBytesLimit` (lib_ui `ui/image/image_prepare.h`):
/// bigger image files open in another app instead of the viewer.
pub(crate) const READ_BYTES_LIMIT: i64 = quill::media_viewer::READ_BYTES_LIMIT;

/// tdesktop `Images::kReadMaxArea`: pictures with more pixels than this are
/// not decoded at all.
pub(crate) const READ_MAX_AREA: u64 = 12_032 * 9_024;

/// The longest side a decoded picture may have: the smallest maximum
/// texture size of GPUI's renderers (wgpu's default limit is 8192; Metal and
/// DirectX allow 16384). Larger pictures are scaled down to fit.
pub(crate) const MAX_TEXTURE_EDGE: u32 = 8192;

/// How large a decode should be.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DecodeSize {
    /// The shorter side is scaled down to at most this many pixels
    /// (`u32::MAX` keeps the file's own size). Never scaled up.
    pub short_edge: u32,
}

impl DecodeSize {
    /// The picture at its own size (still within [`MAX_TEXTURE_EDGE`]).
    pub(crate) const FULL: Self = Self {
        short_edge: u32::MAX,
    };

    /// The decoded size for a picture of `size` pixels.
    pub(crate) fn fit(self, (width, height): (u32, u32)) -> (u32, u32) {
        if width == 0 || height == 0 {
            return (width, height);
        }
        let short = width.min(height);
        let long = width.max(height);
        let by_short = f64::from(self.short_edge.max(1)) / f64::from(short);
        let by_long = f64::from(MAX_TEXTURE_EDGE) / f64::from(long);
        let scale = by_short.min(by_long);
        if scale >= 1.0 {
            return (width, height);
        }
        let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
        (side(width), side(height))
    }

    /// The longest side of the decoded picture (what ImageIO scales to).
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn long_edge(self, size: (u32, u32)) -> u32 {
        let (width, height) = self.fit(size);
        width.max(height)
    }
}

/// Why a picture could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// No decoder on this system reads the format.
    Unsupported,
    /// More pixels than [`READ_MAX_AREA`].
    TooLarge,
    /// The file is damaged, truncated, or the decoder failed.
    Failed(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("unsupported image format"),
            Self::TooLarge => f.write_str("image is too large"),
            Self::Failed(reason) => write!(f, "could not decode image: {reason}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// What the first bytes of a file say it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sniffed {
    /// A format the `image` crate reads.
    Raster(image::ImageFormat),
    /// HEIF container with HEVC pictures (HEIC) or other HEIF brands.
    Heif,
    /// HEIF container with AV1 pictures.
    Avif,
    JpegXl,
    Svg,
    /// Unknown: only the platform decoder may know it (camera RAW, PSD…).
    Other,
}

/// Identify a file from its first bytes (and, for the formats without a
/// signature, its name).
pub(crate) fn sniff(bytes: &[u8], name: Option<&Path>) -> Sniffed {
    if let Some(brand) = iso_bmff_brand(bytes) {
        return match brand {
            b"avif" | b"avis" => Sniffed::Avif,
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1"
            | b"mif2" => Sniffed::Heif,
            _ => Sniffed::Other,
        };
    }
    if bytes.starts_with(&[0xFF, 0x0A])
        || bytes.starts_with(&[
            0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
        ])
    {
        return Sniffed::JpegXl;
    }
    let extension = name
        .and_then(Path::extension)
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase);
    if extension.as_deref() == Some("svg") || looks_like_svg(bytes) {
        return Sniffed::Svg;
    }
    if let Ok(format) = image::guess_format(bytes) {
        return Sniffed::Raster(format);
    }
    // TGA has no signature: trust the name.
    if extension.as_deref() == Some("tga") {
        return Sniffed::Raster(image::ImageFormat::Tga);
    }
    Sniffed::Other
}

/// The major brand of an ISO base media file (`ftyp` box), when `bytes`
/// starts with one. AVIF files list `avif` as a compatible brand behind a
/// `mif1` major brand, so the compatible brands are checked for it too.
fn iso_bmff_brand(bytes: &[u8]) -> Option<&[u8; 4]> {
    let header = bytes.get(4..8)?;
    if header != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
    let major: &[u8; 4] = bytes.get(8..12)?.try_into().ok()?;
    let compatible = bytes.get(16..size.min(bytes.len()))?;
    let lists_avif = compatible
        .as_chunks::<4>()
        .0
        .iter()
        .any(|brand| brand == b"avif" || brand == b"avis");
    if lists_avif && major != b"avif" && major != b"avis" {
        return Some(b"avif");
    }
    Some(major)
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = bytes.get(..bytes.len().min(512)).unwrap_or_default();
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    (text.starts_with("<?xml") || text.starts_with("<svg") || text.starts_with("<!DOCTYPE svg"))
        && text.contains("<svg")
}

/// Decode `bytes` (a whole image file) to upright sRGB RGBA, no larger than
/// `size` allows. `name` helps with formats that carry no signature.
pub(crate) fn decode_image(
    bytes: &[u8],
    name: Option<&Path>,
    size: DecodeSize,
) -> Result<image::RgbaImage, DecodeError> {
    let sniffed = sniff(bytes, name);
    let rust = || -> Result<image::RgbaImage, DecodeError> {
        match sniffed {
            Sniffed::Raster(format) => rust_codecs::decode_raster(bytes, format, size),
            Sniffed::JpegXl => rust_codecs::decode_jxl(bytes, size),
            Sniffed::Svg => rust_codecs::decode_svg(bytes, size),
            Sniffed::Heif | Sniffed::Avif | Sniffed::Other => Err(DecodeError::Unsupported),
        }
    };
    match sniffed {
        // The `image` crate is the one decoder on every platform for these
        // (same pixels everywhere); ImageIO / WIC are the fallback for the
        // variants it rejects (CMYK JPEG, 16-bit TIFF layouts…).
        Sniffed::Raster(_) | Sniffed::Svg => rust().or_else(|err| match err {
            DecodeError::TooLarge => Err(err),
            _ => contained(|| platform(bytes, size)).map_err(|_| err),
        }),
        Sniffed::JpegXl => contained(|| platform(bytes, size)).or_else(|_| rust()),
        Sniffed::Heif | Sniffed::Avif | Sniffed::Other => contained(|| platform(bytes, size)),
    }
}

/// Decode with the operating system's codecs.
fn platform(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    #[cfg(target_os = "macos")]
    {
        macos::decode(bytes, size).or_else(|err| match err {
            DecodeError::TooLarge => Err(err),
            // ImageIO reads HEIF and AVIF on every macOS Quill runs on;
            // libheif (Homebrew) only helps on systems where it does not.
            _ => libheif::decode(bytes, size).map_err(|_| err),
        })
    }
    #[cfg(windows)]
    {
        wic::decode(bytes, size)
    }
    #[cfg(all(not(target_os = "macos"), not(windows)))]
    {
        libheif::decode(bytes, size)
    }
}

/// Read a file and decode it, refusing files over [`READ_BYTES_LIMIT`].
pub(crate) fn decode_file(path: &Path, size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    let meta = std::fs::metadata(path).map_err(|err| DecodeError::Failed(err.to_string()))?;
    // A FIFO or device would block the read forever.
    if !meta.is_file() {
        return Err(DecodeError::Unsupported);
    }
    if meta.len() > READ_BYTES_LIMIT as u64 {
        return Err(DecodeError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|err| DecodeError::Failed(err.to_string()))?;
    decode_image(&bytes, Some(path), size)
}

/// Whether a picture of this many pixels may be decoded.
pub(crate) fn area_allowed(width: u32, height: u32) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        return Err(DecodeError::Failed("empty image".into()));
    }
    if u64::from(width) * u64::from(height) > READ_MAX_AREA {
        return Err(DecodeError::TooLarge);
    }
    Ok(())
}

/// Run a third-party decoder on untrusted bytes. A bug in one (a panic on a
/// malformed file) must cost that one picture, not the app.
pub(crate) fn contained<T>(
    decode: impl FnOnce() -> Result<T, DecodeError>,
) -> Result<T, DecodeError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode))
        .unwrap_or_else(|_| Err(DecodeError::Failed("decoder panicked".into())))
}

/// Undo premultiplied alpha in place (CoreGraphics and WIC hand pixels
/// back premultiplied; GPUI draws straight alpha).
pub(crate) fn unpremultiply(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in pixel.iter_mut().take(3) {
            let value = (u16::from(*channel) * 255 + alpha / 2) / alpha;
            *channel = value.min(255) as u8;
        }
    }
}
