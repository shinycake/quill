//! The chat's bot reply keyboard as TDLib keeps it (`chat.reply_markup_message_id`,
//! `updateChatReplyMarkup`), so the keyboard shows even when its message is
//! outside the loaded history window, plus the recent inline bots offered
//! when the user types "@".
use super::*;
use crate::telegram::envelope::{ReplyKeyboard, ReplyMarkup};

/// A keyboard TDLib says the chat shows.
pub type ChatKeyboard = Option<(MessageId, ReplyKeyboard)>;

#[derive(Debug, Default)]
pub struct ReplyKeyboardState {
    /// Chats TDLib reported a keyboard state for (`Some(None)` = removed).
    pub by_chat: HashMap<i64, ChatKeyboard>,
    /// `chat.reply_markup_message_id` per chat, for chats whose keyboard
    /// message may need fetching.
    pub markup_message_ids: HashMap<i64, i64>,
    /// `getRecentInlineBots` answer (`None` until fetched).
    pub recent_inline_bots: Option<Vec<i64>>,
}

impl Session {
    /// Record a chat's keyboard from `updateChatReplyMarkup` or a fetched
    /// reply-markup message. Only `replyMarkupShowKeyboard` is a keyboard
    /// (force-reply is handled with the history rows).
    pub(crate) fn set_chat_reply_keyboard(
        &mut self,
        chat_id: ChatId,
        message_id: Option<MessageId>,
        markup: Option<ReplyMarkup>,
    ) {
        let keyboard = match (message_id, markup) {
            (Some(id), Some(ReplyMarkup::ShowKeyboard(keyboard))) => Some((id, keyboard)),
            _ => None,
        };
        self.bots
            .reply_keyboards
            .by_chat
            .insert(chat_id.0, keyboard);
    }

    /// What TDLib last said about the chat's keyboard: `None` when it said
    /// nothing (fall back to the loaded messages), `Some(None)` for a removed
    /// keyboard.
    pub fn chat_reply_keyboard(&self, chat_id: ChatId) -> Option<&ChatKeyboard> {
        self.bots.reply_keyboards.by_chat.get(&chat_id.0)
    }

    /// The keyboard to show in a chat: TDLib's own answer when it gave one
    /// (so a keyboard whose message is outside the loaded window still
    /// shows), otherwise the newest keyboard among the loaded messages.
    /// One-time keyboards the user already used are in `dismissed`.
    pub fn custom_keyboard_for_chat(
        &self,
        chat_id: ChatId,
        dismissed: &std::collections::HashSet<(i64, i64)>,
    ) -> Option<(ChatId, MessageId, ReplyKeyboard)> {
        match self.chat_reply_keyboard(chat_id) {
            Some(entry) => entry
                .as_ref()
                .filter(|(message_id, _)| !dismissed.contains(&(chat_id.0, message_id.0)))
                .map(|(message_id, keyboard)| (chat_id, *message_id, keyboard.clone())),
            None => self
                .histories
                .get(&chat_id.0)
                .and_then(|history| active_custom_keyboard(&history.messages, dismissed)),
        }
    }

    /// The message id to fetch for the open chat's keyboard: TDLib named a
    /// reply-markup message, nothing was reported yet, and the message is
    /// not in the loaded history.
    pub fn reply_markup_message_to_fetch(&self, chat_id: ChatId) -> Option<MessageId> {
        let id = *self
            .bots
            .reply_keyboards
            .markup_message_ids
            .get(&chat_id.0)?;
        if id <= 0 || self.bots.reply_keyboards.by_chat.contains_key(&chat_id.0) {
            return None;
        }
        let loaded = self
            .histories
            .get(&chat_id.0)
            .is_some_and(|history| history.messages.contains_key(&id));
        (!loaded).then_some(MessageId(id))
    }

    /// Recent inline bots from `getRecentInlineBots`, newest first.
    pub fn recent_inline_bots(&self) -> &[i64] {
        self.bots
            .reply_keyboards
            .recent_inline_bots
            .as_deref()
            .unwrap_or(&[])
    }
}
