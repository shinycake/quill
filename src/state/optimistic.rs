//! Saves of a whole settings object that show at once and are confirmed or
//! undone by the server's answer.
//!
//! The next edit of the same object starts from what is shown, so two edits
//! sent before the first one's update arrives never revert each other. The
//! saves in flight are kept with the value each one sent and the value the
//! server is known to hold (the one from before the first of them, moved
//! forward by every accepted save); when the last one answers and any save
//! of the burst was refused, that held value comes back.
use crate::ids::RequestId;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone)]
struct Saves<V> {
    /// The saves in flight, each with the value it sent.
    requests: Vec<(RequestId, V)>,
    /// What the server holds: what was shown before the first save in
    /// flight (`None`: nothing), then the value of the last save it
    /// accepted.
    base: Option<V>,
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
    /// A save went out with `sent`; `shown` is what the object showed
    /// before it.
    pub fn begin(
        &mut self,
        key: K,
        request: RequestId,
        sent: V,
        shown: impl FnOnce() -> Option<V>,
    ) {
        let saves = self.saves.entry(key).or_insert_with(|| Saves {
            requests: Vec::new(),
            base: shown(),
            refused: false,
        });
        saves.requests.push((request, sent));
    }

    /// A save of `key` is in flight: an update of the object is mostly its
    /// echo, and the answer settles what shows.
    pub fn is_saving(&self, key: &K) -> bool {
        self.saves.contains_key(key)
    }

    /// A save answered. An accepted save makes its value the held one.
    /// When it was the last one in flight and a save of this burst was
    /// refused, returns the held value to put back (`Some(None)`: the
    /// object showed nothing before).
    pub fn settle(&mut self, key: &K, request: RequestId, refused: bool) -> Option<Option<V>> {
        let saves = self.saves.get_mut(key)?;
        let at = saves.requests.iter().position(|(r, _)| *r == request)?;
        let (_, sent) = saves.requests.remove(at);
        if refused {
            saves.refused = true;
        } else {
            saves.base = Some(sent);
        }
        if !saves.requests.is_empty() {
            return None;
        }
        let done = self.saves.remove(key)?;
        done.refused.then_some(done.base)
    }
}

#[cfg(test)]
mod tests {
    use super::OptimisticSaves;
    use crate::ids::RequestId;

    /// An earlier save of the burst was accepted: a later refusal restores
    /// its value, not the one from before the burst.
    #[test]
    fn a_refusal_restores_the_last_accepted_value() {
        let mut saves: OptimisticSaves<u8, i32> = OptimisticSaves::default();
        saves.begin(1, RequestId(1), 20, || Some(10));
        saves.begin(1, RequestId(2), 30, || Some(10));
        assert_eq!(saves.settle(&1, RequestId(1), false), None);
        assert_eq!(saves.settle(&1, RequestId(2), true), Some(Some(20)));
    }

    #[test]
    fn the_value_comes_back_only_when_the_last_save_is_refused() {
        let mut saves: OptimisticSaves<u8, i32> = OptimisticSaves::default();
        saves.begin(1, RequestId(1), 20, || Some(10));
        saves.begin(1, RequestId(2), 30, || Some(10));
        assert!(saves.is_saving(&1));
        assert_eq!(
            saves.settle(&1, RequestId(1), true),
            None,
            "one is still out"
        );
        assert_eq!(saves.settle(&1, RequestId(2), false), Some(Some(30)));
        assert!(!saves.is_saving(&1));
        saves.begin(1, RequestId(3), 40, || Some(30));
        assert_eq!(
            saves.settle(&1, RequestId(3), false),
            None,
            "all went through"
        );
        saves.begin(2, RequestId(4), 5, || None);
        assert_eq!(saves.settle(&2, RequestId(4), true), Some(None));
        assert_eq!(saves.settle(&2, RequestId(9), true), None, "not ours");
    }
}
