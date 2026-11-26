//! Floating menus wear Web K's shape: `.btn-menu` in tweb's
//! `scss/partials/_button.scss` is `$border-radius-medium` (16px) with
//! `0 0 10px rgba(0, 0, 0, .15)` in both themes, and its rows clip to the
//! corners. Classic menus keep the kit's.
//!
//! Menus are built while their window draws, and every window notes its
//! width (`QuillApp::note_window_width`) before it builds anything else, so
//! the window being drawn writes its clip here first and its menus read it.

use gpui_kit::component::popover_shape::{PopoverShape, PopoverShapeHook};
use gpui_kit::*;
use quill::layout_style::Clip;

/// Web K's menu corner radius.
pub(in crate::ui) const MENU_RADIUS: f32 = 16.;

/// The clip of the window being drawn.
#[derive(Clone, Copy, PartialEq)]
struct DrawnWindowClip(Clip);

impl Global for DrawnWindowClip {}

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
    let clip = cx.try_global::<DrawnWindowClip>()?;
    (clip.0 == Clip::Rounded).then(|| PopoverShape {
        radius: px(MENU_RADIUS),
        shadow: menu_shadow(),
    })
}

/// The window about to draw has this clip; the kit's popovers ask through
/// the same answer.
pub(in crate::ui) fn note_drawn_window(clip: Clip, cx: &mut App) {
    if !cx.has_global::<PopoverShapeHook>() {
        cx.set_global(PopoverShapeHook(menu_shape));
    }
    if cx.try_global::<DrawnWindowClip>().map(|drawn| drawn.0) != Some(clip) {
        cx.set_global(DrawnWindowClip(clip));
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "menu_shape_tests.rs"]
mod tests;
