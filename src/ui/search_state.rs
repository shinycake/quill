//! Search fields: global search and search in chat.
#![deny(clippy::indexing_slicing)]

use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

pub(crate) struct SearchUi {
    pub(super) input: Entity<TextareaState>,
    pub(super) chat_input: Entity<TextareaState>,
    /// The Channels tab lists all joined channels, not the first five.
    pub(super) channels_expanded: bool,
    /// Sideways scroll of the tab strip.
    pub(super) tabs_scroll: ScrollHandle,
    /// Vertical scroll of the results under the strip (infinite paging).
    pub(super) results_scroll: ScrollHandle,
    /// The search generation the results scroll was last reset for.
    pub(super) seen_generation: std::cell::Cell<u64>,
    /// Sideways scroll of the top-people strip.
    pub(super) top_scroll: ScrollHandle,
    /// When the top-people strip last expanded or collapsed (its
    /// animation runs from here).
    pub(super) top_toggled_at: std::cell::Cell<Option<std::time::Instant>>,
    /// A press on the top-people strip: where the pointer and the scroll
    /// started, and whether it has become a drag.
    pub(super) top_drag: std::cell::Cell<Option<TopDrag>>,
}

/// A drag that scrolls the top-people strip (`stripMouseMoveEvent`).
#[derive(Clone, Copy)]
pub(super) struct TopDrag {
    pub(super) start_x: Pixels,
    pub(super) start_scroll: Pixels,
    pub(super) moved: bool,
}

impl SearchUi {
    pub(super) fn new(
        search_input: Entity<TextareaState>,
        chat_search_input: Entity<TextareaState>,
    ) -> Self {
        Self {
            input: search_input,
            chat_input: chat_search_input,
            channels_expanded: false,
            tabs_scroll: ScrollHandle::new(),
            results_scroll: ScrollHandle::new(),
            seen_generation: std::cell::Cell::new(0),
            top_scroll: ScrollHandle::new(),
            top_toggled_at: std::cell::Cell::new(None),
            top_drag: std::cell::Cell::new(None),
        }
    }
}
