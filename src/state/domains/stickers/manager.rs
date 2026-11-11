//! The stickers manager box's share of the session: the changes it saves
//! when it closes and what the driver has already asked for.
//!
//! tdesktop keeps removals and the new order in the box and sends them when
//! the box closes (`StickersBox::saveChanges`): the removed sets are
//! uninstalled first, then the order of the rest is sent. The queue here
//! does the same one step at a time, because TDLib answers a reorder with
//! the installed list as it was when the request went out.
use crate::ids::RequestId;
use std::collections::{HashSet, VecDeque};

/// One closed box's changes, waiting to be sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagerCommit {
    /// The changes are for mask sets, not regular sticker sets.
    pub masks: bool,
    /// The steps still to send, oldest first: the removals one request at a
    /// time, then the order of the sets that stay.
    pub steps: VecDeque<CommitStep>,
    /// The request of the step on its way: only its failure is this
    /// change's (a set box's `changeStickerSet` that fails is not).
    pub sent: Option<RequestId>,
}

/// What the manager box needs from the driver.
#[derive(Debug, Clone, Default)]
pub struct StickersManager {
    /// The box is on screen: set thumbnails download.
    pub open: bool,
    /// Sets whose first sticker has been asked for, once each.
    pub covers_requested: HashSet<i64>,
    /// Saved changes not yet sent, oldest first.
    pub commits: VecDeque<ManagerCommit>,
    /// A step of a saved change that did not go through (a refused
    /// request, a client that could not send): the box is closed by then,
    /// so the UI says so once.
    pub failure: Option<CommitFailure>,
}

/// A saved change of the manager box that was lost on the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitFailure {
    /// The change was for mask sets, not regular sticker sets.
    pub masks: bool,
}

impl StickersManager {
    /// The request `failed` was answered with an error: it is a failure of
    /// the oldest saved change only when it was that change's own step.
    /// True when it was (the failure is then reported by the manager).
    pub fn note_commit_failure(&mut self, failed: RequestId) -> bool {
        if let Some(front) = self.commits.front()
            && front.sent == Some(failed)
        {
            self.failure = Some(CommitFailure { masks: front.masks });
            return true;
        }
        false
    }

    /// A step of the oldest saved change could not go out at all.
    pub fn note_send_failure(&mut self) {
        if let Some(front) = self.commits.front() {
            self.failure = Some(CommitFailure { masks: front.masks });
        }
    }
}

/// What a commit does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitStep {
    Remove(i64),
    Reorder(Vec<i64>),
    Done,
}

impl ManagerCommit {
    /// The steps of a saved change: every removal, then the order once.
    /// Fewer than two sets have no order to send.
    pub fn new(
        masks: bool,
        removals: impl IntoIterator<Item = i64>,
        order: Option<Vec<i64>>,
    ) -> Self {
        let mut steps: VecDeque<CommitStep> =
            removals.into_iter().map(CommitStep::Remove).collect();
        if let Some(order) = order.filter(|order| order.len() >= 2) {
            steps.push_back(CommitStep::Reorder(order));
        }
        Self {
            masks,
            steps,
            sent: None,
        }
    }

    /// The next request, taking it off the commit.
    pub fn next_step(&mut self) -> CommitStep {
        self.steps.pop_front().unwrap_or(CommitStep::Done)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removals_go_first_then_the_order_once() {
        let mut commit = ManagerCommit::new(false, [4, 5], Some(vec![3, 1, 2]));
        assert_eq!(commit.next_step(), CommitStep::Remove(4));
        assert_eq!(commit.next_step(), CommitStep::Remove(5));
        assert_eq!(commit.next_step(), CommitStep::Reorder(vec![3, 1, 2]));
        assert_eq!(commit.next_step(), CommitStep::Done);
    }

    #[test]
    fn a_single_set_has_no_order_to_send() {
        let mut commit = ManagerCommit::new(true, [], Some(vec![1]));
        assert_eq!(commit.next_step(), CommitStep::Done);
    }
}
