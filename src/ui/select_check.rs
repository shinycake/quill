//! The round selection check: one mark for every "pick some of these"
//! surface (history rows, album photos, album files, story tiles).
//!
//! Empty it is a ring; picked it is a disc in the chat's accent with a
//! tick. What it sits on ([`Ground`]) decides how it keeps its contrast: a
//! photo or wallpaper gets a white ring over a soft halo, a bubble gets the
//! theme's own tones.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

/// Diameter, ring included.
pub(crate) const SIZE: f32 = quill::chat_wide::MSG_SELECTION_CHECK_SIZE;
const RING: f32 = 1.5;
const TICK: f32 = 2.;
/// The accent ring inside a picked photo's edge.
const TILE_RING: f32 = 3.;
/// Share of the run in which the disc closes in; the tick follows.
const DISC_SHARE: f32 = 0.6;
const TICK_START: f32 = 0.3;
/// How far the whole check swells mid-run.
const SWELL: f32 = 0.08;

/// What the check is drawn over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ground {
    /// Pixels nobody chose: a wallpaper, a photo, a video.
    Scene,
    /// A neutral theme surface: an incoming bubble, a list row.
    Surface,
    /// A surface in the accent colour: an outgoing bubble.
    Accent,
}

/// The colours of one check.
#[derive(Clone, Copy)]
struct Look {
    /// The ring, empty and picked.
    ring: (Hsla, Hsla),
    /// Inside the empty ring.
    well: Hsla,
    disc: Hsla,
    tick: Hsla,
    halo: Option<Hsla>,
    /// Whether the disc stays inside the ring or covers it.
    ring_stays: bool,
}

impl Look {
    fn of(ground: Ground, cx: &App) -> Self {
        let theme = cx.theme();
        let accent = crate::ui::chat_palette::themed(theme.primary);
        match ground {
            Ground::Scene => Self {
                ring: (white(), white()),
                // The halo's own body is the well.
                well: transparent_black(),
                disc: accent,
                tick: white(),
                halo: Some(black().opacity(0.25)),
                ring_stays: true,
            },
            Ground::Surface => Self {
                ring: (theme.muted_foreground.opacity(0.85), accent),
                well: transparent_black(),
                disc: accent,
                tick: theme.primary_foreground,
                halo: None,
                ring_stays: false,
            },
            Ground::Accent => {
                let on_fill = theme.primary_foreground;
                Self {
                    ring: (on_fill.opacity(0.8), on_fill),
                    well: transparent_black(),
                    disc: on_fill,
                    tick: accent,
                    halo: None,
                    ring_stays: false,
                }
            }
        }
    }
}

/// How the check paints at picked-progress `p` (`0..=1`, linear in time).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// Share of the disc that has closed in from the centre.
    pub(crate) disc: f32,
    /// Share of the tick drawn, from its short stroke on.
    pub(crate) tick: f32,
    /// Scale of the whole check.
    pub(crate) scale: f32,
}

impl Frame {
    pub(crate) fn at(p: f32) -> Self {
        let p = p.clamp(0., 1.);
        let ease_out = |t: f32| 1. - (1. - t.clamp(0., 1.)).powi(3);
        Self {
            disc: ease_out(p / DISC_SHARE),
            tick: ease_out((p - TICK_START) / (1. - TICK_START)),
            scale: 1. + SWELL * (std::f32::consts::PI * p).sin(),
        }
    }
}

/// The check at picked-progress `checked` (`0.` empty, `1.` picked).
pub(crate) fn select_check(checked: f32, ground: Ground, cx: &App) -> impl IntoElement + use<> {
    let look = Look::of(ground, cx);
    let frame = Frame::at(checked);
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| paint(bounds, frame, look, window),
    )
    .flex_none()
    .size(px(SIZE))
}

/// One item's selection mark, as this frame shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Mark {
    /// How far selection mode has faded in (`0..=1`): the check's opacity.
    pub(crate) mode: f32,
    /// Picked-progress (`0..=1`).
    pub(crate) picked: f32,
}

impl Mark {
    /// A mark where nothing animates: selection mode shows, and the item
    /// is picked or not.
    pub(crate) fn settled(picked: bool) -> Self {
        Self {
            mode: 1.,
            picked: if picked { 1. } else { 0. },
        }
    }
}

/// An item's own check for a corner of it, while selection mode shows.
pub(crate) fn corner_check(mark: Mark, ground: Ground, cx: &App) -> Option<Div> {
    (mark.mode > 0.).then(|| {
        div()
            .opacity(mark.mode)
            .child(select_check(mark.picked, ground, cx))
    })
}

/// The check in the corner of a photo or video tile.
pub(crate) fn tile_check(mark: Mark, cx: &App) -> Option<Div> {
    corner_check(mark, Ground::Scene, cx)
        .map(|check| div().absolute().top(px(6.)).right(px(6.)).child(check))
}

/// What marks a picked photo or video tile besides its check: a ring in
/// the accent inside its edge, with a white hairline inside that so the
/// ring shows on a picture of the accent's own colour. The picture itself
/// stays as it is. `corners` are the tile's.
pub(crate) fn picked_tile(mark: Mark, corners: Corners<Pixels>, cx: &App) -> Option<Div> {
    let accent = crate::ui::chat_palette::themed(cx.theme().primary);
    let inner = corners.map(|radius| (*radius - px(TILE_RING)).max(px(0.)));
    (mark.picked > 0.).then(|| {
        div()
            .absolute()
            .inset_0()
            .map(|this| rounded(this, corners))
            .border(px(TILE_RING))
            .border_color(accent.opacity(mark.picked))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .map(|this| rounded(this, inner))
                    .border_1()
                    .border_color(white().opacity(0.85 * mark.picked)),
            )
    })
}

fn rounded(element: Div, corners: Corners<Pixels>) -> Div {
    element
        .rounded_tl(corners.top_left)
        .rounded_tr(corners.top_right)
        .rounded_br(corners.bottom_right)
        .rounded_bl(corners.bottom_left)
}

/// What marks a picked file of a file album: a wash behind its row.
pub(crate) fn picked_file(mark: Mark, ground: Ground, cx: &App) -> Option<Div> {
    let theme = cx.theme();
    let wash = match ground {
        Ground::Accent => theme.primary_foreground.opacity(0.16),
        Ground::Scene | Ground::Surface => {
            crate::ui::chat_palette::themed(theme.primary).opacity(0.16)
        }
    };
    (mark.picked > 0.).then(|| {
        div()
            .absolute()
            .inset(px(-4.))
            .rounded_md()
            .bg(wash.opacity(wash.a * mark.picked))
    })
}

fn paint(bounds: Bounds<Pixels>, frame: Frame, look: Look, window: &mut Window) {
    let center = bounds.center();
    let radius = SIZE / 2. * frame.scale;
    let disc_at = |radius: f32| {
        Bounds::new(
            center - point(px(radius), px(radius)),
            size(px(radius * 2.), px(radius * 2.)),
        )
    };
    let whole = disc_at(radius);
    if let Some(halo) = look.halo {
        window.paint_drop_shadows(
            whole,
            Corners::all(px(radius)),
            &[BoxShadow {
                color: halo,
                offset: point(px(0.), px(0.5)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }],
        );
    }
    let picked = frame.disc >= 1.;
    window.paint_quad(quad(
        whole,
        px(radius),
        look.well,
        px(RING),
        if picked { look.ring.1 } else { look.ring.0 },
        BorderStyle::Solid,
    ));
    if frame.disc > 0. {
        let inset = if look.ring_stays { RING } else { 0. };
        let disc = (radius - inset) * frame.disc;
        window.paint_quad(fill(disc_at(disc), look.disc).corner_radii(px(disc)));
    }
    if frame.tick > 0. {
        paint_tick(center, frame, look.tick, window);
    }
}

/// The tick: a short stroke down, a long one up, drawn in that order.
fn paint_tick(center: Point<Pixels>, frame: Frame, color: Hsla, window: &mut Window) {
    // On an 18-unit disc, from its centre.
    const START: (f32, f32) = (-3.7, 0.5);
    const ELBOW: (f32, f32) = (-1.2, 3.4);
    const END: (f32, f32) = (3.6, -2.8);
    let unit = SIZE / 18. * frame.scale;
    let at = |(x, y): (f32, f32)| center + point(px(x * unit), px(y * unit));
    let lerp =
        |a: (f32, f32), b: (f32, f32), t: f32| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let length = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0).hypot(b.1 - a.1);
    let (short, long) = (length(START, ELBOW), length(ELBOW, END));
    let drawn = frame.tick * (short + long);
    let mut path =
        PathBuilder::stroke(px(TICK * frame.scale)).with_style(PathStyle::Stroke(StrokeOptions {
            line_width: TICK * frame.scale,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        }));
    path.move_to(at(START));
    if drawn <= short {
        path.line_to(at(lerp(START, ELBOW, drawn / short)));
    } else {
        path.line_to(at(ELBOW));
        path.line_to(at(lerp(ELBOW, END, (drawn - short) / long)));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

#[cfg(test)]
mod tests {
    use super::{DISC_SHARE, Frame, TICK_START};

    #[test]
    fn empty_and_picked_are_at_rest() {
        assert_eq!(
            Frame::at(0.),
            Frame {
                disc: 0.,
                tick: 0.,
                scale: 1.
            }
        );
        let picked = Frame::at(1.);
        assert_eq!((picked.disc, picked.tick), (1., 1.));
        assert!((picked.scale - 1.).abs() < 1e-5);
    }

    #[test]
    fn the_disc_closes_before_the_tick_ends() {
        let mid = Frame::at(DISC_SHARE);
        assert_eq!(mid.disc, 1.);
        assert!(mid.tick > 0. && mid.tick < 1.);
        assert!(mid.scale > 1.);
        assert_eq!(Frame::at(TICK_START).tick, 0.);
    }
}
