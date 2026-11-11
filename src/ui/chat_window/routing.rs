//! Which window shows a chat that is asked for, and what it lands on.
//!
//! Every chat open goes through [`QuillApp::route_chat_open`]: a chat with
//! a window of its own shows there (Telegram Desktop
//! `MainWidget::showHistoryInDifferentWindow`), a chat asked for inside a
//! chat window shows in the main window (a non-primary window hands its
//! navigation to the primary one), and the window's own chat stays where
//! it is. What the open lands on (the message to open at, a reply aimed
//! at the chat from another one, a forum topic, a resolved comment thread)
//! rides in the [`ChatOpen`] request, so it reaches the window that shows
//! the chat instead of staying in the state of the window that asked.
//! So does what the open hands over ([`ChatOpenCarry`]): messages to
//! forward, dropped files, text for the composer and the inline results
//! button's round trip. They are applied in the window that shows the
//! chat, never in the one that asked.

use super::super::app::QuillApp;
use super::super::inline_return::InlineReturn;
use crate::ui::status_note::StatusNote;
use gpui_kit::*;
use quill::composer::{ComposerReplyTo, ForwardDraft};
use quill::drop_modes::DropAction;
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::ThreadView;
use std::path::PathBuf;

/// What an open hands to the window that shows the chat.
#[derive(Debug, Default)]
pub(in crate::ui) struct ChatOpenCarry {
    /// Messages to forward, waiting in the forward bar (a drag dropped on
    /// a chat row, the Forward box's "continue in chat").
    pub(in crate::ui) forward: Option<ForwardDraft>,
    /// Files dropped on a chat row, attached in that chat; `true` sends a
    /// folder as one archive.
    pub(in crate::ui) files: Option<(Vec<PathBuf>, bool)>,
    /// Text the composer opens with (a link's `/start` prefill, the
    /// inline query typed back into its chat).
    pub(in crate::ui) compose: Option<String>,
    /// The inline results button's round trip, moved to the bot's window.
    pub(in crate::ui) inline_return: Option<InlineReturn>,
}

/// One request to show a chat, with what the view lands on.
#[derive(Debug)]
pub(in crate::ui) struct ChatOpen {
    pub(in crate::ui) chat_id: ChatId,
    /// A resolved comment thread moves into its discussion group (the
    /// driver keeps the thread) instead of selecting the chat from scratch.
    pub(in crate::ui) thread_switch: bool,
    /// That thread, taken from the chat window it resolved in: the main
    /// window's view takes it over.
    pub(in crate::ui) thread: Option<ThreadView>,
    /// The message the history opens at ("Go to message", a search hit).
    pub(in crate::ui) open_at: Option<MessageId>,
    /// A reply aimed at this chat from another one ("Reply in Another
    /// Chat").
    pub(in crate::ui) reply: Option<ComposerReplyTo>,
    /// The forum topic to open (a chat row's topic chip).
    pub(in crate::ui) topic: Option<i32>,
    /// What the open hands over.
    pub(in crate::ui) carry: ChatOpenCarry,
}

impl ChatOpen {
    pub(in crate::ui) fn new(chat_id: ChatId) -> Self {
        Self {
            chat_id,
            thread_switch: false,
            thread: None,
            open_at: None,
            reply: None,
            topic: None,
            carry: ChatOpenCarry::default(),
        }
    }

    pub(in crate::ui) fn forwarding(mut self, draft: ForwardDraft) -> Self {
        self.carry.forward = Some(draft);
        self
    }

    pub(in crate::ui) fn with_files(mut self, paths: Vec<PathBuf>, archive: bool) -> Self {
        self.carry.files = Some((paths, archive));
        self
    }

    pub(in crate::ui) fn compose(mut self, text: String) -> Self {
        self.carry.compose = Some(text);
        self
    }

    pub(in crate::ui) fn with_inline_return(mut self, ret: Option<InlineReturn>) -> Self {
        self.carry.inline_return = ret;
        self
    }

    pub(in crate::ui) fn at(mut self, message_id: MessageId) -> Self {
        self.open_at = Some(message_id);
        self
    }

    pub(in crate::ui) fn with_reply(mut self, reply: ComposerReplyTo) -> Self {
        self.reply = Some(reply);
        self
    }

    pub(in crate::ui) fn in_topic(mut self, topic: i32) -> Self {
        self.topic = Some(topic);
        self
    }

    pub(in crate::ui) fn thread_switch(mut self, thread: Option<ThreadView>) -> Self {
        self.thread_switch = true;
        self.thread = thread;
        self
    }
}

impl QuillApp {
    /// Send `open` to the window that shows its chat. `Some` gives the
    /// request back when it is for the window in place to open: the main
    /// window, for a chat with no window of its own. `None` when another
    /// window took it over, or the window in place already shows the chat
    /// and landed on what the request asks for.
    pub(in crate::ui) fn route_chat_open(
        &mut self,
        open: ChatOpen,
        cx: &mut Context<Self>,
    ) -> Option<ChatOpen> {
        let chat_id = open.chat_id;
        let here = self.windows.scope;
        if here == Some(chat_id) {
            // The window shows it already.
            self.land_chat_open(open, cx);
            return None;
        }
        if !open.thread_switch && self.routed_to_chat_window(chat_id, cx) {
            self.in_window_scope(Some(chat_id), cx, |app, cx| app.land_chat_open(open, cx));
            return None;
        }
        if here.is_none() {
            return Some(open);
        }
        let main = self.main_window.handle?;
        let owner = cx.entity();
        let screen = self.screen_epoch();
        // window-scope: `in_screen_scope` puts the main window's state in place.
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                owner.update(cx, |app, cx| {
                    app.in_screen_scope(screen, cx, |app, cx| {
                        app.select_chat_request(open, window, cx);
                    });
                });
                cx.activate(true);
                window.activate_window();
            });
        });
        None
    }

    /// The window in place shows `open`'s chat: go to its message, take
    /// its reply, open its topic.
    fn land_chat_open(&mut self, open: ChatOpen, cx: &mut Context<Self>) {
        if let Some(topic) = open.topic {
            self.select_topic_ui(topic, cx);
        }
        if let Some(reply) = open.reply {
            self.chat_ui.set_reply(Some(reply));
        }
        if let Some(message_id) = open.open_at {
            self.jump_to_pinned_message(message_id, cx);
        }
        self.apply_open_carry(open.chat_id, open.carry, None, cx);
        cx.notify();
    }

    /// The window in place shows `chat_id`: what the open handed over
    /// lands in it. Without a window at hand the composer text waits for
    /// the window's frame (`apply_compose_after_open`).
    pub(in crate::ui) fn apply_open_carry(
        &mut self,
        chat_id: ChatId,
        carry: ChatOpenCarry,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let ChatOpenCarry {
            forward,
            files,
            compose,
            inline_return,
        } = carry;
        if let Some(draft) = forward {
            self.show_forward_bar(chat_id, draft, cx);
        }
        if let Some((paths, archive)) = files {
            if archive {
                self.drop_on_zone(DropAction::Archive, &paths, cx);
            } else {
                self.drop_files_plain(&paths, cx);
            }
        }
        // The inline results button's start goes out right away; the
        // prefill is only for a start the user confirms.
        let started = match inline_return {
            Some(ret) => {
                self.composer_ui.inline_return = Some(ret);
                self.take_inline_start(chat_id, cx)
            }
            None => false,
        };
        if let Some(text) = compose.filter(|_| !started) {
            match window {
                Some(window) => self.set_composer_plain(&text, window, cx),
                None => self.chat_ui.compose_after_open = Some(text),
            }
        }
    }

    /// Once per window frame: text an open handed over without a window
    /// at hand goes into the composer, while the chat it was for shows.
    pub(in crate::ui) fn apply_compose_after_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = self.chat_ui.compose_after_open.take() {
            self.set_composer_plain(&text, window, cx);
        }
    }

    /// Plain text into the composer, replacing what is there.
    fn set_composer_plain(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_ui.markdown_revert = None;
        self.composer
            .update(cx, |input, cx| input.set_value(text, window, cx));
        cx.notify();
    }

    /// `draft` waits in the forward bar of `chat_id`, the window in place
    /// showing it: the state the Forward box leaves when one chat is
    /// chosen.
    pub(in crate::ui) fn show_forward_bar(
        &mut self,
        chat_id: ChatId,
        draft: ForwardDraft,
        cx: &mut Context<Self>,
    ) {
        self.share.forward = crate::ui::forward_stage::ForwardStage::Bar {
            draft,
            dest: chat_id,
        };
        self.share.selection.clear();
        self.set_note(StatusNote::silent(tr(
            keys::Q_OPS_FORWARD_BAR_READY_PRESS_SEND,
        )));
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "routing_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "routing_carry_tests.rs"]
mod carry_tests;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "inline_return_slot_tests.rs"]
mod inline_return_slot_tests;
