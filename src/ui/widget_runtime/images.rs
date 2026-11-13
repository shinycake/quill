//! A widget's images: only `data:` URLs of the profile's raster types reach
//! here (the renderer refuses every other scheme and type), decoded with
//! the `image` crate. Each one is checked again here, as Telegram Desktop's
//! backend does: the media type picks the one decoder that may read it, the
//! picture's size is checked before its pixels are made, and what one
//! widget holds is bounded in count and in bytes.
use std::collections::HashMap;
use std::io::Cursor;
use tiny_skia::Pixmap;

/// The largest side an image is kept at, px; a larger one is refused.
const MAX_SIDE: u32 = 4096;
/// The most pixels one image has (a 2048 × 2048 picture, or 4096 × 1024).
const MAX_PIXELS: u64 = 4 * 1024 * 1024;
/// The longest `data:` URL decoded, bytes.
const MAX_URL_BYTES: usize = 5 * 1024 * 1024;
/// The images one widget holds at once, and their pixels together, bytes.
const MAX_HELD: usize = 1024;
const MAX_HELD_BYTES: u64 = 128 * 1024 * 1024;

/// The raster types a widget may show (the profile's image types), each
/// read only by its own decoder.
fn format(mime: &str) -> Option<image::ImageFormat> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Some(image::ImageFormat::Png),
        "image/jpeg" | "image/jpg" => Some(image::ImageFormat::Jpeg),
        "image/gif" => Some(image::ImageFormat::Gif),
        "image/webp" => Some(image::ImageFormat::WebP),
        _ => None,
    }
}

#[derive(Default)]
pub struct Images {
    images: HashMap<i32, Pixmap>,
    /// The bytes of the pixmaps in `images`.
    held_bytes: u64,
}

impl Images {
    /// Decodes `url` under `id`; its size in CSS px, or `None`.
    pub fn load(&mut self, id: i32, url: &str) -> Option<(f32, f32)> {
        if url.len() > MAX_URL_BYTES {
            return None;
        }
        let (mime, bytes) = data_url(url)?;
        let replaced = self.images.get(&id).map_or(0, pixmap_bytes);
        let room = MAX_HELD_BYTES.saturating_sub(self.held_bytes.saturating_sub(replaced));
        if replaced == 0 && self.images.len() >= MAX_HELD {
            return None;
        }
        let pixmap = raster(format(&mime)?, &bytes, room)?;
        let size = (pixmap.width() as f32, pixmap.height() as f32);
        self.held_bytes = self.held_bytes.saturating_sub(replaced) + pixmap_bytes(&pixmap);
        self.images.insert(id, pixmap);
        Some(size)
    }

    pub fn get(&self, id: i32) -> Option<&Pixmap> {
        self.images.get(&id)
    }
}

fn pixmap_bytes(pixmap: &Pixmap) -> u64 {
    u64::from(pixmap.width()) * u64::from(pixmap.height()) * 4
}

/// Whether a picture of `width` × `height` may be made within `room` bytes.
fn fits(width: u32, height: u32, room: u64) -> bool {
    let pixels = u64::from(width) * u64::from(height);
    width > 0
        && height > 0
        && width <= MAX_SIDE
        && height <= MAX_SIDE
        && pixels <= MAX_PIXELS
        && pixels * 4 <= room
}

/// The media type and the bytes of a `data:` URL.
fn data_url(url: &str) -> Option<(String, Vec<u8>)> {
    let rest = url.trim().strip_prefix("data:")?;
    let (meta, body) = rest.split_once(',')?;
    let mut parts = meta.split(';');
    let mime = parts.next().unwrap_or("").trim().to_owned();
    let base64 = parts.any(|part| part.trim().eq_ignore_ascii_case("base64"));
    let bytes = if base64 {
        use base64::Engine;
        let clean: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        base64::engine::general_purpose::STANDARD
            .decode(clean.as_bytes())
            .or_else(|_| {
                base64::engine::general_purpose::STANDARD_NO_PAD.decode(clean.trim_end_matches('='))
            })
            .ok()?
    } else {
        percent_decode(body)
    };
    Some((mime, bytes))
}

fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        let hex = |at: usize| bytes.get(at).and_then(|b| (*b as char).to_digit(16));
        if byte == b'%'
            && let (Some(high), Some(low)) = (hex(i + 1), hex(i + 2))
        {
            out.push((high * 16 + low) as u8);
            i += 3;
        } else {
            out.push(byte);
            i += 1;
        }
    }
    out
}

/// `bytes` as `format`, read only when its header gives a size that fits
/// in `room`; the decoder is held to the same size and allocation.
fn raster(format: image::ImageFormat, bytes: &[u8], room: u64) -> Option<Pixmap> {
    let header = image::ImageReader::with_format(Cursor::new(bytes), format);
    let (width, height) = header.into_dimensions().ok()?;
    if !fits(width, height, room) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(width);
    limits.max_image_height = Some(height);
    // The picture, its RGBA copy and the decoder's own buffers.
    limits.max_alloc = Some(u64::from(width) * u64::from(height) * 4 * 3 + (1 << 20));
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let image = reader.decode().ok()?.to_rgba8();
    // A decoder that reads a size other than its header's is refused.
    if image.dimensions() != (width, height) {
        return None;
    }
    let mut data = image.into_raw();
    premultiply(&mut data);
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(width, height)?)
}

/// Straight RGBA to premultiplied, in place.
pub fn premultiply(rgba: &mut [u8]) {
    for [r, g, b, a] in rgba.as_chunks_mut::<4>().0 {
        let alpha = u16::from(*a);
        let scale = |c: &mut u8| *c = ((u16::from(*c) * alpha + 127) / 255) as u8;
        scale(r);
        scale(g);
        scale(b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_urls_decode_base64_and_percent_forms() {
        assert_eq!(
            data_url("data:text/plain;base64,UXVpbGw="),
            Some(("text/plain".into(), b"Quill".to_vec()))
        );
        assert_eq!(
            data_url("data:image/svg+xml,%3Csvg%3E"),
            Some(("image/svg+xml".into(), b"<svg>".to_vec()))
        );
        assert_eq!(data_url("https://example.com/a.png"), None);
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 0, 0, 255]));
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn data(mime: &str, bytes: &[u8]) -> String {
        use base64::Engine;
        let body = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:{mime};base64,{body}")
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    /// `png` with its header (IHDR) changed by `edit` and its CRC made
    /// right again, so only the changed fields are wrong.
    fn with_header(mut png: Vec<u8>, edit: impl FnOnce(&mut [u8])) -> Vec<u8> {
        // Signature (8), length (4), "IHDR" (4), 13 bytes of fields, CRC (4).
        edit(&mut png[16..29]);
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        png
    }

    #[test]
    fn a_png_decodes_at_its_size() {
        let mut images = Images::default();
        assert_eq!(
            images.load(1, &data("image/png", &png(12, 8))),
            Some((12.0, 8.0))
        );
        assert!(
            images
                .get(1)
                .is_some_and(|image| image.pixel(3, 3).is_some_and(|px| px.red() == 255))
        );
        assert_eq!(images.load(2, "data:image/png;base64,AAAA"), None);
    }

    #[test]
    fn only_the_profile_types_are_decoded_and_each_by_its_own_decoder() {
        let mut images = Images::default();
        let svg = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8'/%3E";
        assert_eq!(images.load(1, svg), None);
        let mut tiff = Cursor::new(Vec::new());
        image::RgbaImage::new(4, 4)
            .write_to(&mut tiff, image::ImageFormat::Tiff)
            .unwrap();
        let tiff = tiff.into_inner();
        assert_eq!(images.load(2, &data("image/tiff", &tiff)), None);
        // A TIFF named a PNG reaches the PNG decoder only, which refuses it.
        assert_eq!(images.load(3, &data("image/png", &tiff)), None);
        assert_eq!(images.load(4, &data("image/jpeg", &png(4, 4))), None);
        assert_eq!(
            images.load(5, &data("IMAGE/PNG", &png(4, 4))),
            Some((4.0, 4.0))
        );
    }

    #[test]
    fn a_png_whose_header_is_too_large_or_impossible_is_refused() {
        let mut images = Images::default();
        let huge = with_header(png(4, 4), |ihdr| {
            ihdr[0..4].copy_from_slice(&65_535u32.to_be_bytes());
            ihdr[4..8].copy_from_slice(&65_535u32.to_be_bytes());
        });
        assert_eq!(images.load(1, &data("image/png", &huge)), None);
        // Each side fits but the pixels together do not.
        let wide = with_header(png(4, 4), |ihdr| {
            ihdr[0..4].copy_from_slice(&4096u32.to_be_bytes());
            ihdr[4..8].copy_from_slice(&4096u32.to_be_bytes());
        });
        assert_eq!(images.load(2, &data("image/png", &wide)), None);
        // A bit depth no PNG has (Telegram Desktop's 2026-11-13 case).
        let depth = with_header(png(4, 4), |ihdr| ihdr[8] = 255);
        assert_eq!(images.load(3, &data("image/png", &depth)), None);
        let truncated = png(16, 16);
        let truncated = &truncated[..truncated.len() / 2];
        assert_eq!(images.load(4, &data("image/png", truncated)), None);
        assert!(images.images.is_empty());
    }

    #[test]
    fn what_a_widget_holds_is_bounded_together() {
        let mut images = Images::default();
        let url = data("image/png", &png(16, 16));
        images.held_bytes = MAX_HELD_BYTES - 1023;
        assert_eq!(images.load(1, &url), None);
        images.held_bytes = MAX_HELD_BYTES - 1024;
        assert_eq!(images.load(1, &url), Some((16.0, 16.0)));
        assert_eq!(images.held_bytes, MAX_HELD_BYTES);
        // Replacing an image frees what it held first.
        assert_eq!(images.load(1, &url), Some((16.0, 16.0)));
        assert_eq!(images.held_bytes, MAX_HELD_BYTES);
        let long = format!("data:image/png;base64,{}", "A".repeat(MAX_URL_BYTES));
        assert_eq!(Images::default().load(2, &long), None);
    }
}
