//! The stickers manager box's share of the session: the changes it saves
//! when it closes and what the driver has already asked for.
//!
//! tdesktop keeps removals and the new order in the box and sends them when
//! the box closes (`StickersBox::saveChanges`): the removed sets are
//! uninstalled first, then the order of the rest is sent. The queue here
//! does the same one step at a time, because TDLib answers a reorder with
//! the installed list as it was when the request went out.
use std::collections::{HashSet, VecDeque};

/// One closed box's changes, waiting to be sent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManagerCommit {
    /// The changes are for mask sets, not regular sticker sets.
    pub masks: bool,
    /// Sets to uninstall, one request at a time.
    pub removals: VecDeque<i64>,
    /// The order of the sets that stay, sent after the last removal.
    pub order: Option<Vec<i64>>,
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
}

/// What a commit does next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitStep {
    Remove(i64),
    Reorder(Vec<i64>),
    Done,
}

impl ManagerCommit {
    /// The next request, taking it off the commit. Fewer than two sets
    /// have no order to send.
    pub fn next_step(&mut self) -> CommitStep {
        if let Some(id) = self.removals.pop_front() {
            return CommitStep::Remove(id);
        }
        match self.order.take() {
            Some(order) if order.len() >= 2 => CommitStep::Reorder(order),
            _ => CommitStep::Done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removals_go_first_then_the_order_once() {
        let mut commit = ManagerCommit {
            masks: false,
            removals: VecDeque::from([4, 5]),
            order: Some(vec![3, 1, 2]),
        };
        assert_eq!(commit.next_step(), CommitStep::Remove(4));
        assert_eq!(commit.next_step(), CommitStep::Remove(5));
        assert_eq!(commit.next_step(), CommitStep::Reorder(vec![3, 1, 2]));
        assert_eq!(commit.next_step(), CommitStep::Done);
    }

    #[test]
    fn a_single_set_has_no_order_to_send() {
        let mut commit = ManagerCommit {
            masks: true,
            removals: VecDeque::new(),
            order: Some(vec![1]),
        };
        assert_eq!(commit.next_step(), CommitStep::Done);
    }
}
