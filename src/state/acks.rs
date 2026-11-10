//! The answer to each request a surface waits on, kept per request: the
//! surface that sent it (the chat look dialog, an edit, a rich send) settles
//! on its own request's answer, never on a counter or an error field that
//! another request shares. The reducer settles every answered request
//! ([`Session::apply`](crate::state::Session::apply)); only a request a
//! surface registered with [`RequestAcks::expect`] keeps its answer.
use crate::ids::RequestId;
use std::collections::{HashMap, HashSet};

/// How one request ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Done,
    Failed(String),
}

/// Requests a surface waits for, and their answers. An answer is kept only
/// for a request the surface registered with [`Self::expect`], and only
/// until it takes it ([`Self::take`]) or forgets the request
/// ([`Self::forget`], when it goes away), so nothing outlives the surface
/// that asked.
#[derive(Debug, Default)]
pub struct RequestAcks {
    awaited: HashSet<RequestId>,
    settled: HashMap<RequestId, Answer>,
}

impl RequestAcks {
    /// A surface waits for `id`.
    pub fn expect(&mut self, id: RequestId) {
        self.awaited.insert(id);
    }

    /// `id` was answered; kept only when a surface waits for it.
    pub fn settle(&mut self, id: RequestId, answer: Answer) {
        if self.awaited.remove(&id) {
            self.settled.insert(id, answer);
        }
    }

    /// The answer to `id`, once, if it came.
    pub fn take(&mut self, id: RequestId) -> Option<Answer> {
        self.settled.remove(&id)
    }

    /// The surface no longer waits for `id` (it closed or moved on).
    pub fn forget(&mut self, id: RequestId) {
        self.awaited.remove(&id);
        self.settled.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_kept_only_for_a_request_that_is_awaited() {
        let mut acks = RequestAcks::default();
        acks.settle(RequestId(1), Answer::Done);
        assert_eq!(acks.take(RequestId(1)), None, "nobody waits for it");
        acks.expect(RequestId(2));
        assert_eq!(acks.take(RequestId(2)), None, "not answered yet");
        acks.settle(RequestId(2), Answer::Failed("X".into()));
        assert_eq!(acks.take(RequestId(2)), Some(Answer::Failed("X".into())));
        assert_eq!(acks.take(RequestId(2)), None, "taken once");
    }

    #[test]
    fn a_forgotten_request_leaves_nothing_behind() {
        let mut acks = RequestAcks::default();
        acks.expect(RequestId(3));
        acks.forget(RequestId(3));
        acks.settle(RequestId(3), Answer::Done);
        assert_eq!(acks.take(RequestId(3)), None);
        acks.expect(RequestId(4));
        acks.settle(RequestId(4), Answer::Done);
        acks.forget(RequestId(4));
        assert_eq!(acks.take(RequestId(4)), None);
    }
}
