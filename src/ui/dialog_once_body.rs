//! A dialog body built once and handed to the dialog's content closure.
//!
//! The builder of a dialog makes its body inside `app.update` (it needs the
//! app's `Context` for click listeners); the content closure runs later,
//! outside that update. The body moves into the closure here through a
//! `Cell`, so there is no `RefCell` borrow to fail; a second run of the
//! same closure draws an empty body instead of panicking.

use gpui_kit::component::dialog::DialogContent;
use gpui_kit::*;
use std::cell::Cell;

/// The scrollable dialog content (`shell::scrollable_dialog_content`)
/// showing `body`.
pub(crate) fn once_dialog_body(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent {
    let slot = Cell::new(Some(body));
    super::shell::scrollable_dialog_content(move |content, _, _| {
        content.child(slot.take().unwrap_or_else(|| div().into_any_element()))
    })
}
