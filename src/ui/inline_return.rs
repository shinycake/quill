//! The round trip of the inline results button (tdesktop
//! `Inner::switchPm`, `botInfo->inlineReturnTo`): the button opens the bot's
//! chat and sends its start with the parameter; when the bot answers with a
//! switch-inline button, Quill goes back to the chat the query came from and
//! types `@bot query` there.

use super::QuillApp;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::inline_grid::switch_inline_query;
use quill::telegram::envelope::ChatKind;

/// Where the inline query came from and what is still waiting.
pub(super) struct InlineReturn {
    origin: ChatId,
    bot_id: i64,
    username: String,
    /// The start parameter, until the bot's chat is open and it was sent.
    pending_start: Option<String>,
    /// Newest message id in the bot's chat when the start went out; only
    /// later messages can bring the user back.
    baseline: i64,
}

impl QuillApp {
    /// Remember the chat to return to and have the bot's start sent once
    /// its chat is open (`take_inline_start`).
    pub(super) fn begin_inline_return(&mut self, bot_id: i64, username: &str, parameter: &str) {
        let Some(origin) = self.open_chat_id() else {
            return;
        };
        if self.is_private_chat_with(origin, bot_id) {
            return;
        }
        self.composer_ui.inline_return = Some(InlineReturn {
            origin,
            bot_id,
            username: username.to_string(),
            pending_start: Some(parameter.to_string()),
            baseline: 0,
        });
    }

    fn is_private_chat_with(&self, chat_id: ChatId, bot_id: i64) -> bool {
        self.session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .is_some_and(
                |chat| matches!(&chat.kind, ChatKind::Private { user_id } if user_id.0 == bot_id),
            )
    }

    /// The bot's chat just opened: send its start with the parameter
    /// (tdesktop `sendBotStartCommand` after `ShowAndStartBotMsgId`).
    /// Returns whether it did, so the caller skips its own start handling.
    pub(super) fn take_inline_start(&mut self, chat_id: ChatId, cx: &mut Context<Self>) -> bool {
        let Some(bot_id) = self
            .composer_ui
            .inline_return
            .as_ref()
            .filter(|ret| ret.pending_start.is_some())
            .map(|ret| ret.bot_id)
        else {
            return false;
        };
        if !self.is_private_chat_with(chat_id, bot_id) {
            return false;
        }
        let baseline = self
            .session()
            .and_then(|session| session.histories.get(&chat_id.0))
            .and_then(|history| history.messages.keys().max().copied())
            .unwrap_or(0);
        let Some(ret) = self.composer_ui.inline_return.as_mut() else {
            return false;
        };
        let parameter = ret.pending_start.take().unwrap_or_default();
        ret.baseline = baseline;
        self.press_bot_start(chat_id, bot_id, parameter, cx);
        true
    }

    /// Once per frame (needs the window for the composer): when the bot's
    /// answer carries a switch-inline button, go back and type the query.
    pub(super) fn flush_inline_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ret) = self.composer_ui.inline_return.as_ref() else {
            return;
        };
        if ret.pending_start.is_some() {
            return;
        }
        let Some(open) = self.open_chat_id() else {
            return;
        };
        if !self.is_private_chat_with(open, ret.bot_id) {
            return;
        }
        let baseline = ret.baseline;
        let query = self
            .session()
            .and_then(|session| session.histories.get(&open.0))
            .and_then(|history| {
                history
                    .messages
                    .iter()
                    .filter(|(id, message)| **id > baseline && !message.is_outgoing)
                    .find_map(|(_, message)| {
                        message.reply_markup.as_ref().and_then(switch_inline_query)
                    })
            });
        if let Some(query) = query {
            self.return_to_inline_query(&query, window, cx);
        }
    }

    /// A switch-inline button pressed in the bot's chat while a return is
    /// waiting goes back too (tdesktop's fast switch). Returns whether it
    /// did.
    pub(super) fn press_switch_inline_with_return(
        &mut self,
        chat_id: ChatId,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let waiting = self.composer_ui.inline_return.as_ref().is_some_and(|ret| {
            ret.pending_start.is_none() && self.is_private_chat_with(chat_id, ret.bot_id)
        });
        if waiting {
            self.return_to_inline_query(query, window, cx);
        }
        waiting
    }

    fn return_to_inline_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ret) = self.composer_ui.inline_return.take() else {
            return;
        };
        self.select_listed_chat(ret.origin, window, cx);
        let text = format!("@{} {}", ret.username, query);
        self.set_composer_markup(&text, window, cx);
        cx.notify();
    }
}
