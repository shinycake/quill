//! Which messages sent with an effect have not played it here yet
//! (tdesktop `MessageFlag::EffectWatched`, `HistoryItem::hasUnwatchedEffect`,
//! `applyInitialEffectWatched` and `markEffectWatched`).
//!
//! tdesktop decides once, when the message reaches the client: a message
//! sent from this client and an incoming message that is still unread have
//! an unwatched effect; an outgoing message from the server (sent from
//! another device) and a read incoming one have watched it. The flag stays
//! with the message: reading it on another device does not mark it, and a
//! message sent from here keeps it when it gets its final id. It clears the
//! first time the message is shown while the window is active, whether or
//! not the effect then plays (power saving can skip it).
#![deny(clippy::indexing_slicing)]

use std::collections::HashSet;

/// A message: chat id and message id.
pub type MessageKey = (i64, i64);

/// Messages remembered in each set, at most. Past it the set starts over;
/// a forgotten message at worst plays or skips its effect once more.
const MAX_KEYS: usize = 16_384;

/// How a message with an effect reached the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectArrival {
    /// Sent from this client, still on its way or just failed.
    SentHere,
    /// Outgoing, from the server: sent from another device or already
    /// delivered when it was loaded.
    SentElsewhere,
    /// Incoming; `unread` when its id is past the chat's read marker.
    Incoming { unread: bool },
}

impl EffectArrival {
    fn starts_unwatched(self) -> bool {
        match self {
            Self::SentHere => true,
            Self::SentElsewhere => false,
            Self::Incoming { unread } => unread,
        }
    }
}

#[derive(Debug, Default)]
struct BoundedKeys(HashSet<MessageKey>);

impl BoundedKeys {
    fn insert(&mut self, key: MessageKey) {
        if self.0.len() >= MAX_KEYS {
            self.0.clear();
        }
        self.0.insert(key);
    }
}

/// The effect bookkeeping for every message the session holds.
#[derive(Debug, Default)]
pub struct EffectWatch {
    unwatched: BoundedKeys,
    /// Marked once: noting the message again never makes it unwatched.
    watched: BoundedKeys,
}

impl EffectWatch {
    /// A message sent with an effect was stored (again).
    pub fn note(&mut self, key: MessageKey, arrival: EffectArrival) {
        if arrival.starts_unwatched() && !self.watched.0.contains(&key) {
            self.unwatched.insert(key);
        }
    }

    /// A message sent from here got its final id (or failed under a new
    /// one): the effect it has not played moves with it.
    pub fn renamed(&mut self, old: MessageKey, new: MessageKey) {
        if self.unwatched.0.remove(&old) {
            self.unwatched.insert(new);
        }
    }

    pub fn is_unwatched(&self, key: MessageKey) -> bool {
        self.unwatched.0.contains(&key)
    }

    /// tdesktop `markEffectWatched`: the message was shown. Returns
    /// whether its effect had not played, so this is its one automatic
    /// play.
    pub fn mark_watched(&mut self, key: MessageKey) -> bool {
        let was = self.unwatched.0.remove(&key);
        self.watched.insert(key);
        was
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectArrival, EffectWatch, MAX_KEYS};

    #[test]
    fn only_messages_sent_here_and_unread_incoming_ones_start_unwatched() {
        let mut watch = EffectWatch::default();
        watch.note((1, 10), EffectArrival::SentHere);
        watch.note((1, 11), EffectArrival::SentElsewhere);
        watch.note((1, 12), EffectArrival::Incoming { unread: true });
        watch.note((1, 13), EffectArrival::Incoming { unread: false });
        assert!(watch.is_unwatched((1, 10)));
        assert!(!watch.is_unwatched((1, 11)));
        assert!(watch.is_unwatched((1, 12)));
        assert!(!watch.is_unwatched((1, 13)));
    }

    #[test]
    fn an_effect_plays_by_itself_once() {
        let mut watch = EffectWatch::default();
        watch.note((1, 12), EffectArrival::Incoming { unread: true });
        assert!(watch.mark_watched((1, 12)));
        // Shown again (scrolled back, chat reopened): not again.
        assert!(!watch.mark_watched((1, 12)));
        assert!(!watch.is_unwatched((1, 12)));
    }

    #[test]
    fn a_message_stored_again_after_it_played_stays_watched() {
        let mut watch = EffectWatch::default();
        watch.note((1, 12), EffectArrival::Incoming { unread: true });
        watch.mark_watched((1, 12));
        // A history page brings it back before the read marker moved.
        watch.note((1, 12), EffectArrival::Incoming { unread: true });
        assert!(!watch.is_unwatched((1, 12)));
    }

    #[test]
    fn reading_elsewhere_does_not_mark_the_effect_watched() {
        let mut watch = EffectWatch::default();
        watch.note((1, 12), EffectArrival::Incoming { unread: true });
        // The read marker moved past it on another device; a refresh of
        // the message now notes it as read.
        watch.note((1, 12), EffectArrival::Incoming { unread: false });
        assert!(watch.is_unwatched((1, 12)));
    }

    #[test]
    fn a_sent_message_keeps_its_effect_under_its_final_id() {
        let mut watch = EffectWatch::default();
        watch.note((1, -5), EffectArrival::SentHere);
        // The update with the final id arrives as sent.
        watch.note((1, 300), EffectArrival::SentElsewhere);
        watch.renamed((1, -5), (1, 300));
        assert!(!watch.is_unwatched((1, -5)));
        assert!(watch.is_unwatched((1, 300)));
        // A message that already played has nothing to carry.
        watch.mark_watched((1, 300));
        watch.renamed((1, 300), (1, 301));
        assert!(!watch.is_unwatched((1, 301)));
    }

    #[test]
    fn the_sets_are_bounded() {
        let mut watch = EffectWatch::default();
        for id in 0..=MAX_KEYS as i64 {
            watch.note((2, id), EffectArrival::SentHere);
            watch.mark_watched((3, id));
        }
        assert!(watch.unwatched.0.len() <= MAX_KEYS);
        assert!(watch.watched.0.len() <= MAX_KEYS);
    }
}
