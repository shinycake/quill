//! The force-reply targets bots leave for the composers.
use crate::state::*;

/// The force-reply targets waiting for the window that shows their chat.
/// One per chat (a newer one replaces an older one of the same chat), so
/// two bots forcing replies in one batch both arm their composers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ForceReplies(Vec<ForceReplyTarget>);

impl ForceReplies {
    pub fn push(&mut self, target: ForceReplyTarget) {
        self.0.retain(|old| old.chat_id != target.chat_id);
        self.0.push(target);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn targets(&self) -> &[ForceReplyTarget] {
        &self.0
    }

    /// Take the targets `wanted` picks, leaving the rest.
    pub fn take(&mut self, wanted: impl Fn(ChatId) -> bool) -> Vec<ForceReplyTarget> {
        let (taken, kept) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|target| wanted(target.chat_id));
        self.0 = kept;
        taken
    }
}

impl Session {
    /// The force-reply targets one window takes: those for the chat it
    /// has open, and those for a chat no window shows (nothing will ever
    /// arm them). One for another window's chat waits for that window's
    /// frame.
    pub fn take_force_replies_for(&mut self, open_chat: Option<ChatId>) -> Vec<ForceReplyTarget> {
        let unshown: Vec<ChatId> = self
            .bots
            .pending_force_reply
            .targets()
            .iter()
            .map(|target| target.chat_id)
            .filter(|chat| open_chat != Some(*chat) && !self.chat_is_shown(*chat))
            .collect();
        self.bots
            .pending_force_reply
            .take(|chat| open_chat == Some(chat) || unshown.contains(&chat))
    }
}
