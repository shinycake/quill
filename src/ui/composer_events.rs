//! The compose field and what typing in it does: markdown, instant and
//! system text replacements, the typing indicator, the draft, the `/`,
//! `@`, `#` and `:emoji` menus, inline mode, spell check, sticker
//! suggestions, and Enter.
//! One wiring for the main window's composer and every chat window's.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::{SendKeyMode, should_send_on_enter};
use quill::lang::{keys, tr};

impl QuillApp {
    /// A compose field with Quill's placeholder, growth, send key and
    /// formatting, with [`Self::subscribe_composer_events`] attached.
    pub(super) fn new_composer_field(
        send_key_mode: SendKeyMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        // Send-key mode drives kit's newline-vs-submit behavior: plain
        // Enter submits only in Enter mode; in CtrlEnter mode it inserts
        // a newline and Ctrl/Cmd+Enter sends.
        let submit_on_enter = send_key_mode == SendKeyMode::Enter;
        let composer = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::MESSAGE_PH))
                .auto_grow(1, 8)
                .submit_on_enter(submit_on_enter);
            // Formatting shows in the field (codex:composer-input).
            state.set_span_styler(Some(super::composer_field::span_styler()), cx);
            state
        });
        Self::subscribe_composer_events(&composer, window, cx);
        composer
    }

    /// React to edits, focus and Enter in `composer`. The handlers read
    /// `self.composer` and the composer state, so they run with the
    /// field's window in place: an edit made inside another window's scope
    /// is delivered after that scope ends (`chat_window::slot`).
    pub(super) fn subscribe_composer_events(
        composer: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| {
                    this.on_composer_event(state, event, window, cx)
                });
            },
        )
        .detach();
    }

    /// An edit, focus change or Enter in a compose field, with the field's
    /// window in place (`in_window_of`).
    fn on_composer_event(
        &mut self,
        state: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut text = state.read(cx).value().to_string();
        if matches!(event, InputEvent::Change) {
            let prev = self.composer_ui.prev_text.clone();
            if self
                .composer_ui
                .markdown_revert
                .as_ref()
                .is_some_and(|revert| revert.text != text)
            {
                self.composer_ui.markdown_revert = None;
            }
            if let Some(replaced) = self.apply_instant_replace(&text, window, cx) {
                text = replaced;
            } else if self.apply_markdown_replacement(&prev, window, cx) {
                // `**bold**` typed: the markers became formatting.
                text = state.read(cx).value().to_string();
                self.composer_ui.prev_text = text.clone();
            } else {
                // A macOS text replacement shortcut ended by this keystroke.
                self.queue_system_text_replace(&prev, &text, window, cx);
            }
            self.sync_composer_typing(&text);
            self.close_stale_rich_paste_notice(&text, window, cx);
            self.note_open_draft(true, cx);
        }
        // Phase 3.3: the `/` menu tracks the composer text (Blur
        // dismisses it); Enter picks the highlighted command
        // instead of sending while the menu is open.
        match event {
            InputEvent::Blur => {
                self.close_command_menu(cx);
                self.close_inline_results(cx);
                self.close_mention_menu(cx);
                self.close_suggest_menu(false, cx);
            }
            _ => {
                self.sync_command_menu(cx);
                self.sync_mention_menu(cx);
                self.sync_suggest_menu(cx);
                self.sync_inline_mode(cx);
                // codex:spellcheck-native: shift underlines with
                // the edit and debounce a background re-check.
                self.sync_spellcheck(&text, cx);
                self.sync_composer_suggestions(&text, cx);
            }
        }
        if let InputEvent::PressEnter { secondary, shift } = event {
            let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
            if should_send_on_enter(
                quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                self.chat_prefs.send_key_mode,
            ) {
                if self.pick_inline_result_selection(window, cx) {
                    // Enter was consumed by the inline results.
                } else if self.pick_mention_selection(window, cx) {
                    // Enter completed the highlighted mention.
                } else if self.pick_suggest_selection(window, cx) {
                    // Enter inserted the highlighted hashtag/emoji.
                } else if self.pick_command_menu_selection(window, cx) {
                    // Enter was consumed by the open menu.
                } else if !text.trim().is_empty()
                    || !self.pending_attachments().is_empty()
                    || self.forward_bar_here()
                {
                    let markup = self.composer_markup(cx);
                    self.submit_composer(
                        quill::composer::send_text_on_enter(markup, self.chat_prefs.send_key_mode),
                        window,
                        cx,
                    );
                }
            }
        }
    }
}
