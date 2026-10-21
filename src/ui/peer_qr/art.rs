//! The box's pictures, made off the UI thread and kept while it is open:
//! the backdrop gradient, the code in its ink and a still of each picked
//! emoji. Each is derived from the box state; the cache only saves making
//! it again. [`QuillApp::qr_prepare_art`] starts the work when the box or
//! what it shows changes; drawing only looks pictures up.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use super::qr_path::{QrGrid, qr_svg};
use gpui_kit::*;
use quill::peer_qr::appearance::{GRADIENT_RADIUS_TO_CARD, GRADIENT_STEPS, gradient_stops};
use quill::peer_qr::color::Rgb;
use quill::peer_qr::layout::{CARD_WIDTH, card_layout};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A new backdrop fades in over the old one (tdesktop animates the
/// appearance change).
pub(super) const BACKDROP_FADE: Duration = Duration::from_millis(220);
/// The gradient is smooth, so it is made at half the logical size and
/// drawn scaled up.
const BACKDROP_SCALE: f64 = 0.5;
/// The code is made at three pixels per point: sharp at 2× and when the
/// card grows on a wide box.
const CODE_SCALE: f64 = 3.;
/// One emoji still, in pixels.
const STILL_PIXELS: u32 = 128;

/// The gradient's radius in points: everything past it is the edge color.
pub(super) fn backdrop_radius() -> f64 {
    f64::from(GRADIENT_RADIUS_TO_CARD) * CARD_WIDTH
}

#[derive(Clone)]
enum Slot {
    Loading,
    Ready {
        image: Arc<RenderImage>,
        at: Instant,
    },
    Failed,
}

impl Slot {
    fn ready(&self) -> Option<(Arc<RenderImage>, Instant)> {
        match self {
            Slot::Ready { image, at } => Some((image.clone(), *at)),
            Slot::Loading | Slot::Failed => None,
        }
    }
}

/// The open box's pictures, newest last.
#[derive(Default)]
pub(super) struct Art {
    backdrops: Vec<((Rgb, Rgb), Slot)>,
    codes: Vec<((String, Rgb), Slot)>,
    stills: HashMap<String, Slot>,
}

/// Keep the shown picture and the one it replaces.
const KEEP: usize = 2;

fn trim<K>(list: &mut Vec<(K, Slot)>) {
    while list.len() > KEEP {
        let (_, slot) = list.remove(0);
        if let Slot::Ready { image, .. } = slot {
            super::super::image_budget::retire_all([image]);
        }
    }
}

impl Art {
    /// Drop everything (the box closed).
    pub(super) fn clear(&mut self) {
        let images = self
            .backdrops
            .drain(..)
            .map(|(_, slot)| slot)
            .chain(self.codes.drain(..).map(|(_, slot)| slot))
            .filter_map(|slot| slot.ready().map(|(image, _)| image))
            .collect::<Vec<_>>();
        super::super::image_budget::retire_all(images);
        // Only the stills Quill drew are here; a set's sprites stay in its atlas.
        let stills = std::mem::take(&mut self.stills);
        super::super::image_budget::retire_all(
            stills
                .into_values()
                .filter_map(|slot| slot.ready().map(|(image, _)| image)),
        );
    }

    /// The still of `emoji`, when made, for the saved image.
    pub(super) fn still(&self, emoji: &str) -> Option<Arc<RenderImage>> {
        self.stills
            .get(emoji)
            .and_then(|slot| slot.ready())
            .map(|(image, _)| image)
    }
}

/// `image` wrapped for GPUI, which wants BGRA.
fn render_image(mut image: image::RgbaImage) -> Arc<RenderImage> {
    for pixel in image.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
        image,
    )])))
}

/// The radial gradient around its center, `2 × radius` across (tdesktop
/// `PaintAppearanceGradient`: OKLab stops mixed linearly between).
fn backdrop_pixels(center: Rgb, edge: Rgb) -> image::RgbaImage {
    let radius = backdrop_radius() * BACKDROP_SCALE;
    let side = (2. * radius).ceil().max(1.) as u32;
    let stops = gradient_stops(center, edge);
    let channel = |rgb: Rgb, shift: u32| f64::from((rgb.to_hex() >> shift) & 0xFF);
    image::RgbaImage::from_fn(side, side, |x, y| {
        let dx = f64::from(x) + 0.5 - radius;
        let dy = f64::from(y) + 0.5 - radius;
        let t = ((dx * dx + dy * dy).sqrt() / radius).clamp(0., 1.) * GRADIENT_STEPS as f64;
        let index = (t.floor() as usize).min(GRADIENT_STEPS.saturating_sub(1));
        let fraction = t - index as f64;
        let from = stops.get(index).copied().unwrap_or(edge);
        let till = stops.get(index + 1).copied().unwrap_or(edge);
        let mix = |shift| {
            let a = channel(from, shift);
            (a + (channel(till, shift) - a) * fraction)
                .round()
                .clamp(0., 255.) as u8
        };
        image::Rgba([mix(16), mix(8), mix(0), 255])
    })
}

/// A system-font emoji drawn through the SVG renderer's emoji fallback.
fn emoji_svg(emoji: &str) -> String {
    let escaped = emoji
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><text x="32" y="53" font-size="54" text-anchor="middle">{escaped}</text></svg>"#
    )
}

fn rasterize(
    renderer: &SvgRenderer,
    svg: &str,
    width: u32,
    height: u32,
) -> Option<Arc<RenderImage>> {
    let parsed = renderer.parse_svg(svg.as_bytes()).ok()?;
    let size = size(
        DevicePixels(i32::try_from(width).ok()?),
        DevicePixels(i32::try_from(height).ok()?),
    );
    renderer
        .render_parsed(&parsed, SvgSize::ExactSize(size))
        .ok()
}

impl QuillApp {
    /// Start making what the open box draws and has not got yet: its
    /// backdrop, its code and the stills of the picked emoji.
    pub(super) fn qr_prepare_art(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.qr_view() else {
            return;
        };
        self.ensure_qr_backdrop(view.appearance.center, view.appearance.edge, cx);
        if let Some(shown) = &view.shown {
            let side = card_layout(CARD_WIDTH, view.peer_box()).qr.width;
            self.ensure_qr_code(&shown.link, view.colors.ink, side, cx);
        }
        let emoji: Vec<String> = self
            .peer_qr
            .open
            .as_ref()
            .map(|open| {
                let previous = open
                    .swap
                    .iter()
                    .flat_map(|swap| swap.previous.iter().cloned());
                super::plain_picks(&open.state.emoji)
                    .into_iter()
                    .chain(previous)
                    .collect()
            })
            .unwrap_or_default();
        for emoji in emoji {
            self.ensure_qr_still(&emoji, cx);
        }
    }

    fn ensure_qr_backdrop(&mut self, center: Rgb, edge: Rgb, cx: &mut Context<Self>) {
        let key = (center, edge);
        let art = &mut self.peer_qr.art;
        if art.backdrops.iter().any(|(k, _)| *k == key) {
            return;
        }
        art.backdrops.push((key, Slot::Loading));
        self.run_off_ui_then(
            move || render_image(backdrop_pixels(center, edge)),
            move |this, image, cx| {
                let art = &mut this.peer_qr.art;
                if let Some((_, slot)) = art.backdrops.iter_mut().find(|(k, _)| *k == key) {
                    *slot = Slot::Ready {
                        image,
                        at: Instant::now(),
                    };
                    trim(&mut art.backdrops);
                }
                cx.notify();
            },
            cx,
        );
    }

    fn ensure_qr_code(&mut self, link: &str, ink: Rgb, side: f64, cx: &mut Context<Self>) {
        let key = (link.to_string(), ink);
        let art = &mut self.peer_qr.art;
        if art.codes.iter().any(|(k, _)| *k == key) {
            return;
        }
        art.codes.push((key.clone(), Slot::Loading));
        let renderer = cx.svg_renderer();
        let link = link.to_string();
        let pixels = (side * CODE_SCALE).round().max(1.) as u32;
        self.run_off_ui_then(
            move || {
                let grid = QrGrid::encode(&link)?;
                rasterize(
                    &renderer,
                    &qr_svg(&grid, side, ink.to_hex()),
                    pixels,
                    pixels,
                )
            },
            move |this, image, cx| {
                let art = &mut this.peer_qr.art;
                if let Some((_, slot)) = art.codes.iter_mut().find(|(k, _)| *k == key) {
                    *slot = match image {
                        Some(image) => Slot::Ready {
                            image,
                            at: Instant::now(),
                        },
                        None => Slot::Failed,
                    };
                    trim(&mut art.codes);
                }
                cx.notify();
            },
            cx,
        );
    }

    fn ensure_qr_still(&mut self, emoji: &str, cx: &mut Context<Self>) {
        if super::super::emoji_sprites::sprite(emoji).is_some()
            || self.peer_qr.art.stills.contains_key(emoji)
        {
            return;
        }
        self.peer_qr
            .art
            .stills
            .insert(emoji.to_string(), Slot::Loading);
        let renderer = cx.svg_renderer();
        let svg = emoji_svg(emoji);
        let key = emoji.to_string();
        self.run_off_ui_then(
            move || rasterize(&renderer, &svg, STILL_PIXELS, STILL_PIXELS),
            move |this, image, cx| {
                if let Some(slot) = this.peer_qr.art.stills.get_mut(&key) {
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

    /// The backdrop layers to draw, bottom first, each with its opacity:
    /// the previous backdrop under a new one while it fades in.
    pub(super) fn qr_backdrops(
        &self,
        center: Rgb,
        edge: Rgb,
        now: Instant,
    ) -> Vec<(Arc<RenderImage>, f32)> {
        let key = (center, edge);
        let ready: Vec<_> = self
            .peer_qr
            .art
            .backdrops
            .iter()
            .filter_map(|(k, slot)| slot.ready().map(|ready| (*k == key, ready)))
            .collect();
        let wanted = ready.iter().find(|(current, _)| *current);
        let older = ready.iter().rev().find(|(current, _)| !*current);
        let mut layers = Vec::new();
        match (wanted, older) {
            (Some((_, (image, at))), Some((_, (old, _)))) => {
                let shown = now.duration_since(*at).as_secs_f32() / BACKDROP_FADE.as_secs_f32();
                if shown < 1. {
                    layers.push((old.clone(), 1.));
                }
                layers.push((image.clone(), shown.clamp(0., 1.)));
            }
            (Some((_, (image, _))), None) => layers.push((image.clone(), 1.)),
            (None, Some((_, (old, _)))) => layers.push((old.clone(), 1.)),
            (None, None) => {}
        }
        layers
    }

    /// The code for `link` in `ink`, or the one it replaces while the new
    /// one is made.
    pub(super) fn qr_code_image(&self, link: &str, ink: Rgb) -> Option<Arc<RenderImage>> {
        let codes = &self.peer_qr.art.codes;
        codes
            .iter()
            .find(|(k, _)| k.0 == link && k.1 == ink)
            .and_then(|(_, slot)| slot.ready())
            .or_else(|| codes.iter().rev().find_map(|(_, slot)| slot.ready()))
            .map(|(image, _)| image)
    }

    /// The picture of `emoji` in the pattern: the emoji set's sprite, or
    /// the system glyph once drawn.
    pub(super) fn qr_emoji_still(&self, emoji: &str) -> Option<Arc<RenderImage>> {
        super::super::emoji_sprites::sprite(emoji).or_else(|| self.peer_qr.art.still(emoji))
    }
}

#[cfg(test)]
mod tests {
    use super::{Rgb, backdrop_pixels};

    #[test]
    fn the_gradient_runs_from_center_to_edge() {
        let center = Rgb::hex(0xFF_0000);
        let edge = Rgb::hex(0x00_00FF);
        let image = backdrop_pixels(center, edge);
        let side = image.width();
        let middle = image.get_pixel(side / 2, side / 2);
        let corner = image.get_pixel(0, 0);
        assert!(middle[0] > 240 && middle[2] < 20, "{middle:?}");
        assert_eq!(corner.0, [0, 0, 255, 255]);
    }
}
