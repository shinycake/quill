//! What each request a stage sent is for, held by the stage that owns the
//! work. "A request of this kind is out" is answered from here by the
//! request's own `RequestId`, not by scanning the session's request table by
//! purpose (any other window's or older request of that purpose answers a
//! scan too).
//!
//! The reducer settles a request in [`Session::settle_held`] when its answer
//! or error lands, or when the request is lost, so nothing outlives its
//! request.
use crate::ids::RequestId;

/// The requests one stage has out, each with what it is for.
///
/// Every field of this type must be listed in [`Session::settle_held`]: a
/// `Holds` left off that list is never settled, and its dedup then blocks
/// for the rest of the session. (`held_requests_all_settle_when_lost` in
/// `state/tests/sweep_r4_05_fix3.rs` begins one request in each owner.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Holds<K> {
    items: Vec<(RequestId, K)>,
}

impl<K> Default for Holds<K> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<K> Holds<K> {
    /// `request` went out for `key`.
    pub fn begin(&mut self, request: RequestId, key: K) {
        self.items.push((request, key));
    }

    /// `request` came back, or was lost: it is out no more. `false` when
    /// this stage did not hold it.
    pub fn settle(&mut self, request: RequestId) -> bool {
        let before = self.items.len();
        self.items.retain(|(held, _)| *held != request);
        self.items.len() < before
    }

    /// Some held request is for a key `matches`.
    pub fn any(&self, matches: impl Fn(&K) -> bool) -> bool {
        self.items.iter().any(|(_, key)| matches(key))
    }

    /// The requests held with their keys, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = (RequestId, &K)> {
        self.items.iter().map(|(request, key)| (*request, key))
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Whether `request` is held.
    pub fn holds(&self, request: RequestId) -> bool {
        self.items.iter().any(|(held, _)| *held == request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_leaves_with_its_own_answer_only() {
        let mut holds = Holds::default();
        holds.begin(RequestId(1), "a");
        holds.begin(RequestId(2), "b");
        assert!(holds.any(|key| *key == "a"));
        assert!(!holds.settle(RequestId(9)));
        assert!(holds.settle(RequestId(1)));
        assert!(!holds.any(|key| *key == "a"));
        assert!(holds.any(|key| *key == "b"));
        assert!(holds.holds(RequestId(2)));
        assert!(holds.settle(RequestId(2)));
        assert!(holds.is_empty());
    }
}
