//! Focusing a field inside a kit dialog. A kit dialog takes focus when it
//! opens, so a field focused at open time loses it; the box keeps the field
//! to focus until its first render and hands it to [`focus_after_mount`].
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

/// Focus `input` once the dialog has mounted.
pub(crate) fn focus_after_mount(
    input: Option<Entity<TextareaState>>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(input) = input {
        window.defer(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx));
        });
    }
}
