//! The chat's bot reply keyboard as TDLib keeps it (`chat.reply_markup_message_id`,
//! `updateChatReplyMarkup`), so the keyboard shows even when its message is
//! outside the loaded history window, plus the recent inline bots offered
//! when the user types "@".
use super::*;
use crate::telegram::envelope::{ReplyKeyboard, ReplyMarkup};

/// A keyboard TDLib says the chat shows.
pub type ChatKeyboard = Option<(MessageId, ReplyKeyboard)>;

/// Where a chat's keyboard stands.
#[derive(Debug, Clone)]
pub enum KeyboardFetch {
    /// `chat.reply_markup_message_id` names a message to fetch.
    Wanted(MessageId),
    /// `getMessage` for it went out as `request`.
    Loading {
        request: RequestId,
        message: MessageId,
    },
    /// The fetch failed; it is not asked again until TDLib names a message.
    Failed(MessageId),
    /// TDLib's answer (`None` = the keyboard was removed).
    Known(ChatKeyboard),
}

#[derive(Debug, Default)]
pub struct ReplyKeyboardState {
    /// Per chat, what TDLib said about the keyboard and whether its
    /// message is being fetched.
    pub by_chat: HashMap<i64, KeyboardFetch>,
    /// `getRecentInlineBots`: asked when the user types "@"; a failed ask is
    /// asked again by the next "@" (an event of the composer, never a frame).
    pub recent_inline_bots: crate::state::fetch::Fetch<Vec<i64>>,
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
            .insert(chat_id.0, KeyboardFetch::Known(keyboard));
    }

    /// `chat.reply_markup_message_id` from `updateNewChat`: the message to
    /// fetch (the newest name wins), unless TDLib already told the keyboard
    /// or the fetch runs.
    pub(crate) fn note_reply_markup_message(&mut self, chat_id: ChatId, message: MessageId) {
        self.bots
            .reply_keyboards
            .by_chat
            .entry(chat_id.0)
            .and_modify(|stage| {
                // A newer name replaces one not yet fetched; a fetch out
                // finishes first, and a known answer stands.
                if matches!(stage, KeyboardFetch::Failed(_) | KeyboardFetch::Wanted(_)) {
                    *stage = KeyboardFetch::Wanted(message);
                }
            })
            .or_insert(KeyboardFetch::Wanted(message));
    }

    /// The fetch for the chat's keyboard message went out as `request`.
    pub(crate) fn begin_reply_markup_fetch(&mut self, chat_id: ChatId, request: RequestId) {
        if let Some(stage) = self.bots.reply_keyboards.by_chat.get_mut(&chat_id.0)
            && let KeyboardFetch::Wanted(message) = *stage
        {
            *stage = KeyboardFetch::Loading { request, message };
        }
    }

    /// The fetch `request` failed or never left: the chat falls back to its
    /// loaded messages and does not ask again.
    pub(crate) fn fail_reply_markup_fetch(&mut self, chat_id: ChatId, request: RequestId) {
        if let Some(stage) = self.bots.reply_keyboards.by_chat.get_mut(&chat_id.0)
            && let KeyboardFetch::Loading {
                request: sent,
                message,
            } = *stage
            && sent == request
        {
            *stage = KeyboardFetch::Failed(message);
        }
    }

    /// Whether `request` is the keyboard fetch the chat waits for.
    pub(crate) fn awaits_reply_markup(&self, chat_id: ChatId, request: RequestId) -> bool {
        matches!(
            self.bots.reply_keyboards.by_chat.get(&chat_id.0),
            Some(KeyboardFetch::Loading { request: sent, .. }) if *sent == request
        )
    }

    /// What TDLib last said about the chat's keyboard: `None` when it said
    /// nothing (fall back to the loaded messages), `Some(None)` for a removed
    /// keyboard.
    pub fn chat_reply_keyboard(&self, chat_id: ChatId) -> Option<&ChatKeyboard> {
        match self.bots.reply_keyboards.by_chat.get(&chat_id.0) {
            Some(KeyboardFetch::Known(keyboard)) => Some(keyboard),
            _ => None,
        }
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
        let Some(KeyboardFetch::Wanted(MessageId(id))) =
            self.bots.reply_keyboards.by_chat.get(&chat_id.0)
        else {
            return None;
        };
        let id = *id;
        if id <= 0 {
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
            .data()
            .map_or(&[], Vec::as_slice)
    }
}
