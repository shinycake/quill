//! Reads and writes of the thread-local `RefCell`s of the image budget that
//! never panic: a cell that is borrowed already is skipped.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::thread::LocalKey;

/// Run `action` on the cell's value, unless it is borrowed already (then
/// nothing runs and the caller falls back): a nested call from inside an
/// action must not panic the UI thread.
pub(super) fn with_cell<T, R>(
    cell: &'static LocalKey<RefCell<T>>,
    action: impl FnOnce(&mut T) -> R,
) -> Option<R> {
    cell.with(|cell| {
        cell.try_borrow_mut()
            .ok()
            .map(|mut value| action(&mut value))
    })
}

/// [`with_cell`] for a read.
pub(super) fn read_cell<T, R>(
    cell: &'static LocalKey<RefCell<T>>,
    action: impl FnOnce(&T) -> R,
) -> Option<R> {
    cell.with(|cell| cell.try_borrow().ok().map(|value| action(&value)))
}
