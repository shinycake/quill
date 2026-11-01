//! Memo for [`super::badge_count`]: the window title asks every root render,
//! and the count walks every chat and forum topic. The answer only changes
//! when the session applies an update that feeds it
//! (`ChatListState::unread_revision`), the badge preferences change, or
//! another account is shown.
#![deny(clippy::indexing_slicing)]

use super::badge_count;
use crate::ids::{AccountGeneration, AccountKey};
use crate::settings::BadgePrefs;
use crate::state::Session;

/// What an unread count memo is keyed on. The account comes from the
/// session itself: a switch moves the shown account's session into the same
/// slot, so its address says nothing about which account it is.
#[derive(Debug, PartialEq, Eq)]
pub struct SessionCountKey {
    account: AccountKey,
    generation: AccountGeneration,
    unread_revision: u64,
    chats: usize,
    prefs: BadgePrefs,
}

impl SessionCountKey {
    /// Whether `key` still describes `session` counted with `prefs`.
    pub fn holds(key: Option<&Self>, session: &Session, prefs: &BadgePrefs) -> bool {
        key.is_some_and(|key| {
            key.unread_revision == session.chat_list.unread_revision
                && key.chats == session.chats.len()
                && key.prefs == *prefs
                && key.generation == session.account_generation
                && key.account == session.account
        })
    }

    pub fn of(session: &Session, prefs: &BadgePrefs) -> Self {
        Self {
            account: session.account.clone(),
            generation: session.account_generation,
            unread_revision: session.chat_list.unread_revision,
            chats: session.chats.len(),
            prefs: *prefs,
        }
    }
}

#[derive(Debug, Default)]
pub struct BadgeCache {
    key: Option<SessionCountKey>,
    count: u32,
}

impl BadgeCache {
    /// [`badge_count`], recomputed only when the account, the unread
    /// revision, the chat count or the preferences changed.
    pub fn get(&mut self, session: &Session, prefs: &BadgePrefs) -> u32 {
        if !SessionCountKey::holds(self.key.as_ref(), session, prefs) {
            self.count = badge_count(session, prefs);
            self.key = Some(SessionCountKey::of(session, prefs));
        }
        self.count
    }
}
