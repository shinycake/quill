//! Saves of a whole settings object that show at once and are confirmed or
//! undone by the server's answer.
//!
//! The next edit of the same object starts from what is shown, so two edits
//! sent before the first one's update arrives never revert each other. The
//! saves in flight are kept with the value from before the first of them;
//! when the last one answers with an error that value comes back.
use crate::ids::RequestId;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
struct Saves<V> {
    requests: Vec<RequestId>,
    /// What was shown before the first save in flight (`None`: nothing).
    previous: Option<V>,
    /// An answer of a save in flight was an error.
    refused: bool,
}

/// The saves in flight, by the key of the object they change.
#[derive(Debug, Clone)]
pub struct OptimisticSaves<K, V> {
    saves: HashMap<K, Saves<V>>,
}

impl<K, V> Default for OptimisticSaves<K, V> {
    fn default() -> Self {
        Self {
            saves: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash, V> OptimisticSaves<K, V> {
    /// A save went out; `shown` is what the object showed before it.
    pub fn begin(&mut self, key: K, request: RequestId, shown: impl FnOnce() -> Option<V>) {
        let saves = self.saves.entry(key).or_insert_with(|| Saves {
            requests: Vec::new(),
            previous: shown(),
            refused: false,
        });
        saves.requests.push(request);
    }

    /// A save of `key` is in flight: an update of the object is mostly its
    /// echo, and the answer settles what shows.
    pub fn is_saving(&self, key: &K) -> bool {
        self.saves.contains_key(key)
    }

    /// A save answered. When it was the last one in flight and a save
    /// of this burst was refused, returns the value to put back
    /// (`Some(None)`: the object showed nothing before).
    pub fn settle(&mut self, key: &K, request: RequestId, refused: bool) -> Option<Option<V>> {
        let saves = self.saves.get_mut(key)?;
        if !saves.requests.contains(&request) {
            return None;
        }
        saves.requests.retain(|r| *r != request);
        saves.refused |= refused;
        if !saves.requests.is_empty() {
            return None;
        }
        let done = self.saves.remove(key)?;
        done.refused.then_some(done.previous)
    }
}

#[cfg(test)]
mod tests {
    use super::OptimisticSaves;
    use crate::ids::RequestId;

    #[test]
    fn the_value_comes_back_only_when_the_last_save_is_refused() {
        let mut saves: OptimisticSaves<u8, i32> = OptimisticSaves::default();
        saves.begin(1, RequestId(1), || Some(10));
        saves.begin(1, RequestId(2), || Some(20));
        assert!(saves.is_saving(&1));
        assert_eq!(
            saves.settle(&1, RequestId(1), true),
            None,
            "one is still out"
        );
        assert_eq!(saves.settle(&1, RequestId(2), false), Some(Some(10)));
        assert!(!saves.is_saving(&1));
        saves.begin(1, RequestId(3), || Some(30));
        assert_eq!(
            saves.settle(&1, RequestId(3), false),
            None,
            "all went through"
        );
        saves.begin(2, RequestId(4), || None);
        assert_eq!(saves.settle(&2, RequestId(4), true), Some(None));
        assert_eq!(saves.settle(&2, RequestId(9), true), None, "not ours");
    }
}
