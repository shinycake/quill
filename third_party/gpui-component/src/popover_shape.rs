//! QUILL (Floating layout): lets the app give the kit's popovers another
//! corner radius and shadow, with their content clipped to the corners. The
//! app installs one function; while it answers `Some`, `popover_style` draws
//! that shape instead of the theme radius and the ring-and-shadow edge. See
//! QUILL-CHANGES.md.

use gpui::{App, BoxShadow, Global, Pixels};

/// A popover's corners and shadow.
#[derive(Clone, Debug, PartialEq)]
pub struct PopoverShape {
    pub radius: Pixels,
    pub shadow: Vec<BoxShadow>,
}

/// The app's answer: `Some` draws that shape, `None` the kit's own.
#[derive(Clone, Copy)]
pub struct PopoverShapeHook(pub fn(&App) -> Option<PopoverShape>);

impl Global for PopoverShapeHook {}

/// The shape popovers draw now, if the app gives one.
pub fn popover_shape(cx: &App) -> Option<PopoverShape> {
    let hook = cx.try_global::<PopoverShapeHook>()?;
    (hook.0)(cx)
}
