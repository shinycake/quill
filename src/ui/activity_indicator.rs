//! The small animated glyph before a peer-activity line ("typing", "Dana is
//! recording a voice message") in the chat row and the conversation header.
//!
//! Only built while an action is active, so an idle chat list animates
//! nothing.

use gpui_kit::*;
use quill::state::ActivityIndicator;
use std::time::Duration;

const DOT: f32 = 3.;

/// Opacity of a dot whose pulse is offset by `phase` (0..1) of the cycle.
fn pulse(delta: f32, phase: f32) -> f32 {
    let t = (delta + phase).fract();
    0.3 + 0.7 * (0.5 - 0.5 * (t * std::f32::consts::TAU).cos())
}

/// `key` must be unique per visible indicator (animation state is keyed by
/// element id).
pub(super) fn activity_indicator(
    indicator: ActivityIndicator,
    color: Hsla,
    key: SharedString,
) -> impl IntoElement {
    let dots = match indicator {
        ActivityIndicator::Dots => 3,
        ActivityIndicator::Pulse => 1,
    };
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
                .with_animation(
                    SharedString::from(format!("{key}-pulse-{index}")),
                    Animation::new(Duration::from_millis(1100)).repeat(),
                    move |dot, delta| dot.opacity(pulse(delta, phase)),
                )
        }))
}

#[cfg(test)]
mod tests {
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
