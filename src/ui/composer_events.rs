//! The compose field and what typing in it does: markdown and instant
//! replacements, the typing indicator, the draft, the `/`, `@`, `#` and
//! `:emoji` menus, inline mode, spell check, sticker suggestions, and Enter.
//! One wiring for the main window's composer and every chat window's.

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::{SendKeyMode, should_send_on_enter};

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
                .placeholder("Write a message...")
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
    /// `self.composer` and the composer state, which are the field's own
    /// while its window handles the event (`chat_window::slot`).
    pub(super) fn subscribe_composer_events(
        composer: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            composer,
            window,
            |this, state, event: &InputEvent, window, cx| {
                let mut text = state.read(cx).value().to_string();
                if matches!(event, InputEvent::Change) {
                    let prev = this.composer_ui.prev_text.clone();
                    if this
                        .composer_ui
                        .markdown_revert
                        .as_ref()
                        .is_some_and(|revert| revert.text != text)
                    {
                        this.composer_ui.markdown_revert = None;
                    }
                    if let Some(replaced) = this.apply_instant_replace(&text, window, cx) {
                        text = replaced;
                    } else if this.apply_markdown_replacement(&prev, window, cx) {
                        // `**bold**` typed: the markers became formatting.
                        text = state.read(cx).value().to_string();
                        this.composer_ui.prev_text = text.clone();
                    }
                    this.sync_composer_typing(&text);
                    this.note_open_draft(true, cx);
                }
                // Phase 3.3: the `/` menu tracks the composer text (Blur
                // dismisses it); Enter picks the highlighted command
                // instead of sending while the menu is open.
                match event {
                    InputEvent::Blur => {
                        this.close_command_menu(cx);
                        this.close_inline_results(cx);
                        this.close_mention_menu(cx);
                        this.close_suggest_menu(false, cx);
                    }
                    _ => {
                        this.sync_command_menu(cx);
                        this.sync_mention_menu(cx);
                        this.sync_suggest_menu(cx);
                        this.sync_inline_mode(cx);
                        // codex:spellcheck-native: shift underlines with
                        // the edit and debounce a background re-check.
                        this.sync_spellcheck(&text, cx);
                        this.sync_sticker_suggestions(&text, cx);
                        this.sync_animated_emoji_suggestion(&text, cx);
                    }
                }
                if let InputEvent::PressEnter { secondary, shift } = event {
                    let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
                    if should_send_on_enter(
                        quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                        this.chat_prefs.send_key_mode,
                    ) {
                        if this.pick_inline_result_selection(window, cx) {
                            // Enter was consumed by the inline results.
                        } else if this.pick_mention_selection(window, cx) {
                            // Enter completed the highlighted mention.
                        } else if this.pick_suggest_selection(window, cx) {
                            // Enter inserted the highlighted hashtag/emoji.
                        } else if this.pick_command_menu_selection(window, cx) {
                            // Enter was consumed by the open menu.
                        } else if !text.trim().is_empty()
                            || !this.composer_ui.pending_attachments.is_empty()
                            || this.forward_bar_here()
                        {
                            let markup = this.composer_markup(cx);
                            this.submit_composer(
                                quill::composer::send_text_on_enter(
                                    markup,
                                    this.chat_prefs.send_key_mode,
                                ),
                                window,
                                cx,
                            );
                        }
                    }
                }
            },
        )
        .detach();
    }
}
