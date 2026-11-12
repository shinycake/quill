//! The scheduled sends of the chats asked for, and the `getChatScheduledMessages`
//! each chat waits for.
use crate::ids::{ChatId, RequestId};
use crate::state::PendingRequest;
use crate::telegram::envelope::ParsedMessage;
use std::collections::HashMap;

/// `getChatScheduledMessages` results: scheduled sends, with
/// `scheduling_state` showing the planned send time. Rows of every chat
/// asked for (read one chat's through `Session::scheduled_in`). A chat's
/// rows are replaced only by the answer of the ask it waits for, so an
/// older ask answering after a newer one began changes nothing.
#[derive(Debug, Default)]
pub struct ScheduledMessages {
    pub rows: Vec<ParsedMessage>,
    asks: HashMap<ChatId, RequestId>,
}

impl ScheduledMessages {
    /// `request` went out for `chat`: it is the ask the chat waits for.
    pub fn asked(&mut self, chat: ChatId, request: RequestId) {
        self.asks.insert(chat, request);
    }

    /// The answer of `pending`: the rows of its chat, when it is the ask the
    /// chat waits for. `false` (and nothing changes) for any other answer.
    pub fn answered(&mut self, pending: &PendingRequest, messages: &[ParsedMessage]) -> bool {
        let Some(chat) = pending.chat_id else {
            return false;
        };
        if self.asks.get(&chat) != Some(&pending.id) {
            return false;
        }
        self.asks.remove(&chat);
        self.rows.retain(|m| m.chat_id != chat);
        self.rows
            .extend(messages.iter().filter(|m| m.chat_id == chat).cloned());
        true
    }

    /// The ask `pending` failed: its chat no longer waits (the rows stay).
    pub fn failed(&mut self, pending: &PendingRequest) {
        if let Some(chat) = pending.chat_id
            && self.asks.get(&chat) == Some(&pending.id)
        {
            self.asks.remove(&chat);
        }
    }
}
