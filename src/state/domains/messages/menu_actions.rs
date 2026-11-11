//! What `getMessageProperties` said each message may offer, by message.
//! The menu, a gallery row's hover and the media viewer each ask about a
//! message of their own; one shared slot let the last answer overwrite the
//! others', and a failed ask left the menu on its permissive local guess.
use crate::state::*;
use crate::telegram::envelope::MessageActions;

/// The most messages remembered; the oldest answer goes first.
const KEPT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Know {
    Allows(MessageActions),
    /// TDLib refused to say.
    Failed,
}

/// The answers by `(chat, message)`, oldest first.
#[derive(Debug, Default, Clone)]
pub struct MenuActions {
    known: Vec<((ChatId, MessageId), Know)>,
}

impl MenuActions {
    fn put(&mut self, chat_id: ChatId, message_id: MessageId, know: Know) {
        self.known.retain(|(key, _)| *key != (chat_id, message_id));
        self.known.push(((chat_id, message_id), know));
        if self.known.len() > KEPT {
            self.known.remove(0);
        }
    }

    fn find(&self, chat_id: ChatId, message_id: MessageId) -> Option<Know> {
        self.known
            .iter()
            .find(|(key, _)| *key == (chat_id, message_id))
            .map(|(_, know)| *know)
    }

    pub fn set(&mut self, chat_id: ChatId, message_id: MessageId, actions: MessageActions) {
        self.put(chat_id, message_id, Know::Allows(actions));
    }

    /// The ask for this message failed: its gated rows stay hidden.
    pub fn fail(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.put(chat_id, message_id, Know::Failed);
    }

    /// What TDLib allows for the message; `None` until it answers.
    pub fn get(&self, chat_id: ChatId, message_id: MessageId) -> Option<MessageActions> {
        match self.find(chat_id, message_id)? {
            Know::Allows(actions) => Some(actions),
            Know::Failed => None,
        }
    }

    /// Whether "delete for everyone" is open for the message: TDLib's
    /// `can_be_deleted_for_all_users` once it answered, `local` (your own
    /// message) until then, and no after a failed ask.
    pub fn allows_revoke(&self, chat_id: ChatId, message_id: MessageId, local: bool) -> bool {
        match self.find(chat_id, message_id) {
            Some(Know::Allows(actions)) => actions.can_be_deleted_for_all_users,
            Some(Know::Failed) => false,
            None => local,
        }
    }

    pub fn is_failed(&self, chat_id: ChatId, message_id: MessageId) -> bool {
        self.find(chat_id, message_id) == Some(Know::Failed)
    }

    /// The answer is out of date (the menu asks again).
    pub fn forget(&mut self, chat_id: ChatId, message_id: MessageId) {
        self.known.retain(|(key, _)| *key != (chat_id, message_id));
    }
}
