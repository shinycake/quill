//! QUILL (Floating layout): lets the app give the kit's popovers another
//! corner radius and shadow, with their content clipped to the corners. The
//! app installs one function; while it answers `Some`, `popover_style` draws
//! that shape instead of the theme radius and the ring-and-shadow edge. See
//! QUILL-CHANGES.md.

use gpui::{App, BoxShadow, Global, Hsla, Pixels, Window};

/// A popover's corners and shadow, and how a dropdown enters.
#[derive(Clone, Debug, PartialEq)]
pub struct PopoverShape {
    pub radius: Pixels,
    pub shadow: Vec<BoxShadow>,
    /// A fill in place of the theme's popover colour (`None` keeps it).
    pub fill: Option<Hsla>,
    /// A hairline round the edge (`None` draws none).
    pub edge: Option<Hsla>,
    /// How a dropdown enters.
    pub enter: MenuEnter,
}

/// How a menu enters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEnter {
    /// The kit's fade and slide.
    FadeSlide,
    /// Scale from .8 and fade, through a snapshot.
    Pop,
}

/// The app's answer for a popover in that window: `Some` draws that
/// shape, `None` the kit's own.
#[derive(Clone, Copy)]
pub struct PopoverShapeHook(pub fn(&Window, &App) -> Option<PopoverShape>);

impl Global for PopoverShapeHook {}

/// The shape popovers in `window` draw, if the app gives one.
pub fn popover_shape(window: &Window, cx: &App) -> Option<PopoverShape> {
    let hook = cx.try_global::<PopoverShapeHook>()?;
    (hook.0)(window, cx)
}

/// How far a popover's shadows reach past its edge: a capture of it holds
/// them whole.
pub fn shadow_reach(shadows: &[BoxShadow]) -> Pixels {
    shadows
        .iter()
        .map(|shadow| {
            let offset = shadow.offset.x.abs().max(shadow.offset.y.abs());
            shadow.blur_radius * 3. + offset + shadow.spread_radius.max(Pixels::ZERO)
        })
        .fold(Pixels::ZERO, Pixels::max)
}
