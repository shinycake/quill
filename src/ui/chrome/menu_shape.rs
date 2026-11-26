//! Floating menus wear Web K's shape: `.btn-menu` in tweb's
//! `scss/partials/_button.scss` is `$border-radius-medium` (16px) with
//! `0 0 10px rgba(0, 0, 0, .15)` in both themes, and its rows clip to the
//! corners. Classic menus keep the kit's.
//!
//! Menus are built while their window draws, and every window notes its
//! width (`QuillApp::note_window_width`) before it builds anything else, so
//! the window being drawn writes its metrics here first and its menus read
//! them: their shape here, their entrance in `menu_pop`.

use gpui_kit::component::popover_shape::{PopoverShape, PopoverShapeHook};
use gpui_kit::*;
use quill::layout_style::{ChromeMetrics, Clip, MenuEnter};

/// Web K's menu corner radius.
pub(in crate::ui) const MENU_RADIUS: f32 = 16.;

/// The metrics of the window being drawn.
#[derive(Clone, Copy)]
struct DrawnWindow(&'static ChromeMetrics);

impl Global for DrawnWindow {}

/// Web K's menu shadow.
pub(in crate::ui) fn menu_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0., 0., 0., 0.15),
        offset: point(px(0.), px(0.)),
        blur_radius: px(10.),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The shape menus in the window being drawn take: `Some` in Floating.
pub(in crate::ui) fn menu_shape(cx: &App) -> Option<PopoverShape> {
    let drawn = cx.try_global::<DrawnWindow>()?;
    (drawn.0.clip == Clip::Rounded).then(|| PopoverShape {
        radius: px(MENU_RADIUS),
        shadow: menu_shadow(),
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
