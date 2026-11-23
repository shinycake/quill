//! The one call a surface makes to be glass.
#![deny(clippy::indexing_slicing)]

use super::{GlassRole, lens};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{App, Hsla, IntoElement, LensStyle, RenderOnce, Styled, Window, transparent_black};

pub(crate) trait GlassSurface: Styled + Sized {
    /// Glass of `role` while the look is glass, else `solid` (exactly the
    /// styles the surface draws today). A glass surface drops its own fill,
    /// border and shadow: the lens draws the tint, the rim and the shadow,
    /// so `solid` is where those go.
    fn glass_or(self, role: GlassRole, cx: &App, solid: impl FnOnce(Self) -> Self) -> Self {
        match lens(role, cx) {
            Some(style) => self.backdrop_glass(Some(style)),
            None => solid(self),
        }
    }
}

impl<E: Styled> GlassSurface for E {}

/// A kit button floating over content (`role` is `Control` or
/// `MediaIcon`) as glass: the ghost variant (its hover and press tints are
/// translucent, so the glass shows through them), no fill or border of its
/// own, and the lens under it. Under the solid look it is left as built.
pub(crate) fn glass_button(button: Button, lens: Option<LensStyle>) -> Button {
    match lens {
        Some(style) => button
            .ghost()
            .bg(transparent_black())
            .border_color(transparent_black())
            .backdrop_glass(Some(style)),
        None => button,
    }
}

/// [`glass_button`] for a button over photos and video: white labels in
/// both themes, as the media recipe is always dark.
pub(crate) fn media_glass_button(button: Button, cx: &App) -> Button {
    match lens(GlassRole::MediaIcon, cx) {
        Some(style) => glass_button(button, Some(style)).text_color(gpui_kit::white()),
        None => button,
    }
}

/// An element built where no `App` is at hand (a free builder function):
/// it takes glass of `role`, or the `solid` fill, when it renders.
#[derive(IntoElement)]
pub(crate) struct GlassFill<E: Styled + IntoElement + 'static> {
    element: E,
    role: GlassRole,
    solid: Hsla,
}

impl<E: Styled + IntoElement + 'static> GlassFill<E> {
    pub(crate) fn new(element: E, role: GlassRole, solid: Hsla) -> Self {
        Self {
            element,
            role,
            solid,
        }
    }
}

impl<E: Styled + IntoElement + 'static> RenderOnce for GlassFill<E> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let solid = self.solid;
        self.element.glass_or(self.role, cx, |this| this.bg(solid))
    }
}
