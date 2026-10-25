//! The glow around the window's edge while an agent drives, in the theme's
//! accent. Three looks, picked with `QUILL_AGENT_GLOW` for comparison:
//!
//! - `edge` (the default): a thin accent border with a soft bloom inside
//!   it. It only fades in and out; the steady state does not move.
//! - `breathe`: the bloom slowly brightens and dims.
//! - `sweep`: a faint bloom with a short light travelling around the edge.
//!
//! With reduced motion every look is the still `edge`. Paint only: nothing
//! here has a hitbox, so input goes to whatever is underneath.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GlowStyle {
    Edge,
    Breathe,
    Sweep,
}

/// One breath, and one lap of the sweep.
const BREATH_MS: f32 = 2800.0;
const SWEEP_MS: f32 = 3200.0;
/// The sweep's light, along the edge.
const SWEEP_LENGTH: f32 = 260.0;
/// Frames per second while a look moves by itself.
pub(super) const GLOW_FPS: u32 = 30;

impl GlowStyle {
    pub(super) fn parse(name: &str) -> Option<Self> {
        match name.trim() {
            "edge" => Some(Self::Edge),
            "breathe" => Some(Self::Breathe),
            "sweep" => Some(Self::Sweep),
            _ => None,
        }
    }

    /// `QUILL_AGENT_GLOW=edge|breathe|sweep`, else `edge`.
    pub(super) fn chosen() -> Self {
        static CHOSEN: std::sync::OnceLock<GlowStyle> = std::sync::OnceLock::new();
        *CHOSEN.get_or_init(|| {
            std::env::var("QUILL_AGENT_GLOW")
                .ok()
                .and_then(|name| Self::parse(&name))
                .unwrap_or(Self::Edge)
        })
    }

    /// The look actually drawn: reduced motion keeps it still.
    pub(super) fn shown(self, reduce_motion: bool) -> Self {
        if reduce_motion { Self::Edge } else { self }
    }

    /// Whether the steady look moves (and so asks for frames).
    pub(super) fn moves(self) -> bool {
        !matches!(self, Self::Edge)
    }
}

/// The glow at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GlowFrame {
    /// Overall strength, 0 to 1 (the fade times the look's own level).
    pub(super) strength: f32,
    /// The solid border (`edge`).
    pub(super) border: bool,
    /// Where the sweep's light is, as a fraction of the way round.
    pub(super) sweep: Option<f32>,
}

/// The glow for `style` at `now`, `elapsed` ms into the session, with the
/// takeover `presence` (its fade, 0 to 1).
pub(super) fn glow_frame(style: GlowStyle, presence: f32, elapsed: u64) -> GlowFrame {
    let t = elapsed as f32;
    match style {
        GlowStyle::Edge => GlowFrame {
            strength: presence,
            border: true,
            sweep: None,
        },
        GlowStyle::Breathe => {
            let phase = (t / BREATH_MS).fract() * std::f32::consts::TAU;
            let level = 0.55 + 0.45 * (0.5 - 0.5 * phase.cos());
            GlowFrame {
                strength: presence * level,
                border: false,
                sweep: None,
            }
        }
        GlowStyle::Sweep => GlowFrame {
            strength: presence * 0.6,
            border: false,
            sweep: Some((t / SWEEP_MS).fract()),
        },
    }
}

/// One straight piece of the sweep's light: where it sits and how bright
/// it is at its two ends, along the direction of travel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SweepPiece {
    pub(super) bounds: Bounds<Pixels>,
    /// The gradient's direction (degrees, 0 = up, clockwise).
    pub(super) angle: f32,
    pub(super) start_alpha: f32,
    pub(super) end_alpha: f32,
}

/// The light at `at` (fraction of the way round, clockwise from the top
/// left corner) on a `width` x `height` window, as straight pieces of
/// `thickness`. It brightens linearly to its middle and dims after, so a
/// two-stop gradient per piece is exact once a piece is cut at the middle.
pub(super) fn sweep_pieces(at: f32, width: f32, height: f32, thickness: f32) -> Vec<SweepPiece> {
    let perimeter = 2.0 * (width + height);
    if perimeter <= 0.0 || !perimeter.is_finite() {
        return Vec::new();
    }
    let length = SWEEP_LENGTH.min(perimeter / 4.0);
    let half = length / 2.0;
    let centre = at.rem_euclid(1.0) * perimeter;
    let alpha = |x: f32| (1.0 - (x - centre).abs() / half).clamp(0.0, 1.0);
    // The edges in travel order: start along the perimeter, length, angle.
    let edges = [
        (0.0, width, 90.0),
        (width, height, 180.0),
        (width + height, width, 270.0),
        (2.0 * width + height, height, 0.0),
    ];
    let mut pieces = Vec::new();
    // The light can straddle the start: look one lap either side too.
    for lap in [-perimeter, 0.0, perimeter] {
        for (start, len, angle) in edges {
            let (lo, hi) = (start + lap, start + lap + len);
            let a = (centre - half).max(lo);
            let b = (centre + half).min(hi);
            if b - a <= 0.5 {
                continue;
            }
            let cuts: Vec<(f32, f32)> = if a < centre && centre < b {
                vec![(a, centre), (centre, b)]
            } else {
                vec![(a, b)]
            };
            for (from, to) in cuts {
                let (off, run) = (from - lo, to - from);
                let bounds = match angle as i32 {
                    90 => Bounds::new(point(px(off), px(0.)), size(px(run), px(thickness))),
                    180 => Bounds::new(
                        point(px(width - thickness), px(off)),
                        size(px(thickness), px(run)),
                    ),
                    270 => Bounds::new(
                        point(px(width - off - run), px(height - thickness)),
                        size(px(run), px(thickness)),
                    ),
                    _ => Bounds::new(
                        point(px(0.), px(height - off - run)),
                        size(px(thickness), px(run)),
                    ),
                };
                pieces.push(SweepPiece {
                    bounds,
                    angle,
                    start_alpha: alpha(from),
                    end_alpha: alpha(to),
                });
            }
        }
    }
    pieces
}

/// The glow as elements filling a `width` x `height` window. `corner`
/// follows the window's own rounded corners.
pub(super) fn glow_element(
    frame: GlowFrame,
    accent: Hsla,
    window_size: Size<Pixels>,
    corner: Pixels,
) -> AnyElement {
    let strength = frame.strength.clamp(0.0, 1.0);
    let (bloom_alpha, blur, spread) = if frame.border {
        (0.32, 18.0, 0.0)
    } else {
        (0.55, 28.0, 2.0)
    };
    let bloom = BoxShadow {
        color: accent.opacity(bloom_alpha * strength),
        offset: point(px(0.), px(0.)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: true,
    };
    let mut layer = div()
        .absolute()
        .top_0()
        .left_0()
        .w(window_size.width)
        .h(window_size.height)
        .rounded(corner)
        .shadow(vec![bloom]);
    if frame.border {
        layer = layer
            .border_2()
            .border_color(accent.opacity(0.85 * strength));
    }
    let width = window_size.width.as_f32();
    let height = window_size.height.as_f32();
    let pieces = frame
        .sweep
        .map(|at| sweep_pieces(at, width, height, 3.0))
        .unwrap_or_default();
    let light = |alpha: f32| accent.opacity(alpha * strength / 0.6);
    let lights = pieces.into_iter().map(|piece| {
        let glow = (piece.start_alpha + piece.end_alpha) / 2.0;
        div()
            .absolute()
            .left(piece.bounds.origin.x)
            .top(piece.bounds.origin.y)
            .w(piece.bounds.size.width)
            .h(piece.bounds.size.height)
            .bg(linear_gradient(
                piece.angle,
                linear_color_stop(light(piece.start_alpha), 0.0),
                linear_color_stop(light(piece.end_alpha), 1.0),
            ))
            .shadow(vec![BoxShadow {
                color: accent.opacity(0.7 * glow * strength / 0.6),
                offset: point(px(0.), px(0.)),
                blur_radius: px(14.),
                spread_radius: px(1.),
                inset: false,
            }])
    });
    layer.children(lights).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{GlowStyle, glow_frame, sweep_pieces};

    #[test]
    fn styles_parse_and_reduced_motion_keeps_the_edge_still() {
        assert_eq!(GlowStyle::parse("sweep"), Some(GlowStyle::Sweep));
        assert_eq!(GlowStyle::parse("glitter"), None);
        assert_eq!(GlowStyle::Breathe.shown(true), GlowStyle::Edge);
        assert_eq!(GlowStyle::Sweep.shown(false), GlowStyle::Sweep);
        assert!(!GlowStyle::Edge.moves());
        assert!(GlowStyle::Breathe.moves());
    }

    #[test]
    fn the_edge_holds_still_and_the_breath_stays_soft() {
        assert_eq!(
            glow_frame(GlowStyle::Edge, 1.0, 0),
            glow_frame(GlowStyle::Edge, 1.0, 1234)
        );
        let levels: Vec<f32> = (0..28)
            .map(|step| glow_frame(GlowStyle::Breathe, 1.0, step * 100).strength)
            .collect();
        let low = levels.iter().copied().fold(f32::MAX, f32::min);
        let high = levels.iter().copied().fold(f32::MIN, f32::max);
        assert!(
            low >= 0.55 && high <= 1.0 && high - low > 0.3,
            "{low} {high}"
        );
        assert_eq!(glow_frame(GlowStyle::Breathe, 0.0, 700).strength, 0.0);
    }

    #[test]
    fn the_sweep_light_wraps_round_the_corners() {
        // In the middle of the top edge: one rising and one falling piece.
        let top = sweep_pieces(0.125, 800.0, 400.0, 3.0);
        assert_eq!(top.len(), 2);
        assert!(top.iter().all(|p| p.angle == 90.0));
        let peak = top.first().map(|p| p.end_alpha);
        assert_eq!(peak, Some(1.0));
        // Across the top right corner: pieces on the top and the right edge.
        let corner = sweep_pieces(800.0 / 2400.0, 800.0, 400.0, 3.0);
        assert!(corner.iter().any(|p| p.angle == 90.0));
        assert!(corner.iter().any(|p| p.angle == 180.0));
        // Across the start (top left corner): the left edge and the top.
        let start = sweep_pieces(0.0, 800.0, 400.0, 3.0);
        assert!(start.iter().any(|p| p.angle == 0.0));
        assert!(start.iter().any(|p| p.angle == 90.0));
        for piece in corner.iter().chain(&start) {
            let b = piece.bounds;
            assert!(b.origin.x.as_f32() >= -0.01 && b.origin.y.as_f32() >= -0.01);
            assert!((b.origin.x + b.size.width).as_f32() <= 800.01);
            assert!((b.origin.y + b.size.height).as_f32() <= 400.01);
        }
        assert!(sweep_pieces(0.3, 0.0, 0.0, 3.0).is_empty());
    }
}
