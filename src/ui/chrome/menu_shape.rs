//! Floating menus wear Web K's shape: `.btn-menu` in tweb's
//! `scss/partials/_button.scss` is `$border-radius-medium` (16px) with
//! `0 0 10px rgba(0, 0, 0, .15)` in both themes, and its rows clip to the
//! corners. Classic menus keep the kit's.
//!
//! In dark the cards are near black, the menu's own fill is too, and a 15%
//! black shadow does not show on them, so a menu would merge with the card
//! under it. Web K's dark menu is lighter than its columns; here the menu
//! takes a fill a step lighter than the cards, a faint light edge and a
//! deeper shadow.
//!
//! Menus are built while their window draws, and every window notes its
//! width (`QuillApp::note_window_width`) before it builds anything else, so
//! the window being drawn writes its metrics here first and its menus read
//! them: their shape here, their entrance in `menu_pop`.

use gpui_kit::component::popover_shape::{PopoverShape, PopoverShapeHook};
use gpui_kit::component::theme::Theme;
use gpui_kit::*;
use quill::layout_style::{ChromeMetrics, Clip, MenuEnter};

/// Web K's menu corner radius.
pub(in crate::ui) const MENU_RADIUS: f32 = 16.;

/// The metrics of the window being drawn.
#[derive(Clone, Copy)]
struct DrawnWindow(&'static ChromeMetrics);

impl Global for DrawnWindow {}

/// Web K's menu shadow; deeper in dark, where its 15% black would not show.
pub(in crate::ui) fn menu_shadow(dark: bool) -> Vec<BoxShadow> {
    if dark {
        return vec![BoxShadow {
            color: hsla(0., 0., 0., 0.55),
            offset: point(px(0.), px(4.)),
            blur_radius: px(18.),
            spread_radius: px(0.),
            inset: false,
        }];
    }
    vec![BoxShadow {
        color: hsla(0., 0., 0., 0.15),
        offset: point(px(0.), px(0.)),
        blur_radius: px(10.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// How much lighter a dark menu is than the theme's popover fill.
const DARK_MENU_LIFT: f32 = 0.10;

/// The dark menu's fill and edge (`None, None` in light).
fn dark_surface(cx: &App) -> (Option<Hsla>, Option<Hsla>) {
    match cx.try_global::<Theme>() {
        Some(theme) if theme.is_dark() => {
            let base = theme.popover;
            let fill = hsla(base.h, base.s, (base.l + DARK_MENU_LIFT).min(1.), 1.);
            (Some(fill), Some(hsla(0., 0., 1., 0.10)))
        }
        _ => (None, None),
    }
}

/// The shape menus in the window being drawn take: `Some` in Floating.
pub(in crate::ui) fn menu_shape(cx: &App) -> Option<PopoverShape> {
    let drawn = cx.try_global::<DrawnWindow>()?;
    if drawn.0.clip != Clip::Rounded {
        return None;
    }
    let (fill, edge) = dark_surface(cx);
    Some(PopoverShape {
        radius: px(MENU_RADIUS),
        shadow: menu_shadow(edge.is_some()),
        fill,
        edge,
        pop: drawn.0.motion.menu == MenuEnter::Pop,
    })
}

/// How menus in the window being drawn enter: Classic's when no window
/// has drawn.
pub(in crate::ui) fn menu_entrance(cx: &App) -> MenuEnter {
    cx.try_global::<DrawnWindow>()
        .map_or(MenuEnter::FadeSlide, |drawn| drawn.0.motion.menu)
}

/// The window about to draw has these metrics; the kit's popovers ask
/// through the same answer.
pub(in crate::ui) fn note_drawn_window(metrics: &'static ChromeMetrics, cx: &mut App) {
    if !cx.has_global::<PopoverShapeHook>() {
        cx.set_global(PopoverShapeHook(menu_shape));
    }
    let same = cx
        .try_global::<DrawnWindow>()
        .is_some_and(|drawn| std::ptr::eq(drawn.0, metrics));
    if !same {
        cx.set_global(DrawnWindow(metrics));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "menu_shape_tests.rs"]
mod tests;
