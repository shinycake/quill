//! Where each closed history stands in the retention policy
//! ([`super::history_prune`]): waiting for the UI's claims to settle, or a
//! kept tail ranked by use. One value per key, so a chat is never both
//! queued and ranked, and the order lives in the value rather than in two
//! lists kept in step.
use std::collections::BTreeMap;

/// One closed history's place in the retention policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Retention {
    /// The history stopped being shown; its trim waits for the UI's claims.
    /// `queued` orders the wait (the order the views closed).
    Releasing { queued: u64 },
    /// A tail is kept; `used` is when it was last used (higher = newer).
    Closed { used: u64 },
}

/// The retention of every closed key (a chat id, or a chat's topic).
#[derive(Debug)]
pub(super) struct Retained<K: Ord + Copy> {
    keys: BTreeMap<K, Retention>,
    tick: u64,
}

impl<K: Ord + Copy> Default for Retained<K> {
    fn default() -> Self {
        Self {
            keys: BTreeMap::new(),
            tick: 0,
        }
    }
}

impl<K: Ord + Copy> Retained<K> {
    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    /// `key` stopped being shown: its trim waits. A key already waiting
    /// moves to the back of the queue.
    pub fn queue(&mut self, key: K) {
        let queued = self.next_tick();
        self.keys.insert(key, Retention::Releasing { queued });
    }

    /// `key` was used: it is the newest kept tail. A key still waiting for
    /// its release stays waiting.
    pub fn touch(&mut self, key: K) {
        if matches!(self.keys.get(&key), Some(Retention::Releasing { .. })) {
            return;
        }
        let used = self.next_tick();
        self.keys.insert(key, Retention::Closed { used });
    }

    /// `key` waits for its release.
    pub fn is_releasing(&self, key: &K) -> bool {
        matches!(self.keys.get(key), Some(Retention::Releasing { .. }))
    }

    /// `key` is shown again: it is no longer a kept tail. A key waiting for
    /// its release keeps waiting (the settle drops it while it is shown).
    pub fn reclaim(&mut self, key: &K) {
        if matches!(self.keys.get(key), Some(Retention::Closed { .. })) {
            self.keys.remove(key);
        }
    }

    /// `key` is gone altogether.
    pub fn forget(&mut self, key: &K) {
        self.keys.remove(key);
    }

    /// Keys for which `keep` is false are gone.
    pub fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.keys.retain(|key, _| keep(key));
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    /// The keys waiting for their release, in the order they closed; they
    /// leave the queue (a key that must go on waiting is queued again).
    pub fn take_releasing(&mut self) -> Vec<K> {
        let mut waiting: Vec<(u64, K)> = self
            .keys
            .iter()
            .filter_map(|(key, stage)| match stage {
                Retention::Releasing { queued } => Some((*queued, *key)),
                Retention::Closed { .. } => None,
            })
            .collect();
        waiting.sort_unstable_by_key(|(queued, _)| *queued);
        for (_, key) in &waiting {
            self.keys.remove(key);
        }
        waiting.into_iter().map(|(_, key)| key).collect()
    }

    /// The kept tails, most recently used first.
    pub fn closed_newest_first(&self) -> Vec<K> {
        let mut closed: Vec<(u64, K)> = self
            .keys
            .iter()
            .filter_map(|(key, stage)| match stage {
                Retention::Closed { used } => Some((*used, *key)),
                Retention::Releasing { .. } => None,
            })
            .collect();
        closed.sort_unstable_by_key(|(used, _)| std::cmp::Reverse(*used));
        closed.into_iter().map(|(_, key)| key).collect()
    }

    /// How many tails are kept.
    pub fn closed_len(&self) -> usize {
        self.keys
            .values()
            .filter(|stage| matches!(stage, Retention::Closed { .. }))
            .count()
    }

    /// The least recently used tail that `evictable` allows, if any.
    pub fn oldest_closed(&self, mut evictable: impl FnMut(&K) -> bool) -> Option<K> {
        self.closed_newest_first()
            .into_iter()
            .rev()
            .find(|key| evictable(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_either_waiting_or_ranked() {
        let mut keys = Retained::<i64>::default();
        keys.touch(1);
        keys.touch(2);
        assert_eq!(keys.closed_newest_first(), vec![2, 1]);
        keys.touch(1);
        assert_eq!(keys.closed_newest_first(), vec![1, 2]);
        // Closing a ranked key makes it wait, and only wait.
        keys.queue(2);
        assert!(keys.is_releasing(&2));
        assert_eq!(keys.closed_newest_first(), vec![1]);
        // A key that waits is not ranked by a touch.
        keys.touch(2);
        assert!(keys.is_releasing(&2));
        keys.queue(3);
        keys.queue(2);
        assert_eq!(keys.take_releasing(), vec![3, 2]);
        assert!(!keys.is_releasing(&2));
        assert_eq!(keys.closed_len(), 1);
        assert_eq!(keys.oldest_closed(|_| true), Some(1));
        keys.reclaim(&1);
        assert_eq!(keys.closed_len(), 0);
    }
}
