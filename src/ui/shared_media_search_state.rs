//! What the gallery's search field keeps on the UI side: the text field,
//! the `(chat, tab)` its text belongs to, and the count of the keystroke
//! the debounce is waiting on. The query a tab answers lives in
//! `SharedMediaTabState::query`.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::SharedMediaTab;

/// What the field's text belongs to, like the grid's `PageKey`.
pub(crate) type SearchKey = (ChatId, SharedMediaTab);

pub(crate) struct SharedMediaSearchUi {
    pub(super) input: Entity<InputState>,
    /// The key `input` was made for: a different key means a fresh, empty
    /// field, so text never carries over to another chat or tab.
    pub(super) key: Option<SearchKey>,
    /// Bumped by every keystroke; a debounce timer only fires for the
    /// keystroke it was started by.
    pub(super) keystroke: u64,
}

impl SharedMediaSearchUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            input: Self::fresh_input(window, cx),
            key: None,
            keystroke: 0,
        }
    }

    /// An empty field whose edits go to the gallery search.
    pub(super) fn fresh_input(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr(keys::DLG_FILTER)));
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = state.read(cx).value().to_string();
                    // The gallery is drawn in the main window only.
                    this.in_window_scope(None, cx, |this, cx| {
                        this.shared_media_search_typed(&text, cx);
                    });
                }
            },
        )
        .detach();
        input
    }
}
