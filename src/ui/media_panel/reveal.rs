//! The emoji / sticker / GIF panel opening over the composer from its
//! bottom-left corner and fading away when it closes (`surface_reveal`).
//!
//! Telegram Desktop (`TabbedPanel::startShowAnimation`,
//! `chat_helpers/tabbed_panel.cpp`) opens its panel with
//! `Ui::PanelAnimation` from the corner nearest the button
//! (`defaultPanelAnimation`, `lib_ui/ui/widgets/widgets.style:930`): the
//! width grows from half over the first 60 % of `emojiPanShowDuration`
//! (200 ms), the height from 30 % over 90 %, and the panel fades in from
//! 20 % over the first 30 %. Closing fades the panel out
//! (`startOpacityAnimation`).
#![deny(clippy::indexing_slicing)]

use super::{PANEL_HEIGHT, PANEL_WIDTH};
use crate::ui::app::QuillApp;
use crate::ui::surface_reveal::{self, MeasureSize};
use gpui_kit::*;

/// `defaultPanelAnimation`: where the width, height and opacity start,
/// and the share of the run each takes.
const START_WIDTH: f32 = 0.5;
const WIDTH_SHARE: f32 = 0.6;
const START_HEIGHT: f32 = 0.3;
const HEIGHT_SHARE: f32 = 0.9;
const START_OPACITY: f32 = 0.2;
const OPACITY_SHARE: f32 = 0.3;

/// The width, height and opacity shares of a panel `shown` of the way
/// in.
pub(super) fn opening(shown: f32) -> (f32, f32, f32) {
    let part = |start: f32, share: f32| start + (1. - start) * (shown / share).clamp(0., 1.);
    (
        part(START_WIDTH, WIDTH_SHARE),
        part(START_HEIGHT, HEIGHT_SHARE),
        part(START_OPACITY, OPACITY_SHARE),
    )
}

impl QuillApp {
    /// The panel over the composer this frame: the open one, or the one
    /// fading out after it closed. `None` once it is gone.
    pub(in crate::ui) fn media_panel_reveal(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self
            .media_panel_over_composer()
            .then_some(self.pickers.media_panel.tab);
        let scope = self.open_chat_id().map_or(0, |chat| chat.0 as u64);
        let now = surface_reveal::now();
        let animate = surface_reveal::animate(cx);
        let (drawn, moving) = {
            let mut presence = self.frame.surfaces.media_panel.borrow_mut();
            let drawn = presence.follow(state, scope, animate, now);
            (drawn, presence.moving(now))
        };
        if moving {
            self.request_animation_tick(60, cx);
        }
        let drawn = drawn?;
        let size = self.frame.surfaces.media_panel_size.clone();
        let panel = MeasureSize {
            height: Some(size.height.clone()),
            width: Some(size.width.clone()),
            child: crate::ui::anim_layer::occluder(self.media_panel(cx)).into_any_element(),
        };
        if drawn.shown >= 1. {
            return Some(panel.into_any_element());
        }
        if drawn.leaving {
            return Some(div().opacity(drawn.shown).child(panel).into_any_element());
        }
        let full = |measured: Pixels, guess: f32| {
            if measured > px(0.) {
                measured
            } else {
                px(guess)
            }
        };
        let width = full(size.width.get(), PANEL_WIDTH);
        let height = full(size.height.get(), PANEL_HEIGHT);
        let (w, h, opacity) = opening(drawn.shown);
        Some(
            div()
                .relative()
                .overflow_hidden()
                .w(width * w)
                .h(height * h)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .bottom_0()
                        .w(width)
                        .h(height)
                        .opacity(opacity)
                        .child(panel),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::opening;

    #[test]
    fn the_panel_grows_from_its_corner_and_fades_in_first() {
        assert_eq!(opening(0.), (0.5, 0.3, 0.2));
        let (w, h, o) = opening(0.3);
        assert!((w - 0.75).abs() < 1e-6 && h < 1. && (o - 1.).abs() < 1e-6);
        let (w, h, _) = opening(0.6);
        assert!((w - 1.).abs() < 1e-6 && h < 1.);
        assert_eq!(opening(1.), (1., 1., 1.));
    }
}
