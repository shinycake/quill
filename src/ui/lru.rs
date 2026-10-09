//! A small count-capped least-recently-used map for thread-local render
//! caches that would otherwise grow with every key ever seen.

use std::collections::HashMap;
use std::hash::Hash;

pub(super) struct Lru<K, V> {
    cap: usize,
    tick: u64,
    entries: HashMap<K, (u64, V)>,
}

impl<K: Hash + Eq + Clone, V: Clone> Lru<K, V> {
    pub(super) fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            tick: 0,
            entries: HashMap::new(),
        }
    }

    /// The cached value, marked most recently used.
    pub(super) fn get(&mut self, key: &K) -> Option<V> {
        self.tick += 1;
        let tick = self.tick;
        self.entries.get_mut(key).map(|(used, value)| {
            *used = tick;
            value.clone()
        })
    }

    /// Insert `value`; when that pushes the map past its cap, the least
    /// recently used entry is removed and returned.
    pub(super) fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.tick += 1;
        self.entries.insert(key, (self.tick, value));
        if self.entries.len() <= self.cap {
            return None;
        }
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, (used, _))| *used)
            .map(|(key, _)| key.clone())?;
        self.entries.remove(&oldest).map(|(_, value)| value)
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::Lru;

    #[test]
    fn evicts_the_least_recently_used_entry_at_the_cap() {
        let mut lru = Lru::new(2);
        assert_eq!(lru.insert(1, "a"), None);
        assert_eq!(lru.insert(2, "b"), None);
        assert_eq!(lru.get(&1), Some("a"));
        assert_eq!(lru.insert(3, "c"), Some("b"));
        assert_eq!(lru.len(), 2);
        assert_eq!(lru.get(&2), None);
        assert_eq!(lru.get(&1), Some("a"));
    }

    #[test]
    fn stays_bounded_under_many_keys() {
        let mut lru = Lru::new(8);
        for key in 0..1000 {
            lru.insert(key, key);
        }
        assert_eq!(lru.len(), 8);
    }
}
