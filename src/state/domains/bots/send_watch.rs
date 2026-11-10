//! Sends a box waits on: the Add-bot and Share-game boxes close on the
//! answer of their request, not when it was only queued.
use crate::state::*;

/// The most requests watched at once; a box that was closed meanwhile
/// leaves its entry behind, and the oldest go first.
const KEPT: usize = 16;

/// Watched requests and how each ended: `None` while waiting, then whether
/// it was answered without an error.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SendWatch(Vec<(RequestId, Option<bool>)>);

impl SendWatch {
    pub fn watch(&mut self, request: RequestId) {
        self.0.retain(|(id, _)| *id != request);
        self.0.push((request, None));
        if self.0.len() > KEPT {
            self.0.remove(0);
        }
    }

    /// The answer of `request` arrived (`ok`: without an error).
    pub fn settle(&mut self, request: RequestId, ok: bool) {
        if let Some((_, outcome)) = self.0.iter_mut().find(|(id, _)| *id == request) {
            *outcome = Some(ok);
        }
    }

    /// How the request ended, once it did; the entry goes with it.
    pub fn take_outcome(&mut self, request: RequestId) -> Option<bool> {
        let at = self
            .0
            .iter()
            .position(|(id, outcome)| *id == request && outcome.is_some())?;
        self.0.remove(at).1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_outcome_is_taken_once_and_only_for_the_watched_request() {
        let mut sends = SendWatch::default();
        sends.watch(RequestId(1));
        sends.settle(RequestId(2), true);
        assert_eq!(sends.take_outcome(RequestId(1)), None, "still waiting");
        sends.settle(RequestId(1), false);
        assert_eq!(sends.take_outcome(RequestId(1)), Some(false));
        assert_eq!(sends.take_outcome(RequestId(1)), None);
    }
}
