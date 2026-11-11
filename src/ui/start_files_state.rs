//! Files that arrived from outside the window (the Windows "Send to" menu,
//! `quill <file>`) and are waiting for the user to pick a chat.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::{SendKeyMode, should_send_on_enter};
use quill::lang::{keys, tr};
use std::path::PathBuf;

/// The "Forward to…" chooser (tdesktop `MainWidget::handleStartFiles`):
/// closed, or open over the files it is about.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) enum StartFiles {
    #[default]
    Idle,
    Choosing {
        files: Vec<PathBuf>,
    },
}

impl StartFiles {
    pub(super) fn is_choosing(&self) -> bool {
        matches!(self, Self::Choosing { .. })
    }

    /// The files the chooser is about.
    #[cfg(test)]
    pub(super) fn files(&self) -> &[PathBuf] {
        match self {
            Self::Idle => &[],
            Self::Choosing { files } => files,
        }
    }

    /// Open the chooser for `new`; files already being chosen for stay.
    pub(super) fn add(&mut self, new: Vec<PathBuf>) {
        let mut files = std::mem::take(self).into_files();
        for file in new {
            if !files.contains(&file) {
                files.push(file);
            }
        }
        *self = Self::Choosing { files };
    }

    /// Close the chooser, handing back its files.
    pub(super) fn take(&mut self) -> Vec<PathBuf> {
        std::mem::take(self).into_files()
    }

    fn into_files(self) -> Vec<PathBuf> {
        match self {
            Self::Idle => Vec::new(),
            Self::Choosing { files } => files,
        }
    }
}

pub(crate) struct StartFilesUi {
    pub(super) choosing: StartFiles,
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
            |this, state, event: &InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| match event {
                    InputEvent::Change => {
                        let text = state.read(cx).value().to_string();
                        this.start_files_search_changed(&text, cx);
                    }
                    InputEvent::PressEnter { secondary, shift } => {
                        let marked =
                            state.update(cx, |input, cx| input.marked_text_range(window, cx));
                        if should_send_on_enter(
                            quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                            SendKeyMode::Enter,
                        ) {
                            this.choose_first_start_files_chat(window, cx);
                        }
                    }
                    _ => {}
                });
            },
        )
        .detach();
        Self {
            choosing: StartFiles::Idle,
            search,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StartFiles;
    use std::path::PathBuf;

    #[test]
    fn files_wait_once_and_leave_with_the_chooser() {
        let mut chooser = StartFiles::default();
        assert!(!chooser.is_choosing());
        let a = PathBuf::from("a.txt");
        chooser.add(vec![a.clone(), a.clone()]);
        chooser.add(vec![PathBuf::from("b.txt"), a.clone()]);
        assert!(chooser.is_choosing());
        assert_eq!(chooser.take(), vec![a, PathBuf::from("b.txt")]);
        assert!(!chooser.is_choosing(), "taking the files closes it");
        assert!(chooser.take().is_empty());
    }
}
