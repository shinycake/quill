//! QUILL (glass effects): lets the app draw the kit's popovers and toasts as
//! backdrop glass. The app installs one function; while it returns a look,
//! `popover_style` and the notification card draw that glass instead of
//! their fill, edge and shadow. See QUILL-CHANGES.md.

use gpui::{App, Global, LensStyle};

/// Which kit surface asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PopoverGlassSurface {
    /// Everything `popover_style` draws: popovers, menus, selects,
    /// comboboxes, the date picker, input completions.
    Popover,
    /// The notification card.
    Toast,
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
