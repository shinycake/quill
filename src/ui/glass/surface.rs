//! The one call a surface makes to be glass.
#![deny(clippy::indexing_slicing)]

use super::{GlassRole, lens};
use gpui_kit::{App, Styled};

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
