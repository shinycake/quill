//! Telegram's spoiler "mess" (lib_ui `spoiler_mess.cpp`): a 128 pt tile
//! of tiny specks, each living 600 ms as it fades in, drifts a few points
//! and fades out, their starts spread over a 2 s loop, the tile repeated
//! over whatever it hides.
//!
//! - Media spoilers draw white specks over the blurred preview from 60
//!   pre-rendered frames of the tile (one multi-frame image, tiled).
//! - Text spoilers draw the specks as quads in the text's color, inside
//!   the hidden run's rectangles.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Instant;

/// Tile side, in points.
const CANVAS: f32 = 128.;
/// lib_ui `kDefaultFramesCount` × `kDefaultFrameDuration`.
const FRAMES: usize = 60;
const FRAME_MS: f32 = 33.;
const LOOP_MS: f32 = FRAMES as f32 * FRAME_MS;
/// Device pixels per point the image tile is rasterized at.
const TILE_SCALE: f32 = 2.;

/// lib_ui `SpoilerMessDescriptor`.
struct Descriptor {
    fade_in_ms: f32,
    shown_ms: f32,
    fade_out_ms: f32,
    size_min: f32,
    size_max: f32,
    speed_min: f32,
    speed_max: f32,
    count: usize,
}

impl Descriptor {
    fn lifetime(&self) -> f32 {
        self.fade_in_ms + self.shown_ms + self.fade_out_ms
    }

    fn start_ms(&self, index: usize) -> f32 {
        index as f32 * LOOP_MS / self.count as f32
    }

    fn index_starting_at(&self, ms: f32) -> f32 {
        ms * self.count as f32 / LOOP_MS
    }

    /// The speck's opacity `age` ms after it appeared.
    fn alpha(&self, age: f32) -> f32 {
        if age < self.fade_in_ms {
            age / self.fade_in_ms
        } else if age < self.fade_in_ms + self.shown_ms {
            1.
        } else {
            (1. - (age - self.fade_in_ms - self.shown_ms) / self.fade_out_ms).max(0.)
        }
    }
}

/// `DefaultDescriptorText`.
const TEXT: Descriptor = Descriptor {
    fade_in_ms: 200.,
    shown_ms: 200.,
    fade_out_ms: 200.,
    size_min: 1.5,
    size_max: 2.,
    speed_min: 4.,
    speed_max: 8.,
    count: 9000,
};

/// `DefaultDescriptorImage`.
const IMAGE: Descriptor = Descriptor {
    fade_in_ms: 300.,
    shown_ms: 0.,
    fade_out_ms: 300.,
    size_min: 1.5,
    size_max: 2.,
    speed_min: 10.,
    speed_max: 20.,
    count: 3000,
};

/// One speck: when it starts in the loop (ms), where (pt, in the tile),
/// its drift (pt per ms) and size (pt).
#[derive(Clone, Copy)]
struct Particle {
    start: f32,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    w: f32,
    h: f32,
}

/// A small, fixed-seed generator: the field looks the same every run.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// lib_ui `GenerateParticle`: starts spread evenly over the loop, random
/// spot and direction, speed between the descriptor's bounds, and one of
/// five sprite shapes (wider, round, or taller).
fn particles(descriptor: &Descriptor, seed: u64) -> Vec<Particle> {
    let mut random = Random(seed);
    (0..descriptor.count)
        .map(|index| {
            let speed = descriptor.speed_min
                + (descriptor.speed_max - descriptor.speed_min) * random.next();
            let direction = random.next() * std::f32::consts::TAU;
            // lib_ui's speeds are device pixels per second: points per
            // ms at 2×.
            let k = speed / TILE_SCALE / 1000.;
            let sprite = (random.next() * 5.) as usize % 5;
            let delta = descriptor.size_max - descriptor.size_min;
            let (w, h) = match sprite {
                0 => (descriptor.size_min + delta, descriptor.size_min),
                1 => (descriptor.size_min + delta / 2., descriptor.size_min),
                3 => (descriptor.size_min, descriptor.size_min + delta / 2.),
                4 => (descriptor.size_min, descriptor.size_min + delta),
                _ => (descriptor.size_min, descriptor.size_min),
            };
            Particle {
                start: descriptor.start_ms(index),
                x: random.next() * CANVAS,
                y: random.next() * CANVAS,
                dx: direction.cos() * k,
                dy: direction.sin() * k,
                w,
                h,
            }
        })
        .collect()
}

static TEXT_FIELD: LazyLock<Vec<Particle>> = LazyLock::new(|| particles(&TEXT, 0x5eed_7e47));
static IMAGE_FIELD: LazyLock<Vec<Particle>> = LazyLock::new(|| particles(&IMAGE, 0x1a6e_5eed));

/// Milliseconds into the shared loop: every spoiler on screen moves as one.
fn loop_ms() -> f32 {
    if still() {
        // A fixed moment of the loop: a few specks showing, none moving.
        return LOOP_MS / 2.;
    }
    static CLOCK: OnceLock<Instant> = OnceLock::new();
    let elapsed = CLOCK.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.;
    (elapsed % f64::from(LOOP_MS)) as f32
}

/// Where a live speck is `now` ms into the loop (tile points), and its
/// opacity; `None` while it isn't showing.
fn place(descriptor: &Descriptor, particle: &Particle, now: f32) -> Option<(f32, f32, f32)> {
    let age = (now - particle.start).rem_euclid(LOOP_MS);
    if age >= descriptor.lifetime() {
        return None;
    }
    let x = (particle.x + particle.dx * age).rem_euclid(CANVAS);
    let y = (particle.y + particle.dy * age).rem_euclid(CANVAS);
    Some((x, y, descriptor.alpha(age)))
}

/// Coverage of a rounded rectangle centered at `(cx, cy)` (pixels) at the
/// pixel whose center is `(px, py)`: an anti-aliased speck.
fn speck_coverage(px: f32, py: f32, cx: f32, cy: f32, w: f32, h: f32) -> f32 {
    let radius = w.min(h) / 2.;
    let qx = ((px - cx).abs() - (w / 2. - radius)).max(0.);
    let qy = ((py - cy).abs() - (h / 2. - radius)).max(0.);
    let distance = (qx * qx + qy * qy).sqrt() - radius;
    (0.5 - distance).clamp(0., 1.)
}

/// The media tile while media spoilers show (16 MB decoded, and as much
/// in the atlas): built on first use, released once nothing painted it for
/// `TILE_UNUSED` and no cached frame can still show it.
struct MediaTile {
    image: Arc<RenderImage>,
    painted: Instant,
    stamp: super::image_budget::PaintStamp,
}

thread_local! {
    static TILE: std::cell::RefCell<Option<MediaTile>> = const { std::cell::RefCell::new(None) };
}

const TILE_UNUSED: std::time::Duration = std::time::Duration::from_secs(3);

/// The media tile, built if needed; marks it painted now.
fn image_tile() -> Arc<RenderImage> {
    TILE.with(|tile| {
        let mut tile = tile.borrow_mut();
        let tile = tile.get_or_insert_with(|| MediaTile {
            image: build_image_tile(),
            painted: Instant::now(),
            stamp: super::image_budget::paint_stamp(),
        });
        tile.painted = Instant::now();
        tile.stamp = super::image_budget::paint_stamp();
        tile.image.clone()
    })
}

/// Drop the media tile (and its atlas copy) once media spoilers have been
/// off screen for a while. `window`: the window being drawn, if any.
pub(super) fn release_media_tile(window: Option<&mut Window>, cx: &mut App) {
    let unused = TILE.with(|tile| {
        let mut tile = tile.borrow_mut();
        let idle = tile.as_ref().is_some_and(|tile| {
            tile.painted.elapsed() >= TILE_UNUSED
                && !super::image_budget::may_still_show(tile.stamp)
        });
        if idle { tile.take() } else { None }
    });
    if let Some(tile) = unused {
        cx.drop_image(tile.image, window);
    }
}

/// The media tile: `FRAMES` frames of white specks on transparent, each
/// `CANVAS × TILE_SCALE` pixels square, seamless when repeated.
fn build_image_tile() -> Arc<RenderImage> {
    {
        let side = (CANVAS * TILE_SCALE) as u32;
        let frames: smallvec::SmallVec<[image::Frame; 1]> = (0..FRAMES)
            .map(|frame| {
                let now = frame as f32 * FRAME_MS;
                let mut alpha = vec![0f32; (side * side) as usize];
                for particle in IMAGE_FIELD.iter() {
                    let Some((x, y, opacity)) = place(&IMAGE, particle, now) else {
                        continue;
                    };
                    // lib_ui stamps its sprites on whole device pixels:
                    // crisp specks, not soft smudges.
                    let (cx, cy) = ((x * TILE_SCALE).round(), (y * TILE_SCALE).round());
                    let (w, h) = (particle.w * TILE_SCALE, particle.h * TILE_SCALE);
                    let reach = (w.max(h) / 2. + 1.).ceil() as i32;
                    for oy in -reach..=reach {
                        for ox in -reach..=reach {
                            let (pxi, pyi) = (cx.floor() as i32 + ox, cy.floor() as i32 + oy);
                            let cover =
                                speck_coverage(pxi as f32 + 0.5, pyi as f32 + 0.5, cx, cy, w, h);
                            if cover <= 0. {
                                continue;
                            }
                            // Wrap so the tile repeats without seams.
                            let (wx, wy) = (
                                pxi.rem_euclid(side as i32) as u32,
                                pyi.rem_euclid(side as i32) as u32,
                            );
                            if let Some(slot) = alpha.get_mut((wy * side + wx) as usize) {
                                *slot = (*slot + cover * opacity).min(1.);
                            }
                        }
                    }
                }
                let mut image = image::RgbaImage::new(side, side);
                for (pixel, value) in image.pixels_mut().zip(alpha) {
                    *pixel = image::Rgba([255, 255, 255, (value * 255.).round() as u8]);
                }
                image::Frame::new(image)
            })
            .collect();
        Arc::new(RenderImage::new(frames))
    }
}

/// Paint the media spoiler's specks over `bounds` (tiles anchored at its
/// top-left; the caller's clip trims the edges).
pub(super) fn paint_media_specks(bounds: Bounds<Pixels>, window: &mut Window) {
    let tile = image_tile();
    let frame = ((loop_ms() / FRAME_MS) as usize).min(FRAMES - 1);
    let step = px(CANVAS);
    let mut y = bounds.top();
    while y < bounds.bottom() {
        let mut x = bounds.left();
        while x < bounds.right() {
            let cell = Bounds::new(point(x, y), size(step, step));
            let _ = window.paint_image(cell, cell, Corners::default(), tile.clone(), frame, false);
            x += step;
        }
        y += step;
    }
}

thread_local! {
    static TEXT_PAINTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether text specks were painted since the last call: the app keeps
/// its frame clock running for them.
pub(super) fn take_text_painted() -> bool {
    TEXT_PAINTED.with(|painted| painted.replace(false))
}

/// Paint text-spoiler specks of `color` inside `rect` (window points).
/// The field is anchored to `origin`, so a run wrapped over lines keeps
/// one continuous field.
pub(super) fn paint_text_specks(
    rect: Bounds<Pixels>,
    origin: Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    TEXT_PAINTED.with(|painted| painted.set(!still()));
    draw_text_specks(rect, origin, color, window);
}

/// Text specks in `rect` for an animation layer, which keeps its own frame
/// clock (`anim_layer`): drawn there, or right away (and ticking the slice)
/// when something covers them. Outside a layer: [`paint_text_specks`].
pub(super) fn layer_text_specks(
    layer: Option<&super::anim_layer::Layer>,
    rect: Bounds<Pixels>,
    origin: Point<Pixels>,
    color: Hsla,
    window: &mut Window,
) {
    let Some(layer) = layer else {
        paint_text_specks(rect, origin, color, window);
        return;
    };
    let paint = std::rc::Rc::new(move |rect: Bounds<Pixels>, window: &mut Window| {
        draw_text_specks(rect, origin, color, window);
    });
    layer.paint_now(
        super::anim_layer::Content::Paint {
            paint,
            fps: specks_fps(),
        },
        rect,
        window,
    );
}

/// lib_ui redraws spoiler specks every 33 ms.
const SPECKS_FPS: u32 = 30;

/// Battery and animations: "Animated spoiler effect" is off, so the specks
/// hold still and nothing asks for redraws.
pub(super) fn still() -> bool {
    quill::power_saving::on(quill::power_saving::Flag::ChatSpoiler)
}

/// The rate the specks are redrawn at: 0 while they hold still.
pub(super) fn specks_fps() -> u32 {
    if still() { 0 } else { SPECKS_FPS }
}

fn draw_text_specks(rect: Bounds<Pixels>, origin: Point<Pixels>, color: Hsla, window: &mut Window) {
    let now = loop_ms();
    let field_area = Bounds::new(rect.origin - origin, rect.size);
    let speck_reach = px(TEXT.size_max / 2.);
    // A layer's doc asks for no overlap, and specks overlap. They are all
    // quads, and `Scene::finish` sorts quads stably, so paint order holds.
    window.paint_layer(rect.dilate(speck_reach), |window| {
        text_specks(field_area, now, |speck| {
            let bounds = Bounds::new(
                point(
                    origin.x + px(speck.center_x - speck.w / 2.),
                    origin.y + px(speck.center_y - speck.h / 2.),
                ),
                size(px(speck.w), px(speck.h)),
            );
            window.paint_quad(
                fill(bounds, color.opacity(color.a * speck.opacity))
                    .corner_radii(px(speck.w.min(speck.h) / 2.)),
            );
        });
    });
}

struct Speck {
    center_x: f32,
    center_y: f32,
    w: f32,
    h: f32,
    opacity: f32,
}

fn text_specks(field_area: Bounds<Pixels>, now: f32, mut paint: impl FnMut(Speck)) {
    let (left, top) = (field_area.left() / px(1.), field_area.top() / px(1.));
    let (right, bottom) = (field_area.right() / px(1.), field_area.bottom() / px(1.));
    let first_col = (left / CANVAS).floor() as i32;
    let first_row = (top / CANVAS).floor() as i32;
    let last_col = (right / CANVAS).floor() as i32;
    let last_row = (bottom / CANVAS).floor() as i32;
    for particle in live_text_particles(now) {
        let Some((x, y, opacity)) = place(&TEXT, particle, now) else {
            continue;
        };
        for row in first_row..=last_row {
            let y = row as f32 * CANVAS + y;
            if y < top || y > bottom {
                continue;
            }
            for col in first_col..=last_col {
                let x = col as f32 * CANVAS + x;
                if x < left || x > right {
                    continue;
                }
                paint(Speck {
                    center_x: x,
                    center_y: y,
                    w: particle.w,
                    h: particle.h,
                    opacity,
                });
            }
        }
    }
}

fn live_text_particles(now: f32) -> impl Iterator<Item = &'static Particle> {
    let oldest = TEXT.index_starting_at(now - TEXT.lifetime()).floor() as i64;
    let newest = TEXT.index_starting_at(now).ceil() as i64;
    let first = oldest.rem_euclid(TEXT.count as i64) as usize;
    let (wrapped, from_first) = TEXT_FIELD.split_at_checked(first).unwrap_or_default();
    from_first
        .iter()
        .chain(wrapped)
        .take((newest - oldest + 1) as usize)
}

/// A spoiler key: (chat id, message id, run index, caption).
pub(super) type RevealKey = (i64, u64, u64, bool);

/// tdesktop `st::fadeWrapDuration`: a revealed spoiler fades its cover out.
const REVEAL_FADE: f32 = 0.2;

thread_local! {
    static REVEALS: std::cell::RefCell<std::collections::HashMap<RevealKey, Instant>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Start the reveal fade of `key`.
pub(super) fn mark_revealed(key: RevealKey) {
    REVEALS.with(|reveals| {
        let mut reveals = reveals.borrow_mut();
        reveals.retain(|_, at| at.elapsed().as_secs_f32() < REVEAL_FADE);
        reveals.insert(key, Instant::now());
    });
}

/// The cover's remaining opacity while `key` fades out after a reveal.
pub(super) fn reveal_fade(key: RevealKey) -> Option<f32> {
    REVEALS.with(|reveals| {
        let at = *reveals.borrow().get(&key)?;
        let t = at.elapsed().as_secs_f32() / REVEAL_FADE;
        (t < 1.).then_some(1. - t)
    })
}

/// The reveal key of a message's media spoiler (photo, video or GIF).
pub(super) fn media_key(
    chat_id: quill::ids::ChatId,
    message_id: quill::ids::MessageId,
) -> RevealKey {
    (chat_id.0, message_id.0 as u64, u64::MAX, false)
}

/// A media spoiler's cover while it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum MediaCover {
    /// Not revealed yet: a click reveals it.
    Covered,
    /// Just revealed: the cover fades out at this opacity.
    Fading(f32),
}

/// The cover over `content`'s media spoiler; `None` once it is gone or
/// when the media has no spoiler.
pub(super) fn media_cover(
    content: &quill::telegram::envelope::MessageContent,
    key: RevealKey,
    revealed: &std::collections::HashSet<RevealKey>,
) -> Option<MediaCover> {
    if !quill::media_spoiler::has_media_spoiler(content) {
        None
    } else if revealed.contains(&key) {
        reveal_fade(key).map(MediaCover::Fading)
    } else {
        Some(MediaCover::Covered)
    }
}

/// Whether any reveal is still fading (the frame clock keeps going).
pub(super) fn revealing() -> bool {
    REVEALS.with(|reveals| {
        reveals
            .borrow()
            .values()
            .any(|at| at.elapsed().as_secs_f32() < REVEAL_FADE)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        CANVAS, FRAME_MS, FRAMES, IMAGE, IMAGE_FIELD, LOOP_MS, TEXT, TEXT_FIELD, TILE_SCALE,
        live_text_particles, place, speck_coverage, text_specks,
    };
    use gpui_kit::{Bounds, Pixels, point, px, size};
    use std::collections::HashSet;

    #[test]
    fn specks_fade_in_and_out_over_their_life() {
        assert_eq!(IMAGE.alpha(0.), 0.);
        assert!((IMAGE.alpha(150.) - 0.5).abs() < 1e-4);
        assert!((IMAGE.alpha(300.) - 1.).abs() < 1e-4);
        assert!((IMAGE.alpha(450.) - 0.5).abs() < 1e-4);
        assert!((TEXT.alpha(300.) - 1.).abs() < 1e-4);
    }

    #[test]
    fn about_a_third_of_the_field_shows_at_once() {
        // 600 ms of life in a 1980 ms loop.
        let live = IMAGE_FIELD
            .iter()
            .filter(|particle| place(&IMAGE, particle, 500.).is_some())
            .count();
        let expected = IMAGE.count as f32 * IMAGE.lifetime() / LOOP_MS;
        assert!((live as f32 - expected).abs() < expected * 0.05, "{live}");
    }

    #[test]
    fn specks_stay_inside_the_tile_and_drift_slowly() {
        for particle in TEXT_FIELD.iter().take(500) {
            let speed = (particle.dx.powi(2) + particle.dy.powi(2)).sqrt() * 1000.;
            let (min, max) = (TEXT.speed_min / TILE_SCALE, TEXT.speed_max / TILE_SCALE);
            assert!((min - 0.01..=max + 0.01).contains(&speed));
            if let Some((x, y, _)) = place(&TEXT, particle, 1000.) {
                assert!((0. ..CANVAS).contains(&x) && (0. ..CANVAS).contains(&y));
            }
        }
    }

    #[test]
    fn the_live_run_holds_every_speck_that_shows() {
        let frames = (0..FRAMES).map(|frame| frame as f32 * FRAME_MS);
        let others = (0..200).map(|step| step as f32 * 3.17 + 0.05);
        let edges = [0.1, 599.9, 600., 600.1, LOOP_MS - 0.001, LOOP_MS];
        let lifetime_count = TEXT.count as f32 * TEXT.lifetime() / LOOP_MS;
        for now in frames.chain(others).chain(edges) {
            let run: HashSet<u32> = live_text_particles(now)
                .map(|particle| particle.start.to_bits())
                .collect();
            assert!(
                run.len() as f32 <= lifetime_count + 4.,
                "{now}: {}",
                run.len()
            );
            for particle in TEXT_FIELD.iter() {
                if place(&TEXT, particle, now).is_some() {
                    assert!(
                        run.contains(&particle.start.to_bits()),
                        "{now}: {}",
                        particle.start
                    );
                }
            }
        }
    }

    type XyOpacity = [f32; 3];

    fn sorted_bits(specks: Vec<XyOpacity>) -> Vec<[u32; 3]> {
        let mut bits: Vec<_> = specks
            .into_iter()
            .map(|speck| speck.map(f32::to_bits))
            .collect();
        bits.sort_unstable();
        bits
    }

    fn tile_by_tile(area: Bounds<Pixels>, now: f32) -> Vec<XyOpacity> {
        let (left, top) = (area.left() / px(1.), area.top() / px(1.));
        let (right, bottom) = (area.right() / px(1.), area.bottom() / px(1.));
        let mut specks = Vec::new();
        for row in (top / CANVAS).floor() as i32..=(bottom / CANVAS).floor() as i32 {
            for col in (left / CANVAS).floor() as i32..=(right / CANVAS).floor() as i32 {
                for particle in TEXT_FIELD.iter() {
                    let Some((x, y, opacity)) = place(&TEXT, particle, now) else {
                        continue;
                    };
                    let (x, y) = (col as f32 * CANVAS + x, row as f32 * CANVAS + y);
                    if x >= left && x <= right && y >= top && y <= bottom {
                        specks.push([x, y, opacity]);
                    }
                }
            }
        }
        specks
    }

    #[test]
    fn specks_land_where_a_walk_over_every_tile_puts_them() {
        let areas = [
            ("one tile", 10., 10., 100., 20.),
            ("across a seam", 120., 120., 30., 22.),
            ("one wide line", 0., 44., 420., 20.),
            ("a block of lines", 6., 3., 420., 264.),
            ("past the field's corner", -40., -30., 80., 60.),
        ];
        let wrapping_past_the_end = [0., 33., 300., 599.];
        let not_wrapping = [990., LOOP_MS - 1.];
        for now in wrapping_past_the_end.into_iter().chain(not_wrapping) {
            for (label, x, y, width, height) in areas {
                let area = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
                let mut specks = Vec::new();
                text_specks(area, now, |speck| {
                    specks.push([speck.center_x, speck.center_y, speck.opacity]);
                });
                assert!(!specks.is_empty(), "{label} at {now} ms");
                assert_eq!(
                    sorted_bits(specks),
                    sorted_bits(tile_by_tile(area, now)),
                    "{label} at {now} ms"
                );
            }
        }
    }

    #[test]
    fn a_speck_is_solid_at_its_center_and_clear_away_from_it() {
        assert_eq!(speck_coverage(10., 10., 10., 10., 4., 4.), 1.);
        assert_eq!(speck_coverage(14., 10., 10., 10., 4., 4.), 0.);
    }
}
