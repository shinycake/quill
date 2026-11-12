//! What each window's selection covers, as the window last painted it.
#![deny(clippy::indexing_slicing)]

use super::selection_start;
use gpui_kit::*;
use std::cell::Cell;

thread_local! {
    /// What each window's selection covers, as that window last painted
    /// it. Every window has its own selection, so a copy reads the entry
    /// of the window it runs in; see [`selected_message_text`].
    static SELECTED: Cell<std::collections::HashMap<WindowId, WindowSelected>> =
        Cell::new(std::collections::HashMap::new());
}

/// Runs `f` on a thread-local table taken out of its cell and put back, so
/// no guard is held and nothing can be borrowed twice.
pub(super) fn with_taken<T: Default + 'static, R>(
    cell: &'static std::thread::LocalKey<Cell<T>>,
    f: impl FnOnce(&mut T) -> R,
) -> R {
    let mut table = cell.take();
    let out = f(&mut table);
    cell.set(table);
    out
}

/// One window's selection, as its last paint saw it.
#[derive(Default)]
pub(super) struct WindowSelected {
    /// The message each painted text holding part of the selection belongs
    /// to, by document order. A text that holds none lets go, so the first
    /// entry is the message the selection starts in.
    pub(super) messages: std::collections::BTreeMap<u64, (i64, u64)>,
    /// Custom emoji inside the selection, by (document order, byte
    /// offset): their fallback text replaces the placeholders in the
    /// copied string.
    pub(super) emoji: std::collections::BTreeMap<(u64, usize), String>,
    /// Where the selection starts (`selection_start`).
    pub(super) start: selection_start::SelectionStart,
}

/// Runs `f` on the selection record of `window`, dropping the records of
/// windows that have closed once a few have piled up.
pub(super) fn with_selected<R>(
    window: &Window,
    cx: &App,
    f: impl FnOnce(&mut WindowSelected) -> R,
) -> R {
    let id = window.window_handle().window_id();
    with_taken(&SELECTED, |selected| {
        if selected.len() > 8 && !selected.contains_key(&id) {
            let open: Vec<WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
            selected.retain(|window, _| open.contains(window));
        }
        f(selected.entry(id).or_default())
    })
}
