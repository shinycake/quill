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
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Rgba, Window, hsla,
    outline, point, px,
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
    /// The corner radius of the glass shape. Glass is rounder than the flat
    /// popover; inner highlights are cut concentric with it.
    pub radius: Pixels,
}

impl GlassStyle {
    /// macOS 26's Regular glass, fitted to `NSGlassEffectView` captures over
    /// flat greys, stripes, checkers and photos, then held to the range where
    /// text in `primary` keeps 4.5:1 and text in `secondary` 3:1 over any
    /// backdrop. `increase_contrast` raises those to 7:1 and 4.5:1 and adds a
    /// border in `primary`.
    pub fn regular(dark: bool, primary: Hsla, secondary: Hsla, increase_contrast: bool) -> Self {
        let (min_luminance, max_luminance) =
            readable_luminance(dark, primary, secondary, increase_contrast);
        // Apple's interior level over a flat grey `x`: dark 31/255 + 0.72 x
        // easing under 0.48; light 0.55 + 0.55 x easing under 1.05.
        let (tint, gain, ceiling, saturation, shade) = if dark {
            (0.122, 0.72, 0.48, 0.92, 0.6)
        } else {
            (0.55, 0.55, 1.05, 0.85, 0.5)
        };
        Self {
            effect: GlassEffect {
                blur_radius: px(11.),
                lens: px(20.),
                reach: px(40.),
                magnify: 0.6,
                gain,
                saturation,
                tint: hsla(0., 0., tint, 1.),
                ceiling,
                min_luminance,
                max_luminance,
                edge: 0.2,
                edge_width: px(0.8),
                specular: 0.05,
                specular_width: px(2.5),
                light: point(0., 1.),
                shade,
                edge_aa: px(0.5),
            },
            border: increase_contrast.then_some(primary),
            radius: px(12.),
        }
    }
}

/// The relative luminance range glass must stay inside for text in
/// `primary` to keep 4.5:1 and in `secondary` 3:1 (7:1 and 4.5:1 with
/// `increase_contrast`), with a little to spare: light text needs the glass
/// under the range's top, dark text over its bottom.
pub fn readable_luminance(
    dark: bool,
    primary: Hsla,
    secondary: Hsla,
    increase_contrast: bool,
) -> (f32, f32) {
    const SPARE: f32 = 0.1;
    let (first, second) = if increase_contrast { (7., 4.5) } else { (4.5, 3.) };
    let (primary, secondary) = (luminance(primary), luminance(secondary));
    if dark {
        let top = ((primary + 0.05) / (first + SPARE)).min((secondary + 0.05) / (second + SPARE));
        (0., (top - 0.05).max(0.))
    } else {
        let bottom = ((primary + 0.05) * (first + SPARE)).max((secondary + 0.05) * (second + SPARE));
        ((bottom - 0.05).min(1.), 1.)
    }
}

/// WCAG relative luminance of an opaque colour.
fn luminance(color: Hsla) -> f32 {
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    let color = Rgba::from(color);
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
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
    /// A glass card of `style` around `child`, rounded to the style's radius,
    /// casting [`glass_shadow`].
    pub fn new(style: GlassStyle, child: impl IntoElement) -> Self {
        Self {
            style,
            corner_radius: style.radius,
            shadows: glass_shadow(),
            child: child.into_any_element(),
        }
    }
}

/// The shadow under a floating glass card, measured off macOS 26's
/// `NSGlassEffectView`: a wide soft shadow that falls mostly below it, and a
/// tight one that seats it. Painted outside the shape only.
pub fn glass_shadow() -> Vec<BoxShadow> {
    vec![
        BoxShadow::new(px(0.), px(8.), hsla(0., 0., 0., 0.14))
            .blur_radius(px(24.))
            .spread_radius(px(0.)),
        BoxShadow::new(px(0.), px(1.), hsla(0., 0., 0., 0.08))
            .blur_radius(px(4.))
            .spread_radius(px(0.)),
    ]
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
