//! The glow around the window's edge while an agent drives, in the theme's
//! accent: a thin accent border with a soft bloom inside it, and a light
//! that sweeps round the edge on top of it while the agent drives.
//!
//! The sweep runs only while the agent drives: paused it holds still where
//! it was, and it fades out with the border on Stop or a disconnect.
//! [`sweep_allowed`] is the one place that decides whether it may move at
//! all ("Takeover effect", reduced motion, Power Saving, the system's
//! low-power mode); otherwise the border holds still. Paint only: nothing
//! here has a hitbox, so input goes to whatever is underneath.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::power_saving::{self, Flag};

/// One lap of the sweep.
const SWEEP_MS: f32 = 5000.0;
/// The sweep's light, along the edge.
const SWEEP_LENGTH: f32 = 340.0;
/// How thick the light's core is, over the 2 px border.
const SWEEP_THICKNESS: f32 = 5.0;
/// Frames per second while the light sweeps. The window redraws whole on
/// each frame, so this is the sweep's cost; a lap of 5 s moves the light
/// about 48 px a frame on a 1100x720 window, a seventh of its length, so
/// its soft ends still read as gliding.
pub(super) const SWEEP_FPS: u32 = 15;
/// Frames per second for the fades, glides and the press pulse.
pub(super) const GLOW_FPS: u32 = 30;

/// Whether the light may sweep right now: the "Takeover effect" preference
/// against reduced motion (the system's, or `QUILL_REDUCED_MOTION`) and
/// power saving: Quill's Power Saving for interface animations (which the
/// system's battery saver or heat force on unless "Save Power on Low
/// Battery" is off) or the system's low-power mode itself (macOS Low Power
/// Mode, the Windows battery saver, Linux's power-saver profile; read by
/// `battery_saving`, unknown counts as not saving). Read on every render,
/// so a change shows on the next frame.
pub(super) fn sweep_allowed() -> bool {
    let saving = power_saving::on(Flag::Animations) || quill::battery_saving::state() == Some(true);
    quill::agent::access::takeover_effect().sweeps(power_saving::os_reduce_motion(), saving)
}

/// The glow at one moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GlowFrame {
    /// Overall strength, 0 to 1 (the takeover's fade).
    pub(super) strength: f32,
    /// Where the sweep's light is, as a fraction of the way round; `None`
    /// for the still border.
    pub(super) sweep: Option<f32>,
}

/// The glow with the takeover `presence` (its fade, 0 to 1) and the sweep
/// `sweep_ms` into its run, or the still border for `None`.
pub(super) fn glow_frame(presence: f32, sweep_ms: Option<u64>) -> GlowFrame {
    GlowFrame {
        strength: presence,
        sweep: sweep_ms.map(|t| (t as f32 / SWEEP_MS).fract()),
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

/// The light's core. On a dark theme the accent lifted towards white reads
/// as light; on a light theme a lighter core would read as a gap in the
/// border, so there the core is a deeper, more saturated accent and the
/// light is its halo spreading into the window.
fn highlight(accent: Hsla, dark: bool) -> Hsla {
    if dark {
        Hsla {
            l: accent.l + (1.0 - accent.l) * 0.55,
            ..accent
        }
    } else {
        Hsla {
            s: (accent.s * 1.15).min(1.0),
            l: accent.l * 0.82,
            ..accent
        }
    }
}

/// How strong the accent border is: dimmer while the light sweeps, so the
/// light stands out against it, full when it holds still.
fn border_alpha(sweeps: bool) -> f32 {
    if sweeps { 0.55 } else { 0.85 }
}

/// The glow as elements filling a `width` x `height` window. `corner`
/// follows the window's own rounded corners; `dark` is the theme's.
pub(super) fn glow_element(
    frame: GlowFrame,
    accent: Hsla,
    dark: bool,
    window_size: Size<Pixels>,
    corner: Pixels,
) -> AnyElement {
    let strength = frame.strength.clamp(0.0, 1.0);
    let bloom = BoxShadow {
        color: accent.opacity(0.32 * strength),
        offset: point(px(0.), px(0.)),
        blur_radius: px(18.),
        spread_radius: px(0.),
        inset: true,
    };
    let layer = div()
        .absolute()
        .top_0()
        .left_0()
        .w(window_size.width)
        .h(window_size.height)
        .rounded(corner)
        .shadow(vec![bloom])
        .border_2()
        .border_color(accent.opacity(border_alpha(frame.sweep.is_some()) * strength));
    let width = window_size.width.as_f32();
    let height = window_size.height.as_f32();
    let pieces = frame
        .sweep
        .map(|at| sweep_pieces(at, width, height, SWEEP_THICKNESS))
        .unwrap_or_default();
    let core = highlight(accent, dark);
    let lights = pieces.into_iter().map(|piece| {
        let glow = (piece.start_alpha + piece.end_alpha) / 2.0 * strength;
        div()
            .absolute()
            .left(piece.bounds.origin.x)
            .top(piece.bounds.origin.y)
            .w(piece.bounds.size.width)
            .h(piece.bounds.size.height)
            .bg(linear_gradient(
                piece.angle,
                linear_color_stop(core.opacity(piece.start_alpha * strength), 0.0),
                linear_color_stop(core.opacity(piece.end_alpha * strength), 1.0),
            ))
            // Half of each shadow falls outside the window, so the light
            // is a tight bright halo plus a wide soft bloom inwards.
            .shadow(vec![
                BoxShadow {
                    color: accent.opacity(glow),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(12.),
                    spread_radius: px(2.),
                    inset: false,
                },
                BoxShadow {
                    color: accent.opacity(0.8 * glow),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(48.),
                    spread_radius: px(10.),
                    inset: false,
                },
            ])
    });
    layer.children(lights).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{SWEEP_MS, border_alpha, glow_frame, highlight, sweep_pieces};
    use gpui_kit::hsla;

    #[test]
    fn the_still_border_has_no_light_and_the_sweep_laps() {
        assert_eq!(glow_frame(1.0, None).sweep, None);
        assert_eq!(glow_frame(0.4, None).strength, 0.4);
        let lap = SWEEP_MS as u64;
        assert_eq!(glow_frame(1.0, Some(lap / 4)).sweep, Some(0.25));
        assert_eq!(glow_frame(1.0, Some(lap + lap / 4)).sweep, Some(0.25));
    }

    #[test]
    fn the_light_stands_out_from_the_border_in_both_themes() {
        for accent in [hsla(0.6, 0.8, 0.5, 1.0), hsla(0.6, 0.7, 0.7, 1.0)] {
            let dark = highlight(accent, true);
            assert!(dark.l > accent.l + 0.1 && dark.l < 1.0, "{accent:?}");
            let light = highlight(accent, false);
            assert!(
                light.l < accent.l - 0.05 && light.s >= accent.s,
                "{accent:?}"
            );
        }
        assert!(border_alpha(true) < border_alpha(false));
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
