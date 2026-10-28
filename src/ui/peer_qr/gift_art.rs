//! The gift theme's pictures (`parity:profile-share-theme`): a still of
//! each gift's model, its symbol tinted in the backdrop's pattern color,
//! and the gifts panel's tile backgrounds. Made off the UI thread from
//! the session's files and kept while the box is open; drawing only looks
//! them up, so what shows is still a function of the box state.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::ids::FileId;
use quill::peer_qr::color::Rgb;
use quill::peer_qr::theme::ThemeGift;
use quill::telegram::envelope::{StickerContent, StickerFormat};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// A model still's edge in pixels: sharp at 2x for the largest tile.
const MODEL_PIXELS: usize = 256;
/// A symbol is drawn at most 24 points across.
const SYMBOL_PIXELS: usize = 64;
/// Tile backgrounds are smooth gradients, made at half size.
const TILE_SCALE: f64 = 0.5;

#[derive(Clone)]
enum Slot {
    Loading,
    Ready {
        image: Arc<RenderImage>,
        at: Instant,
    },
    Failed,
}

/// Which picture of a sticker: as drawn, or every pixel in one color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StillKey {
    file: FileId,
    tint: Option<Rgb>,
}

/// A tile's gradient at a size in points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    center: Rgb,
    edge: Rgb,
    width: u32,
    height: u32,
}

#[derive(Default)]
pub(super) struct GiftArt {
    stills: HashMap<StillKey, Slot>,
    tiles: HashMap<TileKey, Slot>,
}

fn ready(slot: Option<&Slot>) -> Option<(Arc<RenderImage>, Instant)> {
    match slot? {
        Slot::Ready { image, at } => Some((image.clone(), *at)),
        Slot::Loading | Slot::Failed => None,
    }
}

impl GiftArt {
    pub(super) fn clear(&mut self) {
        let images = self
            .stills
            .drain()
            .map(|(_, slot)| slot)
            .chain(self.tiles.drain().map(|(_, slot)| slot))
            .filter_map(|slot| ready(Some(&slot)).map(|(image, _)| image))
            .collect::<Vec<_>>();
        super::super::image_budget::retire_all(images);
    }
}

/// The still to make of a sticker: its file and how to read it. A video
/// sticker uses its image thumbnail.
fn still_source(sticker: &StickerContent) -> Option<(FileId, StickerFormat)> {
    match sticker.format {
        StickerFormat::Webm => sticker
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .map(|id| (id, StickerFormat::Webp)),
        format => Some((sticker.file_id, format)).filter(|(id, _)| id.0 != 0),
    }
}

/// Straight-alpha BGRA `edge × edge` pixels of the sticker's first frame
/// (GPUI's RenderImage layout).
fn sticker_bgra(path: &PathBuf, format: StickerFormat, edge: usize) -> Option<image::RgbaImage> {
    let side = u32::try_from(edge).ok()?;
    match format {
        StickerFormat::Tgs => {
            let cancelled = AtomicBool::new(false);
            let frames =
                quill::sticker_playback::decode_tgs_sized(path, edge, 1, f64::INFINITY, &cancelled)
                    .ok()?;
            image::RgbaImage::from_raw(side, side, frames.frames.into_iter().next()?)
        }
        _ => {
            let image = image::open(path)
                .ok()?
                .resize(side, side, image::imageops::FilterType::Triangle)
                .into_rgba8();
            let (width, height) = image.dimensions();
            let mut pixels = image.into_raw();
            for pixel in pixels.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            image::RgbaImage::from_raw(width, height, pixels)
        }
    }
}

/// Every pixel in `color`, keeping its alpha (a symbol takes the
/// backdrop's pattern color, as tdesktop paints it with `textColor`).
fn fill(image: &mut image::RgbaImage, color: Rgb) {
    let hex = color.to_hex();
    let [r, g, b] = [(hex >> 16) & 0xFF, (hex >> 8) & 0xFF, hex & 0xFF].map(|c| c as u8);
    for pixel in image.as_chunks_mut::<4>().0 {
        *pixel = [b, g, r, pixel[3]];
    }
}

fn render(image: image::RgbaImage) -> Arc<RenderImage> {
    Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
        image,
    )])))
}

/// A tile's two-stop radial gradient, its radius half the longer side
/// (tdesktop `PrepareBackground`), as BGRA.
fn tile_pixels(center: Rgb, edge: Rgb, width: u32, height: u32) -> image::RgbaImage {
    let w = (f64::from(width) * TILE_SCALE).ceil().max(1.);
    let h = (f64::from(height) * TILE_SCALE).ceil().max(1.);
    let radius = w.max(h) / 2.;
    let channel = |rgb: Rgb, shift: u32| f64::from((rgb.to_hex() >> shift) & 0xFF);
    image::RgbaImage::from_fn(w as u32, h as u32, |x, y| {
        let dx = f64::from(x) + 0.5 - w / 2.;
        let dy = f64::from(y) + 0.5 - h / 2.;
        let t = ((dx * dx + dy * dy).sqrt() / radius).clamp(0., 1.);
        let mix = |shift| {
            let a = channel(center, shift);
            (a + (channel(edge, shift) - a) * t).round().clamp(0., 255.) as u8
        };
        image::Rgba([mix(0), mix(8), mix(16), 255])
    })
}

impl QuillApp {
    /// The gifts the open box draws: the shown one, and the gifts panel's
    /// list while it is open.
    fn qr_drawn_gifts(&self) -> Vec<ThemeGift> {
        let Some(open) = self.peer_qr.open.as_ref() else {
            return Vec::new();
        };
        let mut gifts: Vec<ThemeGift> = open
            .state
            .gift
            .iter()
            .map(|gift| (**gift).clone())
            .collect();
        if open.motion.shown == Some(quill::peer_qr::state::QrPanel::Gifts)
            && let Some(session) = self.session()
        {
            gifts.extend(session.users_state.peer_qr.gifts.list.iter().cloned());
        }
        gifts
    }

    /// Start making the gift pictures the box needs, and download the
    /// stickers they come from.
    pub(super) fn qr_prepare_gift_art(&mut self, cx: &mut Context<Self>) {
        let gifts = self.qr_drawn_gifts();
        if gifts.is_empty() {
            return;
        }
        let shown = self
            .peer_qr
            .open
            .as_ref()
            .and_then(|open| open.state.gift.as_ref().map(|gift| gift.id));
        let mut wanted = Vec::new();
        for gift in &gifts {
            if let Some(model) = gift.model.as_ref().and_then(still_source) {
                wanted.push((model, None, MODEL_PIXELS));
            }
            if Some(gift.id) == shown
                && let Some(symbol) = gift.symbol.as_ref().and_then(still_source)
            {
                wanted.push((symbol, Some(gift.symbol_color), SYMBOL_PIXELS));
            }
        }
        let mut missing = Vec::new();
        for ((file, format), tint, edge) in wanted {
            let key = StillKey { file, tint };
            if self.peer_qr.gift_art.stills.contains_key(&key) {
                continue;
            }
            let path = self.session().and_then(|session| {
                session
                    .media
                    .files
                    .get(&file.0)
                    .and_then(|known| known.usable_path())
                    .map(PathBuf::from)
            });
            let Some(path) = path else {
                missing.push(file);
                continue;
            };
            self.peer_qr.gift_art.stills.insert(key, Slot::Loading);
            self.run_off_ui_then(
                move || {
                    let mut image = sticker_bgra(&path, format, edge)?;
                    if let Some(color) = tint {
                        fill(&mut image, color);
                    }
                    Some(render(image))
                },
                move |this, image, cx| {
                    if let Some(slot) = this.peer_qr.gift_art.stills.get_mut(&key) {
                        *slot = match image {
                            Some(image) => Slot::Ready {
                                image,
                                at: Instant::now(),
                            },
                            None => Slot::Failed,
                        };
                    }
                    cx.notify();
                },
                cx,
            );
        }
        if !missing.is_empty()
            && let Some(live) = self.live.as_mut()
        {
            live.driver.download_share_theme_files(&missing);
        }
    }

    /// A gift's model still and when it was made.
    pub(super) fn qr_gift_model(&self, gift: &ThemeGift) -> Option<(Arc<RenderImage>, Instant)> {
        let (file, _) = gift.model.as_ref().and_then(still_source)?;
        ready(
            self.peer_qr
                .gift_art
                .stills
                .get(&StillKey { file, tint: None }),
        )
    }

    /// A gift's symbol still in its pattern color and when it was made.
    pub(super) fn qr_gift_symbol(&self, gift: &ThemeGift) -> Option<(Arc<RenderImage>, Instant)> {
        let (file, _) = gift.symbol.as_ref().and_then(still_source)?;
        ready(self.peer_qr.gift_art.stills.get(&StillKey {
            file,
            tint: Some(gift.symbol_color),
        }))
    }

    /// A tile's gradient, `width × height` points; made on first ask and
    /// `None` until then.
    pub(super) fn qr_gift_tile_image(
        &mut self,
        center: Rgb,
        edge: Rgb,
        width: u32,
        height: u32,
        cx: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        let key = TileKey {
            center,
            edge,
            width,
            height,
        };
        if let Some(slot) = self.peer_qr.gift_art.tiles.get(&key) {
            return ready(Some(slot)).map(|(image, _)| image);
        }
        self.peer_qr.gift_art.tiles.insert(key, Slot::Loading);
        self.run_off_ui_then(
            move || render(tile_pixels(center, edge, width, height)),
            move |this, image, cx| {
                if let Some(slot) = this.peer_qr.gift_art.tiles.get_mut(&key) {
                    *slot = Slot::Ready {
                        image,
                        at: Instant::now(),
                    };
                }
                cx.notify();
            },
            cx,
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Rgb, fill, tile_pixels};

    #[test]
    fn a_symbol_takes_the_pattern_color_and_keeps_its_alpha() {
        let mut image =
            image::RgbaImage::from_raw(2, 1, vec![10, 20, 30, 255, 0, 0, 0, 128]).expect("pixels");
        fill(&mut image, Rgb::hex(0xFF_8000));
        assert_eq!(image.as_raw(), &vec![0, 128, 255, 255, 0, 128, 255, 128]);
    }

    #[test]
    fn a_tile_runs_from_its_center_color_to_its_edge_color() {
        let image = tile_pixels(Rgb::hex(0xFF_0000), Rgb::hex(0x00_00FF), 100, 100);
        let (w, h) = image.dimensions();
        let middle = image.get_pixel(w / 2, h / 2);
        // BGRA: red in the third channel.
        assert!(middle[2] > 240 && middle[0] < 20, "{middle:?}");
        let corner = image.get_pixel(0, 0);
        assert_eq!(corner.0, [255, 0, 0, 255]);
    }
}
