//! A paused video tile of the voice chat, as tdesktop draws it
//! (`calls_group_viewport_raster.cpp`): the last frame scaled into
//! 90 × 90 and blurred (radius 15; the live player's
//! `paused_overlay`), under a black shadow (alpha 80) with the pause
//! icon, and "Video is paused" when the view is wide (in Quill: the
//! full-screen stage).
#![deny(clippy::indexing_slicing)]

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// tdesktop `Viewport::kShadowMaxAlpha` (80 of 255).
const SHADOW: u32 = 0x0000_0050;

/// The shadow, pause icon and (when `wide`) "Video is paused" over a
/// paused tile.
pub(super) fn paused_overlay(wide: bool) -> Div {
    div()
        .absolute()
        .inset_0()
        .bg(rgba(SHADOW))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .text_color(white())
        .child(Icon::new(IconName::Pause).with_size(px(if wide { 40. } else { 28. })))
        .when(wide, |this| {
            this.child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr(keys::GROUP_CALL_VIDEO_PAUSED)),
            )
        })
}
