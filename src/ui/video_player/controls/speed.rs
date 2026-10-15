//! The speed control: the controller's speed button (a gauge at 1×, the
//! rate as a pill otherwise, "2×" while Space is held) and the content of
//! the speed popover (tdesktop `mediaviewSpeedMenu`: a 0.5–2.5 slider over
//! the named presets of `FillSpeedMenu`, the current one checked).
//!
//! The popover itself stays `QuillApp::speed_dial`, shared with the audio
//! player bar.
#![deny(clippy::indexing_slicing)]

use super::metrics::SIDE_BUTTON;
use super::overlay_button;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderState};
use gpui_kit::component::{ActiveTheme, Icon, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::viewer_extras::{SPEED_PRESETS, equal_speeds, speed_label};
use std::rc::Rc;

/// What a preset row does when picked.
pub(crate) type PickHandler = Rc<dyn Fn(f64, &mut Window, &mut App)>;

/// What the speed button shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpeedBadge {
    /// Normal speed: the gauge icon.
    Normal,
    /// The rate as text ("1.5×", or "2×" while boosted).
    Rate(String),
}

/// The speed button's face for the chosen `speed`, or the boost rate
/// while Space is held.
pub(crate) fn speed_badge(speed: f64, boosted: bool) -> SpeedBadge {
    if boosted {
        SpeedBadge::Rate(speed_label(quill::media_viewer::SPEED_BOOST_RATE))
    } else if equal_speeds(speed, 1.0) {
        SpeedBadge::Normal
    } else {
        SpeedBadge::Rate(speed_label(speed))
    }
}

/// The viewer's speed button (the popover's trigger).
pub(crate) fn speed_trigger(
    id: impl Into<ElementId>,
    speed: f64,
    boosted: bool,
    cx: &App,
) -> Button {
    let button = Button::new(id)
        .custom(overlay_button(cx))
        .tooltip("Playback speed")
        .accessibility_label(format!(
            "Playback speed {}",
            speed_label(if boosted {
                quill::media_viewer::SPEED_BOOST_RATE
            } else {
                speed
            })
        ));
    match speed_badge(speed, boosted) {
        SpeedBadge::Normal => button.icon(IconName::Gauge).with_size(SIDE_BUTTON),
        SpeedBadge::Rate(text) => button.compact().h(SIDE_BUTTON).child(
            div()
                .px_1()
                .rounded_full()
                .border_1()
                .border_color(gpui_kit::white().opacity(0.8))
                .text_xs()
                .font_semibold()
                .child(text),
        ),
    }
}

/// The popover's content: the slider, then one row per preset.
#[derive(IntoElement)]
pub(crate) struct SpeedMenu {
    id: &'static str,
    slider: Entity<SliderState>,
    current: f64,
    on_pick: PickHandler,
}

impl SpeedMenu {
    pub(crate) fn new(
        id: &'static str,
        slider: Entity<SliderState>,
        current: f64,
        on_pick: PickHandler,
    ) -> Self {
        Self {
            id,
            slider,
            current,
            on_pick,
        }
    }
}

impl RenderOnce for SpeedMenu {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let rows = SPEED_PRESETS.iter().map(|(speed, name)| {
            let (speed, name) = (*speed, *name);
            let chosen = equal_speeds(speed, self.current);
            let pick = self.on_pick.clone();
            Button::new((self.id, (speed * 10.0).round() as usize))
                .ghost()
                .small()
                .w_full()
                .accessibility_label(format!("{name} {}", speed_label(speed)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().text_left().child(name))
                        .child(div().text_color(muted).child(speed_label(speed)))
                        .child(
                            div()
                                .size_4()
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(chosen, |this| this.child(Icon::new(IconName::Check))),
                        ),
                )
                .on_click(move |_, window, cx| pick(speed, window, cx))
        });
        div()
            .w(px(220.))
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_xs()
                    .text_color(muted)
                    .child("Speed")
                    .child(speed_label(self.current)),
            )
            .child(div().px_2().py_1().child(Slider::new(&self.slider)))
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{SpeedBadge, speed_badge};

    #[test]
    fn the_badge_names_the_rate_unless_normal() {
        assert_eq!(speed_badge(1.0, false), SpeedBadge::Normal);
        assert_eq!(speed_badge(1.5, false), SpeedBadge::Rate("1.5×".into()));
        assert_eq!(speed_badge(0.5, false), SpeedBadge::Rate("0.5×".into()));
        // The held-Space boost always reads 2×, whatever was chosen.
        assert_eq!(speed_badge(1.0, true), SpeedBadge::Rate("2×".into()));
        assert_eq!(speed_badge(1.5, true), SpeedBadge::Rate("2×".into()));
    }
}
