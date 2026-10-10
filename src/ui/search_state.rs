//! Search fields: global search and search in chat.

use gpui_kit::component::input::TextareaState;
use gpui_kit::*;

pub(crate) struct SearchUi {
    pub(super) input: Entity<TextareaState>,
    pub(super) chat_input: Entity<TextareaState>,
}

impl SearchUi {
    pub(super) fn new(
        search_input: Entity<TextareaState>,
        chat_search_input: Entity<TextareaState>,
    ) -> Self {
        Self {
            input: search_input,
            chat_input: chat_search_input,
        }
    }
}
