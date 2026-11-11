//! The body a kit dialog's content closure hands over once.
//!
//! The kit takes an `Fn` content closure, but a box builds its body once
//! per open. The body waits in a `Cell` and the closure takes it, so a
//! second call cannot hit a live borrow (a `RefCell` would panic there).

use super::shell::scrollable_dialog_content;
use gpui_kit::component::dialog::DialogContent;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// A dialog content closure that puts `body` in the box on its first call
/// and an empty element on any later one.
pub(crate) fn one_shot_body(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static {
    let slot = Rc::new(Cell::new(Some(body)));
    scrollable_dialog_content(move |content, _, _| {
        let body = slot.take().unwrap_or_else(|| div().into_any_element());
        content.child(body)
    })
}
