//! One value for a box's Save: what it sent and how that ended, read from
//! the request registry instead of guessed when the request leaves.
//!
//! A box that closes when it sends loses what the user typed when TDLib
//! then refuses; a box that closes on `Ok(Some(_))` from the driver shows
//! "saved" for a request that is only queued. The box holds a [`Submit`]
//! instead: `Sending` leaves on the answer (the box closes) or on the error
//! (the box stays open with the reason, `Failed`), and Cancel leaves it
//! too. The registry remembers the last failed requests
//! ([`RequestRegistry::failure`]) for that read.
use super::requests::RequestRegistry;
use crate::ids::RequestId;
use std::collections::VecDeque;

/// How many refused requests the registry remembers.
const FAILURES_KEPT: usize = 64;

/// The last requests TDLib refused, with the reason, newest last.
#[derive(Debug, Default)]
pub struct Failures(VecDeque<(u64, String)>);

impl RequestRegistry {
    /// Remember that request `id` was refused.
    pub fn record_failure(&mut self, id: RequestId, reason: String) {
        let kept = &mut self.failures.0;
        if kept.len() >= FAILURES_KEPT {
            kept.pop_front();
        }
        kept.push_back((id.0, reason));
    }

    /// Why request `id` was refused, while the registry still remembers.
    pub fn failure(&self, id: RequestId) -> Option<&str> {
        self.failures
            .0
            .iter()
            .rev()
            .find(|(key, _)| *key == id.0)
            .map(|(_, reason)| reason.as_str())
    }
}

/// The stage of one box's Save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Submit {
    /// Nothing sent: the user is typing.
    #[default]
    Editing,
    /// These requests are out. The box shows its Save busy until each
    /// one answered.
    Sending(Vec<RequestId>),
    /// A request failed (or was refused before it left): the typed form
    /// stays and the reason shows.
    Failed(String),
}

/// What a [`Submit`] reads as now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubmitView {
    Editing,
    Sending,
    Failed(String),
    /// Every sent request answered without an error: the box is done.
    Done,
}

impl Submit {
    /// A Save that sent `ids` (none: nothing was left to send, so it is
    /// done at once).
    pub fn sent(ids: Vec<RequestId>) -> Self {
        Submit::Sending(ids)
    }

    /// Where the Save is, given the registry of the session that sent it.
    /// `None`: no session answers for it any more, which is done.
    pub fn view(&self, requests: Option<&RequestRegistry>) -> SubmitView {
        match self {
            Submit::Editing => SubmitView::Editing,
            Submit::Failed(reason) => SubmitView::Failed(reason.clone()),
            Submit::Sending(ids) => {
                let Some(requests) = requests else {
                    return SubmitView::Done;
                };
                if let Some(reason) = ids.iter().find_map(|id| requests.failure(*id)) {
                    return SubmitView::Failed(reason.to_string());
                }
                if ids.iter().any(|id| requests.get(*id).is_some()) {
                    SubmitView::Sending
                } else {
                    SubmitView::Done
                }
            }
        }
    }

    /// The box's own view: `Sending` and `Failed` move to `Failed` when
    /// a request failed, so the reason stays after the registry forgets it.
    pub fn settle(&mut self, requests: Option<&RequestRegistry>) -> SubmitView {
        let view = self.view(requests);
        if let SubmitView::Failed(reason) = &view {
            *self = Submit::Failed(reason.clone());
        }
        view
    }

    /// Keep a failure the registry reported. `true` when this changed the
    /// stage, so the box needs a redraw.
    pub fn keep_failure(&mut self, reason: String) -> bool {
        if matches!(self, Submit::Failed(kept) if *kept == reason) {
            return false;
        }
        *self = Submit::Failed(reason);
        true
    }

    pub fn is_sending(&self, requests: Option<&RequestRegistry>) -> bool {
        self.view(requests) == SubmitView::Sending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RequestPurpose;

    fn registry_with(ids: usize) -> (RequestRegistry, Vec<RequestId>) {
        let mut registry = RequestRegistry::default();
        let ids = (0..ids)
            .map(|_| {
                registry.register(
                    crate::ids::AccountGeneration::default(),
                    RequestPurpose::SetChatTitle,
                    None,
                    None,
                )
            })
            .collect();
        (registry, ids)
    }

    #[test]
    fn sending_leaves_on_the_answer() {
        let (mut registry, ids) = registry_with(2);
        let submit = Submit::sent(ids.clone());
        assert_eq!(submit.view(Some(&registry)), SubmitView::Sending);
        registry.take(ids[0]);
        assert_eq!(submit.view(Some(&registry)), SubmitView::Sending);
        registry.take(ids[1]);
        assert_eq!(submit.view(Some(&registry)), SubmitView::Done);
    }

    #[test]
    fn sending_leaves_on_the_error_and_keeps_the_reason() {
        let (mut registry, ids) = registry_with(2);
        let mut submit = Submit::sent(ids.clone());
        registry.take(ids[0]);
        registry.record_failure(ids[0], "CHAT_ADMIN_REQUIRED".into());
        assert_eq!(
            submit.settle(Some(&registry)),
            SubmitView::Failed("CHAT_ADMIN_REQUIRED".into())
        );
        // The reason outlives the registry's memory of it.
        assert_eq!(
            submit.view(None),
            SubmitView::Failed("CHAT_ADMIN_REQUIRED".into())
        );
    }

    #[test]
    fn the_registry_remembers_only_the_last_failures() {
        let mut registry = RequestRegistry::default();
        for id in 0..200 {
            registry.record_failure(RequestId(id), format!("e{id}"));
        }
        assert_eq!(registry.failure(RequestId(0)), None);
        assert_eq!(registry.failure(RequestId(199)), Some("e199"));
    }
}
