//! The round trip of the inline results button (tdesktop
//! `Inner::switchPm`, `botInfo->inlineReturnTo`): the button opens the bot's
//! chat and sends its start with the parameter; when the bot answers with a
//! switch-inline button, Quill goes back to the chat the query came from and
//! types `@bot query` there.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::*;
use quill::ids::{AccountKey, ChatId};
use quill::inline_grid::switch_inline_query;
use quill::telegram::envelope::ChatKind;

/// Where a round trip stands. Every stage ends: the bot's switch-inline
/// button takes it back, and a lookup that fails
/// ([`QuillApp::cancel_inline_opening`]) or another account on screen
/// drops it.
#[derive(Debug)]
enum InlineStage {
    /// The bot's chat is being looked up; its start waits to be sent.
    Opening { parameter: String },
    /// The start went out. Newest message id in the bot's chat at that
    /// point; only later messages can bring the user back.
    Started { baseline: i64 },
}

/// Where the inline query came from and what is still waiting.
#[derive(Debug)]
pub(in crate::ui) struct InlineReturn {
    account: AccountKey,
    origin: ChatId,
    bot_id: i64,
    username: String,
    stage: InlineStage,
}

impl InlineReturn {
    /// Whether the round trip waits to send `bot_id`'s start.
    pub(in crate::ui) fn waits_for_start(&self, bot_id: i64) -> bool {
        self.bot_id == bot_id && matches!(self.stage, InlineStage::Opening { .. })
    }

    fn started(&self) -> bool {
        matches!(self.stage, InlineStage::Started { .. })
    }
}

impl QuillApp {
    /// The round trip waiting for the start of the bot `chat_id` is the
    /// private chat with, from whichever window began it: the window that
    /// opens the bot's chat sends the start and later comes back.
    pub(super) fn take_waiting_inline_return(&mut self, chat_id: ChatId) -> Option<InlineReturn> {
        self.drop_foreign_inline_return();
        let bot_id = self
            .session()
            .and_then(|session| session.chats.get(&chat_id.0))
            .and_then(|chat| match &chat.kind {
                ChatKind::Private { user_id } => Some(user_id.0),
                _ => None,
            })?;
        let here = self
            .composer_ui
            .inline_return
            .as_ref()
            .is_some_and(|ret| ret.waits_for_start(bot_id));
        if here {
            return self.composer_ui.inline_return.take();
        }
        self.windows.take_slot_inline_return(bot_id)
    }

    /// Remember the chat to return to and have the bot's start sent once
    /// its chat is open (`take_inline_start`).
    pub(super) fn begin_inline_return(&mut self, bot_id: i64, username: &str, parameter: &str) {
        let Some(origin) = self.open_chat_id() else {
            return;
        };
        if self.is_private_chat_with(origin, bot_id) {
            return;
        }
        let Some(account) = self.session().map(|session| session.account.clone()) else {
            return;
        };
        self.composer_ui.inline_return = Some(InlineReturn {
            account,
            origin,
            bot_id,
            username: username.to_string(),
            stage: InlineStage::Opening {
                parameter: parameter.to_string(),
            },
        });
    }

    /// The bot's chat could not be opened (the lookup failed, or the link
    /// went elsewhere): the round trip waiting on it ends, so the start is
    /// never sent into a chat opened later by another route.
    pub(super) fn cancel_inline_opening(&mut self) {
        if self
            .composer_ui
            .inline_return
            .as_ref()
            .is_some_and(|ret| !ret.started())
        {
            self.composer_ui.inline_return = None;
        }
    }

    /// The round trip on screen's account, if it has one: a trip begun on
    /// another account is dropped.
    fn drop_foreign_inline_return(&mut self) {
        let here = self.session().map(|session| session.account.clone());
        if self
            .composer_ui
            .inline_return
            .as_ref()
            .is_some_and(|ret| here.as_ref() != Some(&ret.account))
        {
            self.composer_ui.inline_return = None;
        }
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
        self.drop_foreign_inline_return();
        let Some(bot_id) = self
            .composer_ui
            .inline_return
            .as_ref()
            .filter(|ret| !ret.started())
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
        let InlineStage::Opening { parameter } =
            std::mem::replace(&mut ret.stage, InlineStage::Started { baseline })
        else {
            return false;
        };
        self.press_bot_start(chat_id, bot_id, parameter, cx);
        true
    }

    /// Once per frame (needs the window for the composer): when the bot's
    /// answer carries a switch-inline button, go back and type the query.
    pub(super) fn flush_inline_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drop_foreign_inline_return();
        let Some(ret) = self.composer_ui.inline_return.as_ref() else {
            return;
        };
        let InlineStage::Started { baseline } = ret.stage else {
            return;
        };
        let Some(open) = self.open_chat_id() else {
            return;
        };
        if !self.is_private_chat_with(open, ret.bot_id) {
            return;
        }
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
        let waiting = self
            .composer_ui
            .inline_return
            .as_ref()
            .is_some_and(|ret| ret.started() && self.is_private_chat_with(chat_id, ret.bot_id));
        if waiting {
            self.return_to_inline_query(query, window, cx);
        }
        waiting
    }

    fn return_to_inline_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ret) = self.composer_ui.inline_return.take() else {
            return;
        };
        // The query rides with the open, to the window that shows the
        // chat it came from.
        let text = format!("@{} {}", ret.username, query);
        let open = super::chat_window::ChatOpen::new(ret.origin).compose(text);
        self.select_chat_request(open, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "inline_return_tests.rs"]
mod tests;
