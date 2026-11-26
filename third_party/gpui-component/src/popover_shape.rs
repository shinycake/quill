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

/// QUILL (concentric corners): the radius of a rounded shape set `inset`
/// inside a rounded container of radius `outer`, so the two curves share a
/// centre. A container too tight to inset from gives square corners.
pub fn concentric_radius(outer: Pixels, inset: Pixels) -> Pixels {
    (outer - inset).max(Pixels::ZERO)
}

/// The corner radius and edge width `popover_style` gives a popover in
/// `window`. Glass draws its rim inside the lens, so it takes no width.
pub fn popover_frame(window: &Window, cx: &App) -> (Pixels, Pixels) {
    let shape = popover_shape(window, cx);
    let radius = shape
        .as_ref()
        .map_or(crate::ActiveTheme::theme(cx).radius, |shape| shape.radius);
    let glass = crate::popover_glass::popover_glass(
        crate::popover_glass::PopoverGlassSurface::Popover,
        cx,
    )
    .is_some();
    let edge = match shape.and_then(|shape| shape.edge) {
        Some(_) if !glass => gpui::px(1.),
        _ => Pixels::ZERO,
    };
    (radius, edge)
}

/// The radius of a row (a menu item, a list row) set `padding` inside a
/// popover in `window`: concentric with the popover's corners.
pub fn popover_row_radius(padding: Pixels, window: &Window, cx: &App) -> Pixels {
    let (radius, edge) = popover_frame(window, cx);
    concentric_radius(radius, padding + edge)
}
