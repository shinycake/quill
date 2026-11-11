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

use std::collections::{HashMap, VecDeque};

/// A message: chat id and message id.
pub type MessageKey = (i64, i64);

/// Messages remembered in each stage, at most. Past it the oldest is
/// forgotten; a forgotten message at worst plays or skips its effect once
/// more.
const MAX_KEYS: usize = 16_384;
/// The most messages held across both stages: what two sets of `MAX_KEYS`
/// held before they were one.
const MAX_HELD: usize = MAX_KEYS * 2;

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

/// Where a message's effect stands. A message is in one stage, never both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EffectStage {
    /// Its effect has not played here.
    Unwatched,
    /// Marked once: noting the message again never makes it unwatched.
    Watched,
}

/// The stage of each message, forgetting the oldest when full, so a busy
/// session never replays an effect it forgot or loses one that has not
/// played.
#[derive(Debug, Default)]
struct BoundedStages {
    /// Each key's stage with the ticket it was queued under.
    stages: HashMap<MessageKey, (EffectStage, u64)>,
    /// Insertion order, as `(key, ticket)`. An entry is live only while the
    /// map still holds that key under that ticket; a removed or re-queued
    /// key leaves a stale entry that is skipped, never evicting the live one.
    order: VecDeque<(MessageKey, u64)>,
    next_ticket: u64,
}

impl BoundedStages {
    fn set(&mut self, key: MessageKey, stage: EffectStage) {
        if let Some(held) = self.stages.get_mut(&key) {
            held.0 = stage;
            return;
        }
        let ticket = self.next_ticket;
        self.next_ticket = self.next_ticket.wrapping_add(1);
        self.stages.insert(key, (stage, ticket));
        self.order.push_back((key, ticket));
        while self.stages.len() > MAX_HELD {
            let Some((oldest, oldest_ticket)) = self.order.pop_front() else {
                break;
            };
            if self.stages.get(&oldest).map(|held| held.1) == Some(oldest_ticket) {
                self.stages.remove(&oldest);
            }
        }
        // Removed keys leave their place in `order`: shed them when they
        // outnumber the live ones.
        if self.order.len() > MAX_HELD * 2 {
            let stages = &self.stages;
            self.order
                .retain(|(key, ticket)| stages.get(key).map(|held| held.1) == Some(*ticket));
        }
    }

    fn get(&self, key: &MessageKey) -> Option<EffectStage> {
        self.stages.get(key).map(|held| held.0)
    }

    fn remove(&mut self, key: &MessageKey) -> Option<EffectStage> {
        self.stages.remove(key).map(|held| held.0)
    }
}

/// The effect bookkeeping for every message the session holds.
#[derive(Debug, Default)]
pub struct EffectWatch {
    messages: BoundedStages,
}

impl EffectWatch {
    /// A message sent with an effect was stored (again).
    pub fn note(&mut self, key: MessageKey, arrival: EffectArrival) {
        if arrival.starts_unwatched() && self.messages.get(&key) != Some(EffectStage::Watched) {
            self.messages.set(key, EffectStage::Unwatched);
        }
    }

    /// A message sent from here got its final id (or failed under a new
    /// one): the effect it has not played moves with it.
    pub fn renamed(&mut self, old: MessageKey, new: MessageKey) {
        if self.messages.get(&old) == Some(EffectStage::Unwatched) {
            self.messages.remove(&old);
            self.messages.set(new, EffectStage::Unwatched);
        }
    }

    pub fn is_unwatched(&self, key: MessageKey) -> bool {
        self.messages.get(&key) == Some(EffectStage::Unwatched)
    }

    /// tdesktop `markEffectWatched`: the message was shown. Returns
    /// whether its effect had not played, so this is its one automatic
    /// play.
    pub fn mark_watched(&mut self, key: MessageKey) -> bool {
        let was = self.messages.remove(&key) == Some(EffectStage::Unwatched);
        self.messages.set(key, EffectStage::Watched);
        was
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectArrival, EffectWatch, MAX_HELD, MAX_KEYS};

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
        assert!(watch.messages.stages.len() <= MAX_HELD);
    }

    #[test]
    fn a_full_set_forgets_the_oldest_message_only() {
        let mut watch = EffectWatch::default();
        for id in 0..MAX_KEYS as i64 {
            watch.note((2, id), EffectArrival::SentHere);
            watch.mark_watched((3, id));
        }
        // One more message: only the oldest goes.
        watch.note((2, -1), EffectArrival::SentHere);
        assert!(watch.is_unwatched((2, -1)));
        assert!(!watch.is_unwatched((2, 0)));
        assert!(watch.is_unwatched((2, 1)));
        // A message that played long ago but not the oldest stays watched
        // when a history page brings it back.
        watch.mark_watched((3, -1));
        watch.note((3, 5), EffectArrival::Incoming { unread: true });
        assert!(!watch.is_unwatched((3, 5)));
    }

    #[test]
    fn a_message_marked_after_it_was_noted_is_not_evicted_by_its_stale_queue_entry() {
        let mut watch = EffectWatch::default();
        watch.note((1, 0), EffectArrival::SentHere);
        for id in 1..MAX_HELD as i64 {
            watch.note((2, id), EffectArrival::SentHere);
        }
        // The first message plays: it is now the newest entry.
        watch.mark_watched((1, 0));
        watch.note((2, -1), EffectArrival::SentHere);
        // The oldest live message goes, not the one that just played.
        assert!(!watch.is_unwatched((2, 1)));
        watch.note((1, 0), EffectArrival::Incoming { unread: true });
        assert!(!watch.is_unwatched((1, 0)));
    }
}
