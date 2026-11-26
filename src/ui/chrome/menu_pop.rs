//! Floating menus pop in as Web K's do: `.btn-menu-transition` in tweb's
//! `scss/partials/_button.scss` scales from .8 and fades in over 200 ms on
//! the standard curve (`--btn-menu-transition`, `scss/base.scss`), from the
//! corner the menu opened at (`helpers/positionMenu.ts`). Classic menus
//! appear at once, as before.
//!
//! The menu is drawn through a snapshot for the run
//! (`with_snapshot_animation`, docs/decisions/snapshot-transitions.md), so
//! its rows stay live under the picture. Where the window has no
//! snapshots, or motion is reduced, the run ends at once and the menu
//! appears as Classic's does. A glass menu does not pop: its lens is drawn
//! live and would not scale with the picture. The picture has no backing
//! colour: a menu may be two surfaces with a gap between them (the message
//! menu's reaction strip), which one rounded backing would fill.

use super::menu_shape::menu_entrance;
use crate::ui::glass;
use gpui_kit::component::popover_shape::MenuEnter;
use gpui_kit::component::theme::ActiveTheme;
use gpui_kit::*;
use quill::layout_style::{Curve, Look};
use std::time::Duration;

/// Web K's menu transition.
pub(in crate::ui) const POP_MS: u64 = 200;
/// The scale a menu pops from.
pub(in crate::ui) const POP_FROM: f32 = 0.8;
/// How far a shadow reaches past its box (the kit's measure).
pub(in crate::ui) use gpui_kit::component::popover_shape::shadow_reach;

/// The corner of `bounds` nearest to `anchor`: the point the menu grows
/// from. A menu opened at the pointer has its anchor at a corner unless it
/// was moved inside the window.
pub(in crate::ui) fn pop_origin(bounds: Bounds<Pixels>, anchor: Point<Pixels>) -> Point<Pixels> {
    let center = bounds.center();
    let x = if anchor.x <= center.x {
        bounds.left()
    } else {
        bounds.right()
    };
    let y = if anchor.y <= center.y {
        bounds.top()
    } else {
        bounds.bottom()
    };
    point(x, y)
}

/// The picture at eased time `t`.
pub(in crate::ui) fn pop_draw(
    t: f32,
    bounds: Bounds<Pixels>,
    anchor: Point<Pixels>,
) -> SnapshotDraw {
    let eased = Curve::Standard.ease(t);
    SnapshotDraw::scaled_about(
        pop_origin(bounds, anchor),
        POP_FROM + (1. - POP_FROM) * eased,
    )
    .faded(eased)
}

/// `menu` opened at `anchor`, popping in when the window being drawn is
/// Floating; as it is otherwise.
pub(in crate::ui) fn menu_pop(
    menu: AnyElement,
    anchor: Point<Pixels>,
    look: Look,
    cx: &App,
) -> AnyElement {
    if menu_entrance(look) != MenuEnter::Pop || glass::look(cx).is_glass() {
        return menu;
    }
    menu.with_snapshot_animation(
        "menu-pop",
        Animation::new(Duration::from_millis(POP_MS)),
        move |t, bounds| pop_draw(t, bounds, anchor),
    )
    // Room for the menu's whole shadow in the picture: a clipped shadow
    // would jump when the run ends and the live menu takes over.
    .margin(shadow_reach(&super::menu_shape::menu_shadow(
        cx.theme().is_dark(),
    )))
    .into_any_element()
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "menu_pop_tests.rs"]
mod tests;
