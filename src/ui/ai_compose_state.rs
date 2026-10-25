//! The AI compose box's UI state (`ui/ai_compose/`): the open box, the
//! sub-view it shows, the language search, the composer button's fade-in
//! and the custom emoji the style pills asked for.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::ai_compose::AiComposeBox;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use std::collections::HashSet;
use std::time::Instant;

/// What the box shows. tdesktop opens the language picker and the Cocoon
/// box as boxes of their own; Quill's dialogs never stack, so they are
/// views inside the one dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiComposeView {
    Card,
    Languages,
    About,
}

/// The open box and the chat whose composer it rewrites.
pub(crate) struct AiComposeOpen {
    pub(super) chat_id: ChatId,
    pub(super) ai: AiComposeBox,
    pub(super) view: AiComposeView,
    /// The style names the box last refreshed against.
    pub(super) style_names: Vec<String>,
}

pub(crate) struct AiComposeUi {
    pub(super) open: Option<AiComposeOpen>,
    /// The language picker's search field.
    pub(super) language_search: Entity<TextareaState>,
    /// When the composer's AI button appeared (its sparkle blinks from
    /// here); `None` while it is hidden.
    pub(super) button_shown_at: Option<Instant>,
    /// Custom emoji ids the style pills already asked TDLib for.
    pub(super) emoji_asked: HashSet<i64>,
}

impl AiComposeUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let language_search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_LANGUAGES))
                .auto_grow(1, 1)
        });
        cx.subscribe_in(
            &language_search,
            window,
            |_, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();
        Self {
            open: None,
            language_search,
            button_shown_at: None,
            emoji_asked: HashSet::new(),
        }
    }
}
