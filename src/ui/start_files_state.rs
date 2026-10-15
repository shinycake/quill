//! Files that arrived from outside the window (the Windows "Send to" menu,
//! `quill <file>`) and are waiting for the user to pick a chat.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::{SendKeyMode, should_send_on_enter};
use quill::lang::{keys, tr};
use std::path::PathBuf;

pub(crate) struct StartFilesUi {
    /// The files the "Forward to…" chooser is about (tdesktop
    /// `MainWidget::handleStartFiles`); empty while the chooser is closed.
    pub(super) choosing: Vec<PathBuf>,
    /// The chooser's "Search chats" field.
    pub(super) search: Entity<TextareaState>,
}

impl StartFilesUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let search = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_CHATS))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &search,
            window,
            |this, state, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let text = state.read(cx).value().to_string();
                    this.start_files_search_changed(&text, cx);
                }
                InputEvent::PressEnter { secondary, shift } => {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        SendKeyMode::Enter,
                    ) {
                        this.choose_first_start_files_chat(window, cx);
                    }
                }
                _ => {}
            },
        )
        .detach();
        Self {
            choosing: Vec::new(),
            search,
        }
    }
}
