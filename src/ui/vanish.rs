//! Telegram's delete animation (tdesktop `Ui::ThanosEffect`, from
//! Telegram iOS): a deleted message crumbles into dust from left to right.
//! The dust drifts up and away and fades; the gap it leaves closes with
//! a half-sine ease.
//!
//! tdesktop snapshots the message and turns every pixel into a particle.
//! GPUI can't read back what it drew, so the dust takes its colors from
//! what the message is made of:
//! - the bubble's fill, with a share of grains in its text color;
//! - for a photo, the colors of its preview.
//!
//! The real message stays on screen until the crumbling front passes it,
//! cut away at that front (`CutLeft`), so the text is there until its dust
//! takes over.

use super::app::QuillApp;
use gpui_kit::*;
use quill::ids::{ChatId, MessageId};
use quill::state::HistoryMessage;
use quill::telegram::envelope::MessageContent;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The crumbling front crosses the message in this long (tdesktop's
/// particles start over the first 0.8 of their phase, ~0.5 s).
const WIPE: f32 = 0.45;
/// tdesktop `kBaseDuration` / `kPerPixelDuration` / `kMaxDuration`: the gap
/// closes in 400 ms plus 0.15 ms per pixel of height, at most 600 ms.
const COLLAPSE_BASE: f32 = 0.4;
const COLLAPSE_PER_PX: f32 = 0.000_15;
const COLLAPSE_MAX: f32 = 0.6;
/// The last grain is gone by then.
const DUST_LIFE: f32 = 2.0;
/// About this many grains per message at most (they get coarser beyond).
const MAX_GRAINS: f32 = 7000.;
/// The bubble's corner radius, in points.
const BUBBLE_RADIUS: f32 = 14.;
/// A delete that never lands (failed, offline) stops waiting after this.
const VANISH_WAIT: Duration = Duration::from_secs(10);

/// A row's bounds and its bubble's, in window coordinates.
type Painted = (Bounds<Pixels>, Option<Bounds<Pixels>>);

thread_local! {
    /// Where each history row and its bubble were last painted, by
    /// message id: a delete starts its dust there.
    static PAINTED: RefCell<HashMap<i64, Painted>> = RefCell::new(HashMap::new());
}

fn record(id: i64, update: impl FnOnce(&mut Painted)) {
    PAINTED.with(|painted| {
        let mut painted = painted.borrow_mut();
        if painted.len() > 4096 {
            painted.clear();
        }
        update(painted.entry(id).or_insert((Bounds::default(), None)));
    });
}

/// An invisible child that notes where a history row is painted.
pub(super) fn row_tracker(id: i64) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, _, _| {
            record(id, |entry| entry.0 = bounds);
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// An invisible child that notes where a message's bubble is painted.
pub(super) fn bubble_tracker(id: i64) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, _, _| {
            record(id, |entry| entry.1 = Some(bounds));
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// One grain of dust.
#[derive(Clone, Copy)]
struct Grain {
    /// Where it starts (window points) and its size.
    x: f32,
    y: f32,
    size: f32,
    /// Seconds after the start that the front reaches it.
    release: f32,
    /// Its drift (points per second) and how long it stays opaque.
    vx: f32,
    vy: f32,
    hold: f32,
    color: Hsla,
}

/// Grains rise like smoke: tdesktop adds 80 px/s² against gravity.
const LIFT: f32 = 90.;
/// Seconds over which a released grain picks up speed.
const EASE_IN: f32 = 0.35;
/// How long a grain takes to fade once it lets go.
const FADE: f32 = 0.5;

impl Grain {
    /// Where the grain is and how opaque, `t` seconds into the effect.
    fn at(&self, t: f32) -> Option<(f32, f32, f32)> {
        let age = t - self.release;
        if age < 0. {
            return None;
        }
        // Integral of a smoothstep ramp: it eases into its motion.
        let moving = if age < EASE_IN {
            age.powi(3) / EASE_IN.powi(2) - age.powi(4) / (2. * EASE_IN.powi(3))
        } else {
            EASE_IN / 2. + (age - EASE_IN)
        };
        let alpha = if age < self.hold {
            1.
        } else {
            1. - (age - self.hold) / FADE
        };
        (alpha > 0.).then_some((
            self.x + self.vx * moving,
            self.y + self.vy * moving - 0.5 * LIFT * moving * moving,
            alpha,
        ))
    }
}

/// A small, deterministic generator per message.
struct Random(u64);

impl Random {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// What the dust is colored like.
pub(super) struct Palette {
    fill: Hsla,
    text: Hsla,
    /// A photo's preview: its colors over the top of the bubble.
    picture: Option<image::RgbaImage>,
}

/// Lay grains over `area`: a grid of square cells, each released when
/// the front reaches it.
fn grains(seed: u64, area: Bounds<Pixels>, palette: &Palette) -> Vec<Grain> {
    let (left, top) = (area.left() / px(1.), area.top() / px(1.));
    let (width, height) = (area.size.width / px(1.), area.size.height / px(1.));
    if width <= 0. || height <= 0. {
        return Vec::new();
    }
    // Fine grains (tdesktop goes down to a device pixel), coarser only
    // where a big message would need too many.
    let cell = (width * height / MAX_GRAINS).sqrt().max(1.5);
    // Nothing comes off the bubble's rounded corners.
    let radius = BUBBLE_RADIUS.min(height / 2.).min(width / 2.);
    let outside_corner = |x: f32, y: f32| {
        let dx = (radius - x).max(x - (width - radius)).max(0.);
        let dy = (radius - y).max(y - (height - radius)).max(0.);
        dx * dx + dy * dy > radius * radius
    };
    let (cols, rows) = (
        (width / cell).ceil() as usize,
        (height / cell).ceil() as usize,
    );
    let mut random = Random(seed | 1);
    // A photo fills the bubble's width from the top.
    let picture_height = palette.picture.as_ref().map_or(0., |picture| {
        (width * picture.height() as f32 / picture.width().max(1) as f32).min(height)
    });
    let mut grains = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let (gx, gy) = (col as f32 * cell, row as f32 * cell);
            if outside_corner(gx + cell / 2., gy + cell / 2.) {
                continue;
            }
            let color = match &palette.picture {
                Some(picture) if gy < picture_height => {
                    let px_x = ((gx / width) * picture.width() as f32) as u32;
                    let px_y = ((gy / picture_height) * picture.height() as f32) as u32;
                    let pixel = picture.get_pixel(
                        px_x.min(picture.width() - 1),
                        px_y.min(picture.height() - 1),
                    );
                    Hsla::from(rgba(
                        (u32::from(pixel[0]) << 24)
                            | (u32::from(pixel[1]) << 16)
                            | (u32::from(pixel[2]) << 8)
                            | 0xff,
                    ))
                }
                // Text sits inside the padding; about one grain in six there
                // takes its color, so words leave lighter specks.
                _ if gx > 8.
                    && gx < width - 8.
                    && gy > 6.
                    && gy < height - 6.
                    && random.next() < 0.16 =>
                {
                    palette.text
                }
                _ => {
                    let shade = (random.next() - 0.5) * 0.06;
                    Hsla {
                        l: (palette.fill.l + shade).clamp(0., 1.),
                        ..palette.fill
                    }
                }
            };
            let direction = random.next() * std::f32::consts::TAU;
            // tdesktop: 32–64 px/s (16–32 pt at 2×), sped up 1.65×.
            let speed = 26. + random.next() * 26.;
            grains.push(Grain {
                x: left + gx,
                y: top + gy,
                size: cell,
                release: (gx / width) * WIPE + random.next() * 0.06,
                vx: direction.cos() * speed,
                vy: direction.sin() * speed,
                hold: 0.35 + random.next() * 0.9,
                color,
            });
        }
    }
    grains
}

pub(super) struct Vanishing {
    message: HistoryMessage,
    requested: Instant,
    /// When the message left the history: the animation's clock.
    started: Option<Instant>,
    /// The row's height and the bubble (window points) when deleted.
    row: Option<Bounds<Pixels>>,
    bubble: Option<Bounds<Pixels>>,
    palette: Palette,
    grains: Rc<Vec<Grain>>,
}

impl Vanishing {
    fn elapsed(&self) -> Option<f32> {
        Some(self.started?.elapsed().as_secs_f32())
    }

    fn collapse(&self) -> f32 {
        let height = self.row.map_or(0., |row| row.size.height / px(1.));
        (COLLAPSE_BASE + height * COLLAPSE_PER_PX).min(COLLAPSE_MAX)
    }
}

/// A photo's preview, for coloring its dust.
fn picture_of(message: &HistoryMessage) -> Option<image::RgbaImage> {
    let MessageContent::Photo(photo) = &message.content else {
        return None;
    };
    let mini = photo.minithumbnail.as_ref()?;
    Some(image::load_from_memory(&mini.data).ok()?.to_rgba8())
}

impl QuillApp {
    /// Remember the messages about to be deleted — where they are on
    /// screen and what they look like — so they can crumble once the
    /// deletion lands.
    pub(super) fn begin_vanish(&self, chat_id: ChatId, ids: &[MessageId]) {
        let Some(history) = self.session().and_then(|s| s.histories.get(&chat_id.0)) else {
            return;
        };
        let now = Instant::now();
        let mut vanishing = self.vanishing.borrow_mut();
        for id in ids {
            let Some(message) = history.messages.get(&id.0) else {
                continue;
            };
            let (row, bubble) = PAINTED
                .with(|painted| painted.borrow().get(&id.0).copied())
                .map_or((None, None), |(row, bubble)| {
                    ((row.size.height > px(0.)).then_some(row), bubble)
                });
            let palette = Palette {
                fill: Hsla::from(if message.is_outgoing {
                    super::chat_theme::accent_strong()
                } else {
                    super::chat_theme::bg_bubble_incoming()
                }),
                text: Hsla::from(if message.is_outgoing {
                    super::chat_theme::text_on_fill()
                } else {
                    super::chat_theme::text_bright()
                }),
                picture: picture_of(message),
            };
            vanishing.push(Vanishing {
                message: message.clone(),
                requested: now,
                started: None,
                row,
                bubble,
                palette,
                grains: Rc::new(Vec::new()),
            });
        }
    }

    /// Put the crumbling messages back among `messages` (oldest first)
    /// while their gap closes; drop the finished ones.
    pub(super) fn merge_vanishing(
        &self,
        messages: &mut Vec<HistoryMessage>,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let mut vanishing = self.vanishing.borrow_mut();
        vanishing.retain(|ghost| match ghost.started {
            Some(started) => now.duration_since(started).as_secs_f32() < DUST_LIFE,
            None => now.duration_since(ghost.requested) < VANISH_WAIT,
        });
        let chat = messages.first().map(|m| m.chat_id);
        let mut added = false;
        for ghost in vanishing.iter_mut() {
            if Some(ghost.message.chat_id) != chat
                || messages.iter().any(|m| m.id == ghost.message.id)
            {
                continue;
            }
            if ghost.started.is_none() {
                ghost.started = Some(now);
                let area = ghost.bubble.or(ghost.row);
                ghost.grains = Rc::new(area.map_or_else(Vec::new, |area| {
                    grains(ghost.message.id.0 as u64, area, &ghost.palette)
                }));
            }
            // The row stays (cut away, then closing) until its gap is gone.
            let t = ghost.elapsed().unwrap_or(0.);
            if ghost.row.is_some() && t < WIPE + ghost.collapse() {
                messages.push(ghost.message.clone());
                added = true;
            }
        }
        if added {
            messages.sort_by_key(|m| m.id.0);
        }
        if !vanishing.is_empty() {
            self.request_animation_tick(60, cx);
        }
    }

    /// Seconds since a crumbling message left the history.
    pub(super) fn vanish_progress(&self, id: MessageId) -> Option<f32> {
        let vanishing = self.vanishing.borrow();
        vanishing
            .iter()
            .find(|ghost| ghost.message.id == id)?
            .elapsed()
    }

    /// For the history rows key: a fresh value every frame while a
    /// message crumbles.
    pub(super) fn vanish_rows_hash(&self) -> Option<u128> {
        let vanishing = self.vanishing.borrow();
        (!vanishing.is_empty()).then(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        })
    }

    /// A crumbling message's row, `t` seconds in: the message cut away
    /// behind the front, then an empty gap closing with a half-sine ease.
    pub(super) fn ghost_row(&self, id: MessageId, t: f32, row: AnyElement) -> AnyElement {
        let vanishing = self.vanishing.borrow();
        let Some(ghost) = vanishing.iter().find(|ghost| ghost.message.id == id) else {
            return row;
        };
        let Some(bounds) = ghost.row else {
            return div().into_any_element();
        };
        if t < WIPE {
            let area = ghost.bubble.unwrap_or(bounds);
            let front = area.left() + area.size.width * (t / WIPE).clamp(0., 1.);
            return CutLeft { child: row, front }.into_any_element();
        }
        let closing = ((t - WIPE) / ghost.collapse()).clamp(0., 1.);
        // `anim::halfSine`.
        let eased = (closing * std::f32::consts::FRAC_PI_2).sin();
        div()
            .h(bounds.size.height * (1. - eased))
            .into_any_element()
    }

    /// The dust of every crumbling message, over the history.
    pub(super) fn vanish_overlay(&self) -> Option<AnyElement> {
        let vanishing = self.vanishing.borrow();
        let clouds: Vec<(f32, Rc<Vec<Grain>>)> = vanishing
            .iter()
            .filter_map(|ghost| Some((ghost.elapsed()?, ghost.grains.clone())))
            .filter(|(_, grains)| !grains.is_empty())
            .collect();
        if clouds.is_empty() {
            return None;
        }
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    for (t, grains) in &clouds {
                        for grain in grains.iter() {
                            let Some((x, y, alpha)) = grain.at(*t) else {
                                continue;
                            };
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(px(x), px(y)),
                                    size(px(grain.size), px(grain.size)),
                                ),
                                grain.color.opacity(grain.color.a * alpha),
                            ));
                        }
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full()
            .into_any_element(),
        )
    }
}

/// Paints its child only to the right of `front` (window x): the part
/// the crumbling front has passed is gone.
struct CutLeft {
    child: AnyElement,
    front: Pixels,
}

impl IntoElement for CutLeft {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for CutLeft {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let child = self.child.request_layout(window, cx);
        let style = Style {
            size: size(relative(1.).into(), auto()),
            ..Style::default()
        };
        (window.request_layout(style, [child], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let right = bounds.right().max(self.front);
        let kept = Bounds::from_corners(
            point(self.front, bounds.top() - px(1000.)),
            point(right + px(1000.), bounds.bottom() + px(1000.)),
        );
        window.with_content_mask(Some(ContentMask { bounds: kept }), |window| {
            self.child.paint(window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{FADE, Grain, MAX_GRAINS, Palette, WIPE, grains};
    use gpui_kit::{Bounds, hsla, point, px, size, white};

    fn palette() -> Palette {
        Palette {
            fill: hsla(0.6, 0.8, 0.5, 1.),
            text: white(),
            picture: None,
        }
    }

    #[test]
    fn grains_cover_the_bubble_and_leave_left_to_right() {
        let area = Bounds::new(point(px(100.), px(50.)), size(px(200.), px(40.)));
        let grains = grains(7, area, &palette());
        // A 1.5 pt grid, less the rounded corners.
        let full = (200f32 / 1.5).ceil() * (40f32 / 1.5).ceil();
        assert!(grains.len() < full as usize && grains.len() as f32 > full * 0.95);
        assert!(
            !grains.iter().any(|g| g.x < 101. && g.y < 51.),
            "corner grains"
        );
        let first = grains.iter().find(|g| g.x < 102. && g.y > 70.).unwrap();
        let last = grains.iter().find(|g| g.x > 297.).unwrap();
        assert!(first.release < 0.07 && last.release > WIPE * 0.9);
        assert!(grains.iter().any(|g| g.color == white()));
    }

    #[test]
    fn big_bubbles_get_coarser_grains_not_more() {
        let area = Bounds::new(point(px(0.), px(0.)), size(px(420.), px(600.)));
        assert!(grains(1, area, &palette()).len() as f32 <= MAX_GRAINS * 1.05);
    }

    #[test]
    fn a_grain_waits_then_eases_off_rising_and_fades() {
        let grain = Grain {
            x: 10.,
            y: 10.,
            size: 2.,
            release: 0.2,
            vx: 0.,
            vy: 0.,
            hold: 0.5,
            color: white(),
        };
        assert!(grain.at(0.1).is_none());
        let (_, y0, a0) = grain.at(0.2).unwrap();
        assert_eq!((y0, a0), (10., 1.));
        let (_, y1, _) = grain.at(0.4).unwrap();
        let (_, y2, _) = grain.at(0.7).unwrap();
        assert!(y1 < 10. && y2 < y1, "rises: {y1} {y2}");
        let (_, _, fading) = grain.at(0.2 + 0.5 + FADE / 2.).unwrap();
        assert!((fading - 0.5).abs() < 1e-3);
        assert!(grain.at(0.2 + 0.5 + FADE + 0.01).is_none());
    }
}
