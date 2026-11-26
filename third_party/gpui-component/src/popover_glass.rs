//! QUILL (glass effects): lets the app draw the kit's popovers, toasts and
//! segmented tab selection as backdrop glass. The app installs one function; while it returns a look,
//! `popover_style` and the notification card draw that glass instead of
//! their fill, edge and shadow. See QUILL-CHANGES.md.

use gpui::{App, Global, LensStyle, Styled};

/// Which kit surface asks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PopoverGlassSurface {
    /// Everything `popover_style` draws: popovers, menus, selects,
    /// comboboxes, the date picker, input completions.
    Popover,
    /// The notification card.
    Toast,
    /// The sliding selection of a segmented `TabBar`; `travel` is how far
    /// it still has to go, in widths of the tab it heads for (0 at rest).
    TabSelection {
        /// 0 at rest, towards 1 while it slides.
        travel: f32,
    },
    /// The lit row of a menu or list (hover, keyboard or selection).
    Highlight,
}

/// The app's answer for a surface: `Some` draws glass of that look, `None`
/// keeps the kit's solid surface.
#[derive(Clone, Copy)]
pub struct PopoverGlass(pub fn(PopoverGlassSurface, &App) -> Option<LensStyle>);

impl Global for PopoverGlass {}

/// The look `surface` draws now, if glass.
pub fn popover_glass(surface: PopoverGlassSurface, cx: &App) -> Option<LensStyle> {
    let glass = cx.try_global::<PopoverGlass>()?;
    (glass.0)(surface, cx)
}

/// QUILL (glass effects): `this` lit as the highlighted row of a menu or
/// list: a glass lens where the app draws glass, the theme's accent fill
/// otherwise. Works on an element and inside a `hover` refinement.
pub fn highlight<E: Styled>(this: E, cx: &App) -> E {
    let theme = crate::ActiveTheme::theme(cx);
    match popover_glass(PopoverGlassSurface::Highlight, cx) {
        Some(lens) => this
            .backdrop_glass(Some(lens))
            .text_color(theme.popover_foreground),
        None => this
            .bg(theme.tokens.accent)
            .text_color(theme.accent_foreground),
    }
}
