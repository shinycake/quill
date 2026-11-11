//! What the gallery's search field keeps on the UI side: the text field,
//! the `(chat, tab)` its text belongs to, and the debounce of its last
//! keystroke. The query a tab answers lives in
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

/// The gallery's search: nothing while no searchable tab is open, else the
/// field of one `(chat, tab)` with the debounce of its last keystroke. The
/// whole value is replaced when the key changes, so text never carries over
/// to another chat or tab and no search of the old one is still waiting.
pub(crate) enum GallerySearch {
    Closed,
    Open(SearchField),
}

pub(crate) struct SearchField {
    /// What the field's text belongs to.
    pub(super) key: SearchKey,
    pub(super) input: Entity<InputState>,
    /// The search waiting for a quiet moment; the next keystroke or the
    /// field going replaces (and so cancels) it.
    pub(super) debounce: Option<Task<()>>,
}

impl crate::ui::on_screen::Farewell for GallerySearch {}

impl GallerySearch {
    pub(super) fn new() -> Self {
        Self::Closed
    }

    /// The `(chat, tab)` the field was made for.
    pub(super) fn key(&self) -> Option<SearchKey> {
        match self {
            Self::Closed => None,
            Self::Open(field) => Some(field.key),
        }
    }

    pub(super) fn input(&self) -> Option<&Entity<InputState>> {
        match self {
            Self::Closed => None,
            Self::Open(field) => Some(&field.input),
        }
    }

    pub(super) fn field_mut(&mut self) -> Option<&mut SearchField> {
        match self {
            Self::Closed => None,
            Self::Open(field) => Some(field),
        }
    }

    /// A fresh, empty field for `key` (or none), replacing the old one.
    pub(super) fn reopen(
        &mut self,
        key: Option<SearchKey>,
        window: &mut Window,
        cx: &mut Context<QuillApp>,
    ) {
        *self = match key {
            None => Self::Closed,
            Some(key) => Self::Open(SearchField {
                key,
                input: Self::fresh_input(window, cx),
                debounce: None,
            }),
        };
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
                // A field since replaced can no longer be typed into, but its
                // last edit may still be on its way: only the current one
                // speaks for the gallery.
                if matches!(event, InputEvent::Change)
                    && this.media_search.input().map(Entity::entity_id) == Some(state.entity_id())
                {
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
