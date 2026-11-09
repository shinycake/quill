//! A widget's images: only `data:` URLs reach here (the renderer refuses
//! every other scheme), decoded with the `image` crate, or resvg for SVG.
use std::collections::HashMap;
use tiny_skia::Pixmap;

/// The largest side an image is kept at, px; a larger one is refused.
const MAX_SIDE: u32 = 4096;

#[derive(Default)]
pub struct Images {
    images: HashMap<i32, Pixmap>,
}

impl Images {
    /// Decodes `url` under `id`; its size in CSS px, or `None`.
    pub fn load(&mut self, id: i32, url: &str) -> Option<(f32, f32)> {
        let (mime, bytes) = data_url(url)?;
        let pixmap = if mime.eq_ignore_ascii_case("image/svg+xml") {
            svg(&bytes)?
        } else {
            raster(&bytes)?
        };
        let size = (pixmap.width() as f32, pixmap.height() as f32);
        self.images.insert(id, pixmap);
        Some(size)
    }

    pub fn get(&self, id: i32) -> Option<&Pixmap> {
        self.images.get(&id)
    }
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

fn raster(bytes: &[u8]) -> Option<Pixmap> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let mut data = image.into_raw();
    premultiply(&mut data);
    Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(width, height)?)
}

fn svg(bytes: &[u8]) -> Option<Pixmap> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size().to_int_size();
    if size.width() > MAX_SIDE || size.height() > MAX_SIDE {
        return None;
    }
    let mut pixmap = Pixmap::new(size.width(), size.height())?;
    resvg::render(
        &tree,
        tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Some(pixmap)
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

    #[test]
    fn an_svg_image_decodes_at_its_size() {
        let mut images = Images::default();
        let svg = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='8'%3E%3Crect width='12' height='8' fill='red'/%3E%3C/svg%3E";
        assert_eq!(images.load(1, svg), Some((12.0, 8.0)));
        assert!(
            images
                .get(1)
                .is_some_and(|image| image.pixel(3, 3).is_some_and(|px| px.red() == 255))
        );
        assert_eq!(images.load(2, "data:image/png;base64,AAAA"), None);
    }
}
