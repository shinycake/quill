//! The answers a box waits for before it closes: the profile boxes (edit
//! contact, birthday, personal channel, photo, share contact) send one
//! request and stay open, saving, until it is answered. The request is
//! watched here by its id, so an answer for another request (or an earlier
//! box) never closes this one.
use crate::ids::RequestId;
use std::collections::HashMap;

/// One watched request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveAnswer {
    /// Sent, not answered.
    Waiting,
    /// TDLib refused it; the reason as the box shows it.
    Failed(String),
}

/// What became of a watched request, as the box that watched it asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saving,
    Saved,
    Failed(String),
    /// Never watched, or already taken: no answer to act on.
    Unknown,
}

/// The requests boxes wait on. An entry leaves when its box takes the
/// outcome or closes (`forget`), so none outlives its box.
#[derive(Debug, Default)]
pub struct SaveWatch {
    entries: HashMap<RequestId, SaveAnswer>,
}

impl SaveWatch {
    /// Start watching `request`, just sent.
    pub fn watch(&mut self, request: RequestId) {
        self.entries.insert(request, SaveAnswer::Waiting);
    }

    /// `request` was refused. Only a watched, unanswered one records it.
    pub fn failed(&mut self, request: RequestId, reason: String) {
        if let Some(entry @ SaveAnswer::Waiting) = self.entries.get_mut(&request) {
            *entry = SaveAnswer::Failed(reason);
        }
    }

    /// The box stopped waiting (closed, or took its outcome).
    pub fn forget(&mut self, request: RequestId) {
        self.entries.remove(&request);
    }

    /// Where `request` stands. A watched request that is no longer in
    /// flight (`in_flight` false) and was not refused was answered: saved.
    /// The outcome is taken: the entry goes unless it is still saving.
    pub fn take_outcome(&mut self, request: RequestId, in_flight: bool) -> SaveOutcome {
        match self.entries.get(&request) {
            None => SaveOutcome::Unknown,
            Some(SaveAnswer::Failed(reason)) => {
                let reason = reason.clone();
                self.entries.remove(&request);
                SaveOutcome::Failed(reason)
            }
            Some(SaveAnswer::Waiting) if in_flight => SaveOutcome::Saving,
            Some(SaveAnswer::Waiting) => {
                self.entries.remove(&request);
                SaveOutcome::Saved
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveOutcome, SaveWatch};
    use crate::ids::RequestId;

    #[test]
    fn a_watched_request_saves_when_it_leaves_flight_and_fails_with_its_reason() {
        let mut watch = SaveWatch::default();
        let (first, second) = (RequestId(1), RequestId(2));
        watch.watch(first);
        watch.watch(second);
        assert_eq!(watch.take_outcome(first, true), SaveOutcome::Saving);
        watch.failed(second, "no".into());
        // Another request's refusal does not touch the first.
        assert_eq!(watch.take_outcome(first, true), SaveOutcome::Saving);
        assert_eq!(watch.take_outcome(first, false), SaveOutcome::Saved);
        assert_eq!(watch.take_outcome(first, false), SaveOutcome::Unknown);
        assert_eq!(
            watch.take_outcome(second, false),
            SaveOutcome::Failed("no".into())
        );
    }

    #[test]
    fn a_refusal_of_a_request_nobody_watches_is_ignored() {
        let mut watch = SaveWatch::default();
        watch.failed(RequestId(9), "no".into());
        assert_eq!(
            watch.take_outcome(RequestId(9), false),
            SaveOutcome::Unknown
        );
        watch.watch(RequestId(9));
        watch.forget(RequestId(9));
        assert_eq!(
            watch.take_outcome(RequestId(9), false),
            SaveOutcome::Unknown
        );
    }
}
