//! The mark on a Premium sticker in a list (tdesktop `StickerPremiumMark`,
//! `boxes/sticker_set_box.cpp`): a small Premium-gradient disc in the
//! sticker's bottom-right corner with a lock for an account without
//! Premium and a star for one with it.

use gpui_kit::assets::IconName;
use gpui_kit::component::*;
use gpui_kit::*;

/// Side of the disc.
const SIZE: f32 = 18.;
/// tdesktop `roundRadiusSmall`: the gap to the cell's corner.
const INSET: f32 = 4.;

/// The Premium badge gradient (tdesktop `Premium::ButtonGradientStops`).
pub(crate) fn premium_gradient() -> Background {
    linear_gradient(
        135.,
        linear_color_stop(rgb(0x6b93ff), 0.),
        linear_color_stop(rgb(0xc66cd8), 1.),
    )
}

/// The disc, placed absolutely in the cell's bottom-right corner (the cell
/// must be `relative`). `unlocked` is whether the account has Premium.
pub(super) fn premium_mark(unlocked: bool) -> AnyElement {
    div()
        .absolute()
        .right(px(INSET))
        .bottom(px(INSET))
        .size(px(SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(premium_gradient())
        .child(
            Icon::new(if unlocked {
                IconName::StarFill
            } else {
                IconName::Lock
            })
            .with_size(px(10.))
            .text_color(rgb(0xffffff)),
        )
        .into_any_element()
}
