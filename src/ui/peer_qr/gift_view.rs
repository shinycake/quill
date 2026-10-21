//! The upgraded-gift pattern behind the card, one frame at a time
//! (tdesktop `PeerQrGiftField::paint`): the faint tinted symbols fading
//! in, then the models popping in, on the layer that drifts like the
//! emoji pattern. Each layer starts once its still is made. Reduced
//! motion and power saving show it settled.
#![deny(clippy::indexing_slicing)]

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::peer_qr::gift_pattern::{
    ENTRY_MS, GiftMetrics, SYMBOL_ALPHA, gift_models, gift_symbols, model_scale, symbol_opacity,
};
use quill::peer_qr::pattern::{Falloff, LAYER_BAND, Rect, drift_offset, overlaps};
use quill::peer_qr::theme::ThemeGift;
use std::sync::Arc;
use std::time::Instant;

/// Symbols are drawn in this many opacity steps: GPUI paints images at
/// full opacity, so each step is its own layer.
const OPACITY_STEPS: f64 = 12.;

/// One picture centered on `(x, y)`.
struct Placed {
    x: f64,
    y: f64,
    size: f64,
}

/// A frame of the gift pattern and whether the next one differs.
pub(super) struct GiftFrame {
    symbol: Option<Arc<RenderImage>>,
    /// The symbols by opacity step.
    symbols: Vec<(f32, Vec<Placed>)>,
    model: Option<Arc<RenderImage>>,
    models: Vec<Placed>,
    pub(super) moving: bool,
}

pub(super) struct GiftArgs<'a> {
    pub(super) gift: &'a ThemeGift,
    /// When the gift began to show.
    pub(super) entry: Instant,
    pub(super) opened: Instant,
    pub(super) now: Instant,
    pub(super) animate: bool,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) card: Rect,
    pub(super) glass: &'a [Rect],
}

fn ms(from: Instant, till: Instant) -> i64 {
    i64::try_from(till.saturating_duration_since(from).as_millis()).unwrap_or(i64::MAX)
}

impl QuillApp {
    pub(super) fn qr_gift_frame(&self, args: GiftArgs<'_>) -> GiftFrame {
        let drift = if args.animate {
            ms(args.opened, args.now)
        } else {
            0
        };
        let (dx, dy) = drift_offset(drift);
        let (cx0, cy0) = args.card.center();
        let origin = (cx0 + dx, cy0 + dy);
        let falloff = Falloff {
            area: Rect::new(0., 0., args.width, args.height),
            band: LAYER_BAND,
        };
        let covered = |x: f64, y: f64, size: f64| {
            std::iter::once(&args.card)
                .chain(args.glass)
                .any(|rect| overlaps(rect, x, y, size))
        };
        // Each layer comes in from when both the gift shows and its still
        // is made (tdesktop `std::max(entryAt, readyAt)`).
        let since = |ready: Instant| {
            if args.animate {
                ms(args.entry.max(ready), args.now)
            } else {
                ENTRY_MS
            }
        };
        let symbol = self.qr_gift_symbol(args.gift);
        let mut symbols: Vec<(f32, Vec<Placed>)> = Vec::new();
        if let Some((_, ready)) = &symbol {
            let elapsed = since(*ready);
            for item in gift_symbols(&falloff, origin, &GiftMetrics::LIVE) {
                let opacity = symbol_opacity(elapsed, item.delay);
                if opacity <= 0. || covered(item.x, item.y, item.size) {
                    continue;
                }
                let step = (opacity / SYMBOL_ALPHA * OPACITY_STEPS).round() / OPACITY_STEPS;
                let level = (step * SYMBOL_ALPHA) as f32;
                if level <= 0. {
                    continue;
                }
                let placed = Placed {
                    x: item.x,
                    y: item.y,
                    size: item.size,
                };
                match symbols.iter_mut().find(|(at, _)| *at == level) {
                    Some((_, list)) => list.push(placed),
                    None => symbols.push((level, vec![placed])),
                }
            }
        }
        let model = self.qr_gift_model(args.gift);
        let mut models = Vec::new();
        if let Some((_, ready)) = &model {
            let elapsed = since(*ready);
            for item in gift_models(&falloff, origin, &GiftMetrics::LIVE) {
                let size = item.size * model_scale(elapsed, item.delay);
                if size <= 0. || covered(item.x, item.y, size) {
                    continue;
                }
                models.push(Placed {
                    x: item.x,
                    y: item.y,
                    size,
                });
            }
        }
        GiftFrame {
            symbol: symbol.map(|(image, _)| image),
            symbols,
            model: model.map(|(image, _)| image),
            models,
            moving: args.animate,
        }
    }
}

fn layer(image: Arc<RenderImage>, items: Vec<Placed>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            for item in items {
                let half = item.size / 2.;
                let at =
                    bounds.origin + point(px((item.x - half) as f32), px((item.y - half) as f32));
                let side = px(item.size as f32);
                let _ = window.paint_image(
                    bounds,
                    Bounds::new(at, size(side, side)),
                    Corners::default(),
                    image.clone(),
                    0,
                    false,
                );
            }
        },
    )
    .size_full()
}

/// The frame filling the box: symbols under the models.
pub(super) fn gift_canvas(frame: GiftFrame) -> impl IntoElement {
    let GiftFrame {
        symbol,
        symbols,
        model,
        models,
        ..
    } = frame;
    let symbol_layers: Vec<Div> = match symbol {
        Some(image) => symbols
            .into_iter()
            .map(|(opacity, items)| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .opacity(opacity)
                    .child(layer(image.clone(), items))
            })
            .collect(),
        None => Vec::new(),
    };
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .children(symbol_layers)
        .children(model.map(|image| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(layer(image, models))
        }))
}
