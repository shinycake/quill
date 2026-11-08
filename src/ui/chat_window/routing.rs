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

use super::super::app::QuillApp;
use gpui_kit::*;
use quill::composer::ComposerReplyTo;
use quill::ids::{ChatId, MessageId};
use quill::state::ThreadView;

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
        }
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
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "routing_tests.rs"]
mod tests;
