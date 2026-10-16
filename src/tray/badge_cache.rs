//! Memo for [`super::badge_count`]: the window title asks every root render,
//! and the count walks every chat and forum topic. The answer only changes
//! when the session applies an update (`Session::revision`), the badge
//! preferences change, or another session is shown.
#![deny(clippy::indexing_slicing)]

use super::badge_count;
use crate::settings::BadgePrefs;
use crate::state::Session;

#[derive(Debug, Default)]
pub struct BadgeCache {
    key: Option<(usize, u64, usize, BadgePrefs)>,
    count: u32,
}

impl BadgeCache {
    /// [`badge_count`], recomputed only when the session revision, the chat
    /// count, the preferences or the session itself changed.
    pub fn get(&mut self, session: &Session, prefs: &BadgePrefs) -> u32 {
        let key = (
            std::ptr::from_ref(session) as usize,
            session.revision,
            session.chats.len(),
            *prefs,
        );
        if self.key.as_ref() != Some(&key) {
            self.count = badge_count(session, prefs);
            self.key = Some(key);
        }
        self.count
    }
}
