use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};

/// The composer's inline "Send Contact" panel (the attach menu's contact
/// chooser): the search over the address book.
pub struct ShareContentDialog {
    pub(crate) query: Entity<TextareaState>,
}

impl ShareContentDialog {
    pub(crate) fn contact(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        Self {
            query: cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(tr(keys::Q_OPS_SEARCH_CONTACTS))
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            }),
        }
    }
}
