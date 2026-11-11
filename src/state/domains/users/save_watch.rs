//! The answers a box waits for before it closes: the profile boxes (edit
//! contact, birthday, personal channel, photo, share contact) send one
//! request and stay open, saving, until it is answered. The request is
//! watched here by its id, so an answer for another request (or an earlier
//! box) never closes this one.
use crate::ids::RequestId;

/// What the watched request has met so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveAnswer {
    /// Sent, not answered.
    Waiting,
    /// TDLib answered it (not with an error).
    Answered,
    /// TDLib refused it; the reason as the box shows it.
    Failed(String),
}

/// What became of a watched request, as the box that watched it asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Saving,
    Saved,
    Failed(String),
    /// Never watched, taken already, or it left flight with no answer
    /// (dropped with its connection): nothing to act on, and no success to
    /// claim.
    Unknown,
}

/// The request the open profile box waits on. Only one profile box is open
/// at a time, so one slot: watching a new request replaces whatever an
/// earlier box left (a box replaced without closing), and none outlives
/// the next one.
#[derive(Debug, Default)]
pub struct SaveWatch {
    entry: Option<(RequestId, SaveAnswer)>,
}

impl SaveWatch {
    /// Start watching `request`, just sent, in place of any earlier one.
    pub fn watch(&mut self, request: RequestId) {
        self.entry = Some((request, SaveAnswer::Waiting));
    }

    /// `request` was answered. Only the watched, unanswered one records it.
    pub fn answered(&mut self, request: RequestId) {
        if let Some((id, answer @ SaveAnswer::Waiting)) = self.entry.as_mut()
            && *id == request
        {
            *answer = SaveAnswer::Answered;
        }
    }

    /// `request` was refused. Only the watched, unanswered one records it.
    pub fn failed(&mut self, request: RequestId, reason: String) {
        if let Some((id, answer @ SaveAnswer::Waiting)) = self.entry.as_mut()
            && *id == request
        {
            *answer = SaveAnswer::Failed(reason);
        }
    }

    /// The box stopped waiting (closed, or took its outcome).
    pub fn forget(&mut self, request: RequestId) {
        if self.entry.as_ref().is_some_and(|(id, _)| *id == request) {
            self.entry = None;
        }
    }

    /// Where `request` stands. Saved only on its answer: a watched request
    /// that left flight (`in_flight` false) without one was dropped, so it
    /// is `Unknown`, never a success. The outcome is taken: the entry goes
    /// unless it is still saving.
    pub fn take_outcome(&mut self, request: RequestId, in_flight: bool) -> SaveOutcome {
        let Some((id, answer)) = self.entry.as_ref() else {
            return SaveOutcome::Unknown;
        };
        if *id != request {
            return SaveOutcome::Unknown;
        }
        let outcome = match answer {
            SaveAnswer::Waiting if in_flight => return SaveOutcome::Saving,
            SaveAnswer::Waiting => SaveOutcome::Unknown,
            SaveAnswer::Answered => SaveOutcome::Saved,
            SaveAnswer::Failed(reason) => SaveOutcome::Failed(reason.clone()),
        };
        self.entry = None;
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveOutcome, SaveWatch};
    use crate::ids::RequestId;

    #[test]
    fn a_watched_request_saves_on_its_answer_and_fails_with_its_reason() {
        let mut watch = SaveWatch::default();
        let (first, second) = (RequestId(1), RequestId(2));
        watch.watch(first);
        assert_eq!(watch.take_outcome(first, true), SaveOutcome::Saving);
        // Another request's answer or refusal does not touch it.
        watch.answered(second);
        watch.failed(second, "no".into());
        assert_eq!(watch.take_outcome(first, true), SaveOutcome::Saving);
        watch.answered(first);
        assert_eq!(watch.take_outcome(first, false), SaveOutcome::Saved);
        assert_eq!(watch.take_outcome(first, false), SaveOutcome::Unknown);
        watch.watch(second);
        watch.failed(second, "no".into());
        assert_eq!(
            watch.take_outcome(second, false),
            SaveOutcome::Failed("no".into())
        );
    }

    #[test]
    fn a_request_dropped_without_an_answer_is_not_a_success() {
        let mut watch = SaveWatch::default();
        watch.watch(RequestId(1));
        // Out of flight (a TDLib restart dropped it), never answered.
        assert_eq!(watch.take_outcome(RequestId(1), false), SaveOutcome::Unknown);
        assert_eq!(watch.take_outcome(RequestId(1), false), SaveOutcome::Unknown);
    }

    #[test]
    fn a_box_replaced_without_closing_leaves_nothing_behind() {
        let mut watch = SaveWatch::default();
        watch.watch(RequestId(1));
        // The box was replaced, never taken nor forgotten; the next one
        // takes the slot.
        watch.watch(RequestId(2));
        watch.answered(RequestId(1));
        assert_eq!(watch.take_outcome(RequestId(1), false), SaveOutcome::Unknown);
        watch.answered(RequestId(2));
        assert_eq!(watch.take_outcome(RequestId(2), false), SaveOutcome::Saved);
    }

    #[test]
    fn a_refusal_of_a_request_nobody_watches_is_ignored() {
        let mut watch = SaveWatch::default();
        watch.failed(RequestId(9), "no".into());
        watch.answered(RequestId(9));
        assert_eq!(watch.take_outcome(RequestId(9), false), SaveOutcome::Unknown);
        watch.watch(RequestId(9));
        watch.forget(RequestId(9));
        assert_eq!(watch.take_outcome(RequestId(9), false), SaveOutcome::Unknown);
    }
}
