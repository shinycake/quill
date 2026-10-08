//! The small animated glyph before a peer-activity line ("typing", "Dana is
//! recording a voice message") in the chat row and the conversation header.
//!
//! Only built while an action is active, so an idle chat list animates
//! nothing.

use gpui_kit::*;
use quill::state::ActivityIndicator;
use std::sync::OnceLock;
use std::time::Instant;

const CYCLE_MS: u128 = 1100;

const DOT: f32 = 3.;

/// Opacity of a dot whose pulse is offset by `phase` (0..1) of the cycle.
fn pulse(delta: f32, phase: f32) -> f32 {
    let t = (delta + phase).fract();
    0.3 + 0.7 * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos())
}

/// Position in the pulse cycle (0..1) for `elapsed_ms` on the shared clock.
fn cycle_delta(elapsed_ms: u128) -> f32 {
    (elapsed_ms % CYCLE_MS) as f32 / CYCLE_MS as f32
}

/// Inside a slice with an animation layer (`anim_layer`) the layer draws
/// the dots and keeps them pulsing; elsewhere callers must also
/// `request_animation_tick` so the frame clock redraws (GPUI's
/// `with_animation` would redraw the window at display rate).
///
/// `key` must be unique per visible indicator (animation state is keyed by
/// element id).
pub(super) fn activity_indicator(
    indicator: ActivityIndicator,
    color: Hsla,
    key: SharedString,
) -> AnyElement {
    let dots = match indicator {
        ActivityIndicator::Dots => 3,
        ActivityIndicator::Pulse => 1,
    };
    if super::anim_layer::current().is_some() {
        let width = dots as f32 * DOT + (dots - 1) as f32 * GAP;
        return div()
            .flex_none()
            .mr_1()
            .child(
                super::anim_layer::painter(FPS, move |bounds, window| {
                    paint_dots(bounds, dots, color, window);
                })
                .w(px(width))
                .h(px(DOT)),
            )
            .into_any_element();
    }
    let delta = cycle_delta(clock_ms());
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.))
        .mr_1()
        .children((0..dots).map(|index| {
            // Dots light up one after another, left to right.
            let phase = 1.0 - index as f32 / 3.;
            div()
                .id(SharedString::from(format!("{key}-{index}")))
                .size(px(DOT))
                .rounded_full()
                .bg(color)
                .opacity(pulse(delta, phase))
        }))
        .into_any_element()
}

/// The indicator's frame rate: its slow pulse looks the same at 12 fps.
pub(super) const FPS: u32 = 12;

const GAP: f32 = 2.;

fn clock_ms() -> u128 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis()
}

/// The dots at this moment, left to right in `bounds`.
fn paint_dots(bounds: Bounds<Pixels>, dots: usize, color: Hsla, window: &mut Window) {
    let delta = cycle_delta(clock_ms());
    for index in 0..dots {
        let phase = 1.0 - index as f32 / 3.;
        let dot = Bounds::new(
            point(bounds.left() + px(index as f32 * (DOT + GAP)), bounds.top()),
            size(px(DOT), px(DOT)),
        );
        window.paint_quad(
            fill(dot, color.opacity(color.a * pulse(delta, phase))).corner_radii(px(DOT / 2.)),
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn clock_maps_to_cycle_position() {
        assert_eq!(super::cycle_delta(0), 0.0);
        assert_eq!(super::cycle_delta(1100), 0.0);
        assert!((super::cycle_delta(550) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn pulse_stays_visible_and_staggers() {
        for step in 0..=20 {
            let delta = step as f32 / 20.;
            for phase in [0.0, 1. / 3., 2. / 3.] {
                let o = super::pulse(delta, phase);
                assert!((0.3..=1.0).contains(&o), "{o}");
            }
        }
        assert!((super::pulse(0.0, 0.0) - super::pulse(0.0, 1.0)).abs() < 1e-4);
        assert!(super::pulse(0.0, 0.0) != super::pulse(0.0, 1. / 3.));
    }
}
