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
        }
    }
}
