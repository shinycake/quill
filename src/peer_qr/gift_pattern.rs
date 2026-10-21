//! The upgraded-gift pattern behind the card (tdesktop
//! `peer_qr_gift_field.cpp`): the gift's model on a wide staggered grid
//! and its symbol, faint and tinted, on a finer one around the models.
//! Pure math in logical pixels, shared by the live box and the saved
//! image.

use super::pattern::{Falloff, node_random_salt, place_item};
use std::f64::consts::{PI, SQRT_2};

const MODEL_SCALE_MIN: f64 = 0.93;
const MODEL_SCALE_MAX: f64 = 1.07;
const SYMBOL_SCALE_MIN: f64 = 0.75;
const SYMBOL_SCALE_RANGE: f64 = 0.5;
const SYMBOL_EMPTY_SHARE: f64 = 0.39;
const CLEARANCE: f64 = 0.8;
/// tdesktop `kSymbolAlpha`.
pub const SYMBOL_ALPHA: f64 = 0.37;
/// One model's pop and the spread of the pops.
pub const MODEL_POP_MS: i64 = 315;
pub const MODEL_STAGGER_MS: i64 = 200;
/// One symbol's fade and the spread of the fades.
pub const SYMBOL_FADE_MS: i64 = 300;
pub const SYMBOL_STAGGER_MS: i64 = 300;
/// The whole entry, from the first pop to the last fade.
pub const ENTRY_MS: i64 = if SYMBOL_STAGGER_MS + SYMBOL_FADE_MS > MODEL_STAGGER_MS + MODEL_POP_MS {
    SYMBOL_STAGGER_MS + SYMBOL_FADE_MS
} else {
    MODEL_STAGGER_MS + MODEL_POP_MS
};

#[derive(Clone, Copy)]
enum Salt {
    ModelRadius = 101,
    ModelAngle,
    ModelScale,
    /// tdesktop tilts each model by this; GPUI draws images unrotated.
    #[allow(dead_code)]
    ModelTilt,
    ModelDelay,
    SymbolEmpty,
    SymbolX,
    SymbolY,
    SymbolHeight,
    SymbolDelay,
}

fn random(row: i32, column: i32, salt: Salt) -> f64 {
    node_random_salt(row, column, salt as u32)
}

/// Grid spacing and sizes (tdesktop `PeerQrGiftMetrics::Live`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiftMetrics {
    pub model_size: f64,
    pub model_skip_x: f64,
    pub model_skip_y: f64,
    pub model_offset: f64,
    pub symbol_size: f64,
    pub symbol_skip_x: f64,
    pub symbol_skip_y: f64,
    pub symbol_jitter_x: f64,
    pub symbol_jitter_y: f64,
}

impl GiftMetrics {
    /// `peerQrGift*`.
    pub const LIVE: GiftMetrics = GiftMetrics {
        model_size: 78.,
        model_skip_x: 150.,
        model_skip_y: 132.,
        model_offset: 16.,
        symbol_size: 20.,
        symbol_skip_x: 39.,
        symbol_skip_y: 33.,
        symbol_jitter_x: 17.,
        symbol_jitter_y: 14.,
    };
}

/// One model or symbol of the pattern, centered on `(x, y)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiftItem {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// `0..1`: the item's share of the entry stagger.
    pub delay: f64,
}

fn shift(row: i32) -> f64 {
    if row & 1 != 0 { 0.5 } else { 0. }
}

/// A model's flat center and size (tdesktop `ModelFlat`).
fn model_flat(row: i32, column: i32, origin: (f64, f64), m: &GiftMetrics) -> ((f64, f64), f64) {
    let radius = m.model_offset * random(row, column, Salt::ModelRadius);
    let angle = 2. * PI * random(row, column, Salt::ModelAngle);
    let flat = (
        origin.0 + (f64::from(column) + shift(row)) * m.model_skip_x + radius * angle.cos(),
        origin.1 + f64::from(row) * m.model_skip_y + radius * angle.sin(),
    );
    let size = m.model_size
        * (MODEL_SCALE_MIN
            + (MODEL_SCALE_MAX - MODEL_SCALE_MIN) * random(row, column, Salt::ModelScale));
    (flat, size)
}

/// A symbol at `flat` would touch a model (tdesktop `NearModel`).
fn near_model(flat: (f64, f64), size: f64, origin: (f64, f64), m: &GiftMetrics) -> bool {
    let row_near = ((flat.1 - origin.1) / m.model_skip_y).round() as i32;
    for row in row_near - 1..=row_near + 1 {
        let column_near = ((flat.0 - origin.0) / m.model_skip_x - shift(row)).round() as i32;
        for column in column_near - 1..=column_near + 1 {
            let (model, model_size) = model_flat(row, column, origin, m);
            let (dx, dy) = (flat.0 - model.0, flat.1 - model.1);
            let clearance = CLEARANCE * (model_size + size) / 2.;
            if dx * dx + dy * dy < clearance * clearance {
                return true;
            }
        }
    }
    false
}

/// Row and column ranges of a grid `skip` apart covering `reach`.
fn rows(reach: &super::pattern::Rect, origin: f64, skip: f64) -> std::ops::RangeInclusive<i32> {
    let from = ((reach.y - origin) / skip).ceil() as i32;
    let till = ((reach.y + reach.height - origin) / skip).floor() as i32;
    from..=till
}

fn columns(
    reach: &super::pattern::Rect,
    origin: f64,
    skip: f64,
    row: i32,
) -> std::ops::RangeInclusive<i32> {
    let from = ((reach.x - origin) / skip - shift(row)).ceil() as i32;
    let till = ((reach.x + reach.width - origin) / skip - shift(row)).floor() as i32;
    from..=till
}

/// The models, grid centered on `origin` (tdesktop `EnumerateModels`).
pub fn gift_models(falloff: &Falloff, origin: (f64, f64), m: &GiftMetrics) -> Vec<GiftItem> {
    let mut out = Vec::new();
    if m.model_skip_x <= 0. || m.model_skip_y <= 0. {
        return out;
    }
    let reach = falloff.reach(m.model_offset + m.model_size * MODEL_SCALE_MAX / SQRT_2);
    for row in rows(&reach, origin.1, m.model_skip_y) {
        for column in columns(&reach, origin.0, m.model_skip_x, row) {
            let (flat, size) = model_flat(row, column, origin, m);
            let Some(place) = place_item(falloff, flat, size) else {
                continue;
            };
            out.push(GiftItem {
                x: place.x,
                y: place.y,
                size: size * place.scale,
                delay: random(row, column, Salt::ModelDelay),
            });
        }
    }
    out
}

/// The symbols, kept clear of the models (tdesktop `EnumerateSymbols`).
pub fn gift_symbols(falloff: &Falloff, origin: (f64, f64), m: &GiftMetrics) -> Vec<GiftItem> {
    let mut out = Vec::new();
    if m.symbol_skip_x <= 0.
        || m.symbol_skip_y <= 0.
        || m.model_skip_x <= 0.
        || m.model_skip_y <= 0.
    {
        return out;
    }
    let reach = falloff.reach(
        m.symbol_jitter_x.max(m.symbol_jitter_y)
            + m.symbol_size * (SYMBOL_SCALE_MIN + SYMBOL_SCALE_RANGE) / 2.,
    );
    for row in rows(&reach, origin.1, m.symbol_skip_y) {
        for column in columns(&reach, origin.0, m.symbol_skip_x, row) {
            let r = |salt| random(row, column, salt);
            if r(Salt::SymbolEmpty) < SYMBOL_EMPTY_SHARE {
                continue;
            }
            let flat = (
                origin.0
                    + m.symbol_jitter_x * (2. * r(Salt::SymbolX) - 1.)
                    + (f64::from(column) + shift(row)) * m.symbol_skip_x,
                origin.1
                    + m.symbol_jitter_y * (2. * r(Salt::SymbolY) - 1.)
                    + f64::from(row) * m.symbol_skip_y,
            );
            let height = r(Salt::SymbolHeight);
            let size = m.symbol_size * (SYMBOL_SCALE_MIN + SYMBOL_SCALE_RANGE * height * height);
            let Some(place) = place_item(falloff, flat, size) else {
                continue;
            };
            if near_model(flat, size, origin, m) {
                continue;
            }
            out.push(GiftItem {
                x: place.x,
                y: place.y,
                size: size * place.scale,
                delay: r(Salt::SymbolDelay),
            });
        }
    }
    out
}

fn progress(now_ms: i64, start_ms: i64, duration_ms: i64) -> f64 {
    if duration_ms <= 0 {
        return 1.;
    }
    ((now_ms - start_ms) as f64 / duration_ms as f64).clamp(0., 1.)
}

/// A model's scale `now_ms` into the entry: an overshooting pop.
pub fn model_scale(now_ms: i64, delay: f64) -> f64 {
    // The emoji pattern's pop: the same easing over the same 315 ms.
    super::pattern::pop_scale(now_ms, (MODEL_STAGGER_MS as f64 * delay) as i64)
}

/// A symbol's opacity `now_ms` into the entry, out of [`SYMBOL_ALPHA`].
pub fn symbol_opacity(now_ms: i64, delay: f64) -> f64 {
    let start = (SYMBOL_STAGGER_MS as f64 * delay) as i64;
    let t = progress(now_ms, start, SYMBOL_FADE_MS);
    SYMBOL_ALPHA * (1. - (1. - t).powi(3))
}
