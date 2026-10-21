//! The emoji pattern behind the card (tdesktop `peer_qr_emoji_field.cpp`
//! and `peer_qr_layer.cpp`): a staggered grid of the picked emoji that
//! shrinks towards the box edges, bobs, drifts slowly and pops in. Pure
//! math in logical pixels, shared by the live box and the saved image.

use std::f64::consts::{PI, SQRT_2};

const EMPTY_SHARE: f64 = 0.2;
const BIG_TIER: f64 = 1.17;
const SMALL_TIER: f64 = 0.9;
const NODE_SCALE_MIN: f64 = 0.92;
const NODE_SCALE_MAX: f64 = 1.08;
const BOB_SPEED: f64 = 0.45;
const SCALE_CUTOFF: f64 = 0.05;
const CARD_SHRINK: f64 = 0.03;
const FALLOFF_MIN: f64 = 0.15;
const TABLE_SIZE: usize = 64;
const HEADING_DEGREES: f64 = 225.;
const WANDER_AMPLITUDE: f64 = 1.2;
const WANDER_SPEED: f64 = 0.025;
const WANDER_SLOW_AMPLITUDE: f64 = 0.6;
const WANDER_SLOW_SPEED: f64 = 0.016;
const WANDER_SLOW_PHASE: f64 = 1.;
const DRIFT_STEP_MS: i64 = 1000;
const BACK_OVERSHOOT: f64 = 1.70158;

/// Milliseconds between the first and the last tile popping in.
pub const ENTRY_STAGGER_MS: i64 = 60;
/// One tile's pop.
pub const POP_MS: i64 = 315;
/// One leaving tile's shrink.
pub const SHRINK_MS: i64 = 180;
/// A swapped tile pops this long after its old emoji starts shrinking.
pub const POP_DELAY_MS: i64 = 135;
/// Spread of a swap across the tiles.
pub const SWAP_STAGGER_MS: i64 = 300;
/// A whole swap, from the change to the last pop.
pub const SWAP_MS: i64 = SWAP_STAGGER_MS
    + if SHRINK_MS > POP_DELAY_MS + POP_MS {
        SHRINK_MS
    } else {
        POP_DELAY_MS + POP_MS
    };
/// `peerQrLayerBand`: the edge band where tiles shrink.
pub const LAYER_BAND: f64 = 98.;
/// `peerQrDriftSpeed`, pixels per second.
pub const DRIFT_SPEED: f64 = 6.;

#[derive(Clone, Copy)]
enum Salt {
    Empty = 1,
    Scale,
    Radius = 4,
    Angle,
    Bob = 7,
    Delay,
}

/// tdesktop `PeerQrHash`.
pub fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    value
}

/// tdesktop `PeerQrNodeHash`.
pub fn node_hash(row: i32, column: i32, salt: u32) -> u32 {
    let seeded = hash(salt.wrapping_mul(0x9E37_79B9).wrapping_add(row as u32));
    hash(seeded ^ (column as u32).wrapping_mul(0x85EB_CA6B))
}

fn node_random(row: i32, column: i32, salt: Salt) -> f64 {
    f64::from(node_hash(row, column, salt as u32)) / 4_294_967_296.
}

/// Which pick a grid node shows: picks repeat along diagonals.
pub fn pick_index(row: i32, column: i32, count: usize) -> usize {
    let Ok(count) = i32::try_from(count) else {
        return 0;
    };
    if count <= 0 {
        return 0;
    }
    let q = column - (row - (row & 1)) / 2;
    usize::try_from((q - row).rem_euclid(count)).unwrap_or(0)
}

/// An axis-aligned rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2., self.y + self.height / 2.)
    }

    fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }
}

/// Grid spacing and tile size (`PeerQrEmojiMetrics`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub skip_x: f64,
    pub skip_y: f64,
    pub size: f64,
    pub offset_min: f64,
    pub offset_max: f64,
    pub bob: f64,
}

impl Metrics {
    /// The box's metrics (`peerQrEmoji*`).
    pub const LIVE: Metrics = Metrics {
        skip_x: 80.,
        skip_y: 66.,
        size: 32.,
        offset_min: 3.,
        offset_max: 10.,
        bob: 3.,
    };

    pub fn scaled(self, by: f64) -> Metrics {
        Metrics {
            skip_x: self.skip_x * by,
            skip_y: self.skip_y * by,
            size: self.size * by,
            offset_min: self.offset_min * by,
            offset_max: self.offset_max * by,
            bob: self.bob * by,
        }
    }
}

fn factor(t: f64) -> f64 {
    let u = t.clamp(0., 1.);
    FALLOFF_MIN + (1. - FALLOFF_MIN) * u * u * (3. - 2. * u)
}

/// `table[i]` integrates `1 / factor` from `i / TABLE_SIZE` to 1.
fn integrals() -> &'static [f64; TABLE_SIZE + 1] {
    static TABLE: std::sync::OnceLock<[f64; TABLE_SIZE + 1]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.; TABLE_SIZE + 1];
        let step = 1. / TABLE_SIZE as f64;
        let mut sum = 0.;
        for (i, slot) in table.iter_mut().enumerate().take(TABLE_SIZE).rev() {
            let from = i as f64 * step;
            let simpson =
                1. / factor(from) + 4. / factor(from + step / 2.) + 1. / factor(from + step);
            sum += simpson * step / 6.;
            *slot = sum;
        }
        table
    })
}

fn table_at(index: usize) -> f64 {
    integrals().get(index).copied().unwrap_or(0.)
}

fn integral(t: f64) -> f64 {
    let position = t.clamp(0., 1.) * TABLE_SIZE as f64;
    let index = (position as usize).min(TABLE_SIZE - 1);
    let fraction = position - index as f64;
    table_at(index) + (table_at(index + 1) - table_at(index)) * fraction
}

fn integral_inverse(value: f64) -> f64 {
    let table = integrals();
    let first_not_greater = table
        .iter()
        .position(|entry| *entry <= value)
        .unwrap_or(table.len());
    let till = first_not_greater.clamp(1, TABLE_SIZE);
    let from = till - 1;
    let span = table_at(from) - table_at(till);
    let fraction = if span > 0. {
        (table_at(from) - value) / span
    } else {
        0.
    };
    (from as f64 + fraction.clamp(0., 1.)) / TABLE_SIZE as f64
}

fn k(d: f64, band: f64) -> f64 {
    if d >= band {
        band - d
    } else if d >= 0. {
        band * integral(d / band)
    } else {
        band * table_at(0) - d / FALLOFF_MIN
    }
}

fn k_inverse(value: f64, band: f64) -> f64 {
    let edge = band * table_at(0);
    if value <= 0. {
        band - value
    } else if value <= edge {
        band * integral_inverse(value / band)
    } else {
        (edge - value) * FALLOFF_MIN
    }
}

/// Maps one axis; the factor follows the on-screen distance from the
/// side, so the size curve holds wherever the layer has drifted.
fn map_axis(flat: f64, center: f64, half: f64, band: f64) -> (f64, f64) {
    if band <= 0. {
        return (flat, 1.);
    }
    let shift = flat - center;
    let d = k_inverse(shift.abs() + k(half, band), band);
    let distance = half - d;
    let screen = center + if shift < 0. { -distance } else { distance };
    let scale = if d >= 0. {
        factor(d / band)
    } else {
        FALLOFF_MIN
    };
    (screen, scale)
}

/// The lens that pulls the layer's edge tiles in and shrinks them
/// (`PeerQrLayerFalloff`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Falloff {
    pub area: Rect,
    pub band: f64,
}

/// Where a tile lands and how much it shrinks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

impl Falloff {
    pub fn place(&self, flat: (f64, f64)) -> Place {
        let (cx, cy) = self.area.center();
        let (x, sx) = map_axis(flat.0, cx, self.area.width / 2., self.band);
        let (y, sy) = map_axis(flat.1, cy, self.area.height / 2., self.band);
        Place {
            x,
            y,
            scale: sx * sy,
        }
    }

    /// The flat rectangle whose tiles can land in the area.
    pub fn reach(&self, margin: f64) -> Rect {
        let flat_half = |half: f64| k(0., self.band) - k(half, self.band) + margin;
        let (x, y) = (
            flat_half(self.area.width / 2.),
            flat_half(self.area.height / 2.),
        );
        let (cx, cy) = self.area.center();
        Rect::new(cx - x, cy - y, 2. * x, 2. * y)
    }
}

/// One emoji of the pattern.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    /// `0..1`: the tile's share of the entry and swap staggers.
    pub delay: f64,
    pub row: i32,
    pub column: i32,
    pub pick: usize,
}

fn place_item(falloff: &Falloff, flat: (f64, f64), size: f64) -> Option<Place> {
    let place = falloff.place(flat);
    if place.scale < SCALE_CUTOFF {
        return None;
    }
    let extent = size * place.scale / SQRT_2;
    let bounds = Rect::new(place.x - extent, place.y - extent, 2. * extent, 2. * extent);
    bounds.intersects(&falloff.area).then_some(place)
}

/// The tiles of `count` picks, `seconds` into the animation, with the
/// grid's center at `origin` (tdesktop `EnumerateTiles`).
pub fn tiles(
    falloff: &Falloff,
    origin: (f64, f64),
    metrics: &Metrics,
    count: usize,
    seconds: f64,
) -> Vec<Tile> {
    let mut out = Vec::new();
    if count == 0 || metrics.skip_x <= 0. || metrics.skip_y <= 0. {
        return out;
    }
    let reach = falloff
        .reach(metrics.offset_max + metrics.bob + metrics.size * BIG_TIER * NODE_SCALE_MAX / 2.);
    let row_from = ((reach.y - origin.1) / metrics.skip_y).ceil() as i32;
    let row_till = ((reach.y + reach.height - origin.1) / metrics.skip_y).floor() as i32;
    for row in row_from..=row_till {
        let odd = (row & 1) != 0;
        let shift = if odd { 0.5 } else { 0. };
        let tier = if odd { SMALL_TIER } else { BIG_TIER };
        let column_from = ((reach.x - origin.0) / metrics.skip_x - shift).ceil() as i32;
        let column_till =
            ((reach.x + reach.width - origin.0) / metrics.skip_x - shift).floor() as i32;
        for column in column_from..=column_till {
            let random = |salt| node_random(row, column, salt);
            if random(Salt::Empty) < EMPTY_SHARE {
                continue;
            }
            let radius = metrics.offset_min
                + (metrics.offset_max - metrics.offset_min) * random(Salt::Radius);
            let angle = 2. * PI * random(Salt::Angle);
            let bob = metrics.bob * (BOB_SPEED * seconds + 2. * PI * random(Salt::Bob)).sin();
            let flat = (
                origin.0 + (f64::from(column) + shift) * metrics.skip_x + radius * angle.cos(),
                origin.1 + f64::from(row) * metrics.skip_y + radius * angle.sin() + bob,
            );
            let scale = NODE_SCALE_MIN + (NODE_SCALE_MAX - NODE_SCALE_MIN) * random(Salt::Scale);
            let size = metrics.size * tier * scale;
            let Some(place) = place_item(falloff, flat, size) else {
                continue;
            };
            out.push(Tile {
                x: place.x,
                y: place.y,
                size: size * place.scale,
                delay: random(Salt::Delay),
                row,
                column,
                pick: pick_index(row, column, count),
            });
        }
    }
    out
}

/// The card, dipped a little, for [`covered`] (tdesktop `PeerQrCardCull`).
pub fn card_cull(card: Rect) -> Rect {
    let x = card.width * CARD_SHRINK / 2.;
    let y = card.height * CARD_SHRINK / 2.;
    Rect::new(
        card.x + x,
        card.y + y,
        card.width - 2. * x,
        card.height - 2. * y,
    )
}

/// A tile wholly under the card is not drawn: one inside the ellipse
/// inscribed in the dipped card is covered for any corner radius.
pub fn covered(cull: &Rect, x: f64, y: f64, size: f64) -> bool {
    if cull.width <= 0. || cull.height <= 0. {
        return false;
    }
    let radius = size / SQRT_2;
    let (cx, cy) = cull.center();
    let dx = ((x - cx).abs() + radius) / (cull.width / 2.);
    let dy = ((y - cy).abs() + radius) / (cull.height / 2.);
    dx * dx + dy * dy <= 1.
}

/// The layer's slow drift after `elapsed_ms` (tdesktop
/// `PeerQrLayerMotion::advance` with no drag).
pub fn drift_offset(elapsed_ms: i64) -> (f64, f64) {
    let (mut x, mut y) = (0., 0.);
    let mut done = 0;
    while done < elapsed_ms {
        let step = (elapsed_ms - done).min(DRIFT_STEP_MS);
        let middle = (done as f64 + step as f64 / 2.) / 1000.;
        let wander = WANDER_AMPLITUDE * (WANDER_SPEED * middle).sin()
            + WANDER_SLOW_AMPLITUDE * (WANDER_SLOW_SPEED * middle + WANDER_SLOW_PHASE).sin();
        let heading = HEADING_DEGREES.to_radians() + wander;
        let distance = DRIFT_SPEED * step as f64 / 1000.;
        x += heading.cos() * distance;
        y += heading.sin() * distance;
        done += step;
    }
    (x, y)
}

fn ease_out_back(t: f64) -> f64 {
    let t = t - 1.;
    1. + (BACK_OVERSHOOT + 1.) * t * t * t + BACK_OVERSHOOT * t * t
}

/// A tile's scale `now_ms` after its pop started at `start_ms`.
pub fn pop_scale(now_ms: i64, start_ms: i64) -> f64 {
    if now_ms <= start_ms {
        0.
    } else if now_ms >= start_ms + POP_MS {
        1.
    } else {
        ease_out_back((now_ms - start_ms) as f64 / POP_MS as f64)
    }
}

/// A leaving tile's scale `now_ms` after its shrink started.
pub fn shrink_scale(now_ms: i64, start_ms: i64) -> f64 {
    if now_ms <= start_ms {
        1.
    } else if now_ms >= start_ms + SHRINK_MS {
        0.
    } else {
        let t = (now_ms - start_ms) as f64 / SHRINK_MS as f64;
        1. - t * t * t
    }
}
