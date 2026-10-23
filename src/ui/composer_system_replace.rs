//! The composer side of macOS text replacements
//! (`quill::system_text_replace`): after a typed space or punctuation the
//! text before it is checked on a background thread, and a match is
//! swapped for its phrase while the text it was asked about is unchanged,
//! as one undo step. Other platforms have no system replacement list.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::*;

impl QuillApp {
    /// Ask the system about the text before the character just typed.
    /// `prev` is the composer text before the keystroke, `text` after it.
    #[cfg(target_os = "macos")]
    pub(super) fn queue_system_text_replace(
        &mut self,
        prev: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use quill::system_text_replace::check_for;
        if !self.chat_prefs.system_text_replace {
            return;
        }
        let range = self.composer.read(cx).selected_range();
        if !range.is_empty() || self.composer_caret_in_code(range.start, cx) {
            return;
        }
        let Some(check) = check_for(prev, text, range.start) else {
            return;
        };
        self.spawn_scoped_in(window, cx, async move |this, cx| {
            let asked = check.text.clone();
            let found = cx
                .background_spawn(
                    async move { super::spellcheck_mac::find_system_text_replace(&asked) },
                )
                .await;
            if let Some(found) = found {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.apply_system_text_replace(&check, &found, window, cx);
                });
            }
        })
        .detach();
    }

    #[cfg(not(target_os = "macos"))]
    pub(super) fn queue_system_text_replace(
        &mut self,
        _prev: &str,
        _text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// Swap the shortcut for its phrase, keeping the caret where the user
    /// left it (tdesktop `commitInstantReplacement`).
    #[cfg(target_os = "macos")]
    fn apply_system_text_replace(
        &mut self,
        check: &quill::system_text_replace::Check,
        found: &quill::system_text_replace::Found,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chat_prefs.system_text_replace {
            return;
        }
        let text = self.composer.read(cx).value().to_string();
        let Some(edit) = quill::system_text_replace::edit_for(&text, check, found) else {
            return;
        };
        let selection = self.composer.read(cx).selected_range();
        let shift = |at: usize| {
            if at >= edit.range.end {
                at - edit.range.len() + edit.with.len()
            } else {
                at.min(edit.range.start)
            }
        };
        let caret = shift(selection.start)..shift(selection.end);
        self.replace_composer_text(edit.range.clone(), &edit.with, window, cx);
        self.composer
            .update(cx, |input, cx| input.set_selected_range(caret, cx));
        self.composer_ui.prev_text = self.composer.read(cx).value().to_string();
    }
}
