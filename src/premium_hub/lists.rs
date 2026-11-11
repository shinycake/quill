//! The two paged lists of the hub, each one value that is replaced whole
//! when its owner changes: the Stars history of one filter, and the
//! received gifts of one owner. What was a row of fields kept in step by
//! hand (rows, cursor, page stage, selection, notice) is one struct whose
//! constructors and `restart` methods are the only places that reset it.
#![deny(clippy::indexing_slicing)]

use super::gift_view::GiftView;
use super::page_stage::PageStage;
use super::{ReceivedGift, StarTx, TxFilter};
use crate::state::fetch::ListOp;
use crate::telegram::envelope::MessageSender;

/// What the Stars box shows: closed, the list, or one transaction's details.
/// The selection exists only while the box is open, so closing takes it
/// along.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StarsView {
    #[default]
    Closed,
    List,
    /// The details box of the transaction with this id.
    Detail(String),
}

/// The Stars history of one filter. A filter change replaces it whole
/// ([`StarsHistory::switch_filter`]); the history outlives the box so a
/// balance refresh does not start from nothing.
#[derive(Debug, Clone, Default)]
pub struct StarsHistory {
    pub view: StarsView,
    pub filter: TxFilter,
    pub rows: Vec<StarTx>,
    /// Where the next page starts; empty when there is none.
    pub offset: String,
    /// The history request: only the newest one's answer counts (a filter
    /// switched mid-flight drops older ones).
    pub page: PageStage,
}

impl StarsHistory {
    /// The Stars box is open.
    pub fn is_open(&self) -> bool {
        self.view != StarsView::Closed
    }

    pub fn open(&mut self) {
        if self.view == StarsView::Closed {
            self.view = StarsView::List;
        }
    }

    /// Close the box; a transaction's details go with it.
    pub fn close(&mut self) {
        self.view = StarsView::Closed;
    }

    /// The transaction whose details are showing.
    pub fn selected(&self) -> Option<&str> {
        match &self.view {
            StarsView::Detail(id) => Some(id),
            StarsView::Closed | StarsView::List => None,
        }
    }

    /// Show (`Some`) or hide (`None`) a transaction's details. Nothing to
    /// show while the box is closed.
    pub fn select(&mut self, id: Option<String>) {
        if self.view == StarsView::Closed {
            return;
        }
        self.view = match id {
            Some(id) => StarsView::Detail(id),
            None => StarsView::List,
        };
    }

    /// Another tab: the history starts over, still open if it was, with no
    /// transaction selected.
    pub fn switch_filter(&mut self, filter: TxFilter) {
        let view = match self.view {
            StarsView::Closed => StarsView::Closed,
            StarsView::List | StarsView::Detail(_) => StarsView::List,
        };
        *self = Self {
            view,
            filter,
            ..Self::default()
        };
    }
}

/// The received gifts of one owner, with the panel over them and the
/// changes under way. Opening it for an owner replaces it whole
/// ([`ReceivedGifts::opened_for`]); the list outlives the box (a toggle
/// still out answers into it), and `view` says whether the box is open.
#[derive(Debug, Clone, Default)]
pub struct ReceivedGifts {
    pub owner: Option<MessageSender>,
    pub rows: Vec<ReceivedGift>,
    pub total: i32,
    /// Where the next page starts; empty when there is none.
    pub offset: String,
    /// The list request: only the newest one's answer counts.
    pub page: PageStage,
    /// Why the last toggle, sale or management request failed.
    pub notice: Option<String>,
    /// Closed, the list alone, a gift's panel or its convert confirmation.
    pub view: GiftView,
    /// The list's changes: one mutation in flight under its request (the
    /// controls stay off), or the list marked to be fetched again.
    pub op: ListOp,
}

impl ReceivedGifts {
    /// The box is open.
    pub fn is_open(&self) -> bool {
        self.view.is_open()
    }

    /// Open the box on the list as it is.
    pub fn open(&mut self) {
        if self.view == GiftView::Closed {
            self.view = GiftView::List;
        }
    }

    /// The list of `owner`, opened: nothing loaded yet. A change still out
    /// keeps its lock until its own answer (another request would pay
    /// twice); a refetch owed to the old list is no longer owed.
    pub fn opened_for(owner: MessageSender, mut op: ListOp) -> Self {
        op.refetched();
        Self {
            owner: Some(owner),
            view: GiftView::List,
            op,
            ..Self::default()
        }
    }

    /// The same owner's list from the top (another collection): rows,
    /// cursor, stage, notice and panel start over; the change in flight
    /// stays.
    pub fn restart(&mut self) {
        let op = self.op;
        *self = Self {
            owner: self.owner,
            view: GiftView::List,
            op,
            ..Self::default()
        };
    }

    /// Close the box. The list stays for a change still out.
    pub fn close(&mut self) {
        self.view = GiftView::Closed;
    }
}
