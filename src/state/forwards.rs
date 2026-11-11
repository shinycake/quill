//! `forwardMessages` in flight and the outcomes the UI has not shown yet.
use super::*;
use std::collections::VecDeque;

/// Outcomes kept for the UI to show; one more drops the oldest.
pub const FORWARD_RESULTS_MAX: usize = 16;

/// The forwards of the account: one flight per destination (the share box
/// sends to several chats at once), and one outcome per finished flight
/// until the UI takes it. Several flights finishing before the UI polls
/// each keep their outcome.
#[derive(Debug, Default)]
pub struct Forwards {
    flights: Vec<ForwardFlight>,
    results: VecDeque<ForwardResult>,
}

impl Forwards {
    /// A `forwardMessages` went out.
    pub fn begin(&mut self, flight: ForwardFlight) {
        self.flights.push(flight);
    }

    /// The send of `request` was refused: it is not in flight.
    pub fn unsent(&mut self, request: RequestId) {
        self.flights.retain(|flight| flight.extra != request);
    }

    /// Whether another forward is on its way.
    pub fn in_flight(&self) -> bool {
        !self.flights.is_empty()
    }

    /// The flight of `request`, ended.
    pub(crate) fn finish(&mut self, request: RequestId) -> Option<ForwardFlight> {
        let index = self.flights.iter().position(|f| f.extra == request)?;
        Some(self.flights.remove(index))
    }

    /// The account is going away: nothing is on its way and no outcome is
    /// waiting.
    pub fn clear(&mut self) {
        self.flights.clear();
        self.results.clear();
    }

    /// The user is logging out: the flights will never be answered.
    pub fn forget_flights(&mut self) {
        self.flights.clear();
    }

    pub(crate) fn push_result(&mut self, result: ForwardResult) {
        self.results.push_back(result);
        while self.results.len() > FORWARD_RESULTS_MAX {
            self.results.pop_front();
        }
    }

    /// The outcomes nobody showed are forgotten.
    pub fn clear_results(&mut self) {
        self.results.clear();
    }

    /// The oldest outcome the UI has not shown, taken.
    pub fn take_result(&mut self) -> Option<ForwardResult> {
        self.results.pop_front()
    }

    /// The newest outcome, left in place (the demo mirrors it).
    pub fn latest_result(&self) -> Option<&ForwardResult> {
        self.results.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(dest: i64) -> ForwardResult {
        ForwardResult {
            dest_chat_id: ChatId(dest),
            dest_title: String::new(),
            from_chat_id: ChatId(1),
            from_topic: crate::ids::TopicId::None,
            requested: 1,
            forwarded_ids: Vec::new(),
        }
    }

    #[test]
    fn unshown_outcomes_are_bounded_and_the_oldest_goes() {
        let mut forwards = Forwards::default();
        for dest in 0..(FORWARD_RESULTS_MAX as i64 + 4) {
            forwards.push_result(result(dest));
        }
        assert_eq!(
            forwards.take_result().map(|r| r.dest_chat_id),
            Some(ChatId(4)),
            "the four oldest were dropped"
        );
        let mut left = 1;
        while forwards.take_result().is_some() {
            left += 1;
        }
        assert_eq!(left, FORWARD_RESULTS_MAX);
    }
}
