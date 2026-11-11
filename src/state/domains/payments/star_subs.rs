//! The Subscriptions dialog's list and its one mutation, each one value
//! (slice `parity:bots-payment-recurring`).
use crate::state::paged::{Page, Paged};
use crate::telegram::envelope::StarSubscriptionsData;

impl Page for StarSubscriptionsData {
    fn next_offset(&self) -> &str {
        &self.next_offset
    }

    fn append(&mut self, later: Self) {
        self.star_amount = later.star_amount;
        self.required_star_count = later.required_star_count;
        self.next_offset = later.next_offset;
        self.subscriptions.extend(later.subscriptions);
    }
}

/// `editStarSubscription` / `reuseStarSubscription`, one at a time. The
/// dialog disables its action buttons while `Busy`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SubsEdit {
    #[default]
    Idle,
    Busy,
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StarSubscriptions {
    pub list: Paged<StarSubscriptionsData>,
    pub edit: SubsEdit,
}

impl StarSubscriptions {
    /// The error line the dialog shows: the list's, else the mutation's.
    pub fn error(&self) -> Option<&str> {
        match (self.list.error(), &self.edit) {
            (Some(error), _) => Some(error),
            (None, SubsEdit::Failed(error)) => Some(error),
            _ => None,
        }
    }

    pub fn mutating(&self) -> bool {
        self.edit == SubsEdit::Busy
    }

    /// A mutation's `ok`: the list is out of date.
    pub fn outdate(&mut self) {
        self.edit = SubsEdit::Idle;
        self.list.outdate();
    }

    pub fn begin_edit(&mut self) {
        self.edit = SubsEdit::Busy;
    }

    pub fn edit_failed(&mut self, error: impl Into<String>) {
        self.edit = SubsEdit::Failed(error.into());
    }

    /// A mutation's send failed, so no answer will come.
    pub fn edit_unsent(&mut self) {
        self.edit = SubsEdit::Idle;
    }
}
