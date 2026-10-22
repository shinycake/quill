//! Kit buttons built and erased in a frame of their own.
//!
//! A debug build gives every by-value step of a `Button` chain its own
//! 6.7 KB stack slot in the function that writes the chain, for the life of
//! that function. A panel with ten buttons written inline reserves several
//! hundred KB before it draws anything. Calling one of these instead keeps
//! the chain in a short frame that is gone before the caller's next child
//! (docs/decisions/main-thread-stack.md). Release builds are unaffected.
#![deny(clippy::indexing_slicing)]

use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

/// A ghost button with a text label.
pub(super) fn ghost_text_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    Button::new(id)
        .label(label)
        .ghost()
        .on_click(on_click)
        .into_any_element()
}

/// An extra-small ghost icon button whose tooltip is also its
/// accessibility label.
pub(super) fn ghost_icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let label = label.into();
    Button::new(id)
        .icon(icon)
        .ghost()
        .xsmall()
        .tooltip(label.clone())
        .accessibility_label(label)
        .on_click(on_click)
        .into_any_element()
}

/// `ghost_icon_button` without the tooltip: only screen readers name it.
pub(super) fn ghost_icon_button_untitled(
    id: impl Into<ElementId>,
    icon: IconName,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    Button::new(id)
        .icon(icon)
        .ghost()
        .xsmall()
        .accessibility_label(label)
        .on_click(on_click)
        .into_any_element()
}
