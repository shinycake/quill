//! Which chats have an OS notification on screen, and which of those the UI
//! must withdraw: one stage per chat instead of a set of shown chats and a
//! list of chats to clear kept in step by hand.
use crate::ids::ChatId;
use std::collections::BTreeMap;

/// Where a chat's OS notification stands (no entry: none on screen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyShown {
    /// Shown (or TDLib reports it active from a previous launch): only
    /// these produce a clear.
    Shown,
    /// The chat was read elsewhere or TDLib removed its notifications; the
    /// UI withdraws the toast the next time it drains.
    ClearDue,
}

#[derive(Debug, Default)]
pub struct ShownNotifications(BTreeMap<ChatId, NotifyShown>);

impl ShownNotifications {
    /// A notification of `chat` is queued for the OS. A clear still due is
    /// replaced: the new toast takes the old one's place.
    pub fn mark_shown(&mut self, chat: ChatId) {
        self.0.insert(chat, NotifyShown::Shown);
    }

    /// Chats TDLib reports active from a previous launch.
    pub fn mark_active(&mut self, chats: impl IntoIterator<Item = ChatId>) {
        for chat in chats {
            self.0.entry(chat).or_insert(NotifyShown::Shown);
        }
    }

    pub fn is_shown(&self, chat: ChatId) -> bool {
        self.0.get(&chat) == Some(&NotifyShown::Shown)
    }

    /// The chat was read: its toast is withdrawn at the next drain. A chat
    /// with nothing shown (or already due) changes nothing.
    pub fn clear(&mut self, chat: ChatId) {
        if let Some(stage) = self.0.get_mut(&chat)
            && *stage == NotifyShown::Shown
        {
            *stage = NotifyShown::ClearDue;
        }
    }

    /// The chats whose toast is to be withdrawn, taken.
    pub fn take_clears(&mut self) -> Vec<ChatId> {
        let due: Vec<ChatId> = self
            .0
            .iter()
            .filter(|(_, stage)| **stage == NotifyShown::ClearDue)
            .map(|(chat, _)| *chat)
            .collect();
        for chat in &due {
            self.0.remove(chat);
        }
        due
    }

    /// The chats with a toast on screen, taken (the UI dismisses them all).
    pub fn take_shown(&mut self) -> Vec<ChatId> {
        let shown: Vec<ChatId> = self
            .0
            .iter()
            .filter(|(_, stage)| **stage == NotifyShown::Shown)
            .map(|(chat, _)| *chat)
            .collect();
        for chat in &shown {
            self.0.remove(chat);
        }
        shown
    }

    /// Whether a clear is due, for tests.
    #[cfg(test)]
    pub fn clears_due(&self) -> Vec<ChatId> {
        self.0
            .iter()
            .filter(|(_, stage)| **stage == NotifyShown::ClearDue)
            .map(|(chat, _)| *chat)
            .collect()
    }

    /// A clear that is due for `chat` regardless of what was shown, for
    /// tests of the drain.
    #[cfg(test)]
    pub fn force_clear(&mut self, chat: ChatId) {
        self.0.insert(chat, NotifyShown::ClearDue);
    }
}
