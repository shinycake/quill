//! A dialog's body, built once and handed to the kit's `Fn` content closure.
//! The kit may call the closure again, and the body is an element that
//! cannot be built twice from the closure's arguments: a `Cell` hands it to
//! the first call and an empty box to any later one. No borrow is held, so
//! a call that re-enters cannot panic.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::dialog::DialogContent;
use gpui_kit::*;
use std::cell::Cell;
use std::rc::Rc;

/// The content closure that shows `body` on its first call.
pub(super) fn once_body(
    body: AnyElement,
) -> impl Fn(DialogContent, &mut Window, &mut App) -> DialogContent + 'static {
    let body = Rc::new(Cell::new(Some(body)));
    move |content, _, _| content.child(body.take().unwrap_or_else(|| div().into_any_element()))
}
