//! The search-in-chat and forward-picker text fields and what typing in
//! them does. One wiring for the main window's fields and every chat
//! window's own (`chat_window::slot`).
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::*;
use quill::composer::{SendKeyMode, should_send_on_enter};
use quill::lang::{keys, tr};

impl QuillApp {
    /// The "Search in chat" field: its text drives the chat search, Enter
    /// jumps to the selected hit.
    pub(super) fn new_chat_search_field(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_IN_CHAT))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| {
                    this.on_chat_search_field_event(state, event, window, cx)
                });
            },
        )
        .detach();
        input
    }

    /// The "Search in chat" field changed or took Enter, with its window in
    /// place (`in_window_of`).
    fn on_chat_search_field_event(
        &mut self,
        state: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = state.read(cx).value().to_string();
        if matches!(event, InputEvent::Change) {
            self.sync_chat_search_query(&text, cx);
        }
        if let InputEvent::PressEnter { secondary, shift } = event {
            let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
            if *secondary && !*shift && marked.is_none() && !self.share.reply_elsewhere_open {
                self.submit_share_from_search(cx);
            } else if should_send_on_enter(
                quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                // Not a chat composer: the send-key setting does not apply.
                SendKeyMode::Enter,
            ) {
                self.jump_selected_chat_search_hit(cx);
            }
        }
    }

    /// The share box's "Search chats" field: its text filters the
    /// destinations, Enter picks the first one (or the first chat for
    /// "Reply in Another Chat").
    pub(super) fn new_forward_search_field(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextareaState> {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_MISC_SEARCH_CHATS))
                .auto_grow(1, 1)
                .submit_on_enter(true)
        });
        cx.subscribe_in(
            &input,
            window,
            |this, state, event: &InputEvent, window, cx| {
                this.in_window_of(window, cx, |this, window, cx| {
                    this.on_forward_search_field_event(state, event, window, cx)
                });
            },
        )
        .detach();
        input
    }

    /// The share box's search field changed or took Enter, with its window
    /// in place (`in_window_of`).
    fn on_forward_search_field_event(
        &mut self,
        state: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, InputEvent::Change) {
            let text = state.read(cx).value().to_string();
            self.sync_share_search(&text, cx);
        }
        if let InputEvent::PressEnter { secondary, shift } = event {
            let marked = state.update(cx, |input, cx| input.marked_text_range(window, cx));
            if *secondary && !*shift && marked.is_none() && !self.share.reply_elsewhere_open {
                self.submit_share_from_search(cx);
            } else if should_send_on_enter(
                quill::composer::enter_event_from_kit(*shift, *secondary, marked),
                // Not a chat composer: the send-key setting does not apply.
                SendKeyMode::Enter,
            ) {
                if self.share.reply_elsewhere_open {
                    self.choose_first_reply_chat(window, cx);
                } else {
                    self.activate_first_forward_destination(cx);
                }
            }
        }
    }
}
