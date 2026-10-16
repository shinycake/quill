//! QUILL: liquid glass for floating surfaces.
//!
//! The app decides whether glass is on and what it looks like through a
//! [`GlassProvider`] global; components ask [`glass_style`] and paint through a
//! [`GlassSurface`] when it answers. With no provider, or a provider that says
//! off, components keep their flat look.
//!
//! [`GlassSurface`] is a port of bezel's `Surface` (github.com/crabtalk/bezel,
//! MIT, `crates/ui/src/surface.rs`). It needs `Window::paint_backdrop_blur`,
//! which only the macOS Metal renderer draws.

use gpui::{
    AnyElement, App, BorderStyle, Bounds, BoxShadow, Corners, Element, GlassEffect, Global,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Window, outline,
};

/// Which glass a surface asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlassVariant {
    /// The glass for text-heavy floating UI such as menus.
    Regular,
}

/// The resolved look of one glass surface.
#[derive(Clone, Copy, Debug)]
pub struct GlassStyle {
    /// The backdrop transfer, lens and rim.
    pub effect: GlassEffect,
    /// A 1px border inside the shape, for Increase Contrast.
    pub border: Option<Hsla>,
}

/// The app's answer to "is glass on, and what does it look like". Read on
/// every render, so the look is a function of the app's state.
#[derive(Clone, Copy)]
pub struct GlassProvider(pub fn(GlassVariant, &App) -> Option<GlassStyle>);

impl Global for GlassProvider {}

/// The glass to paint for `variant`, or `None` when there is no provider or
/// it says glass is off.
pub fn glass_style(variant: GlassVariant, cx: &App) -> Option<GlassStyle> {
    let provider = cx.try_global::<GlassProvider>()?;
    (provider.0)(variant, cx)
}

/// Paints its child as a glass card. The whole subtree goes into one scene
/// layer, in this order: the backdrop blur, the shadows (cut to outside the
/// shape), the optional border, then the child. The child must not paint a
/// fill of its own, which would bury the glass.
pub struct GlassSurface {
    style: GlassStyle,
    corner_radius: Pixels,
    shadows: Vec<BoxShadow>,
    child: AnyElement,
}

impl GlassSurface {
    /// A glass card of `style` around `child`, rounded to `corner_radius`.
    pub fn new(style: GlassStyle, corner_radius: Pixels, child: impl IntoElement) -> Self {
        Self {
            style,
            corner_radius,
            shadows: Vec::new(),
            child: child.into_any_element(),
        }
    }

    /// The drop shadows the card casts, painted outside its shape only.
    pub fn shadows(mut self, shadows: Vec<BoxShadow>) -> Self {
        self.shadows = shadows;
        self
    }
}

impl Element for GlassSurface {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let corners = Corners::all(self.corner_radius).clamp_radii_for_quad_size(bounds.size);
        // Two rims cannot meet in the middle of a small box.
        let half_extent = bounds.size.width.min(bounds.size.height) / 2.;
        let effect = GlassEffect {
            lens: self.style.effect.lens.min(half_extent),
            reach: self.style.effect.reach.min(half_extent),
            ..self.style.effect
        };
        window.paint_layer(bounds, |window| {
            window.paint_backdrop_blur(bounds, corners, effect);
            // After the blur, never before: the blur samples what is beneath
            // it, so a shadow painted first would show through the glass.
            window.paint_drop_shadows_outside(bounds, corners, &self.shadows);
            if let Some(border) = self.style.border {
                window
                    .paint_quad(outline(bounds, border, BorderStyle::Solid).corner_radii(corners));
            }
            self.child.paint(window, cx);
        });
    }
}

impl IntoElement for GlassSurface {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}
