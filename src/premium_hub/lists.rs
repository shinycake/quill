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

/// "Buy more Stars" in the Stars box: folded, or expanded to the pack list.
/// It lives inside the open stages of [`StarsView`], so closing the box
/// folds it again without a second reset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PacksView {
    #[default]
    Folded,
    Expanded,
}

/// What the Stars box shows: closed, the list, or one transaction's details.
/// The selection and the packs section exist only while the box is open, so
/// closing takes them along.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum StarsView {
    #[default]
    Closed,
    List {
        packs: PacksView,
    },
    /// The details box of the transaction with this id, over the list.
    Detail {
        id: String,
        packs: PacksView,
    },
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
            self.view = StarsView::List {
                packs: PacksView::Folded,
            };
        }
    }

    /// Close the box; a transaction's details and the packs go with it.
    pub fn close(&mut self) {
        self.view = StarsView::Closed;
    }

    /// The transaction whose details are showing.
    pub fn selected(&self) -> Option<&str> {
        match &self.view {
            StarsView::Detail { id, .. } => Some(id),
            StarsView::Closed | StarsView::List { .. } => None,
        }
    }

    /// Show (`Some`) or hide (`None`) a transaction's details. Nothing to
    /// show while the box is closed.
    pub fn select(&mut self, id: Option<String>) {
        let Some(packs) = self.packs() else {
            return;
        };
        self.view = match id {
            Some(id) => StarsView::Detail { id, packs },
            None => StarsView::List { packs },
        };
    }

    /// The packs section of the open box.
    fn packs(&self) -> Option<PacksView> {
        match &self.view {
            StarsView::Closed => None,
            StarsView::List { packs } | StarsView::Detail { packs, .. } => Some(*packs),
        }
    }

    /// "Buy more Stars" shows its packs.
    pub fn packs_expanded(&self) -> bool {
        self.packs() == Some(PacksView::Expanded)
    }

    /// Fold or expand the packs; returns whether they show now. Nothing to
    /// do while the box is closed.
    pub fn toggle_packs(&mut self) -> bool {
        let expand = !self.packs_expanded();
        self.set_packs(if expand {
            PacksView::Expanded
        } else {
            PacksView::Folded
        });
        self.packs_expanded()
    }

    /// Expand the packs of the open box (a reaction that needs more Stars).
    pub fn expand_packs(&mut self) {
        self.set_packs(PacksView::Expanded);
    }

    fn set_packs(&mut self, to: PacksView) {
        match &mut self.view {
            StarsView::Closed => {}
            StarsView::List { packs } | StarsView::Detail { packs, .. } => *packs = to,
        }
    }

    /// Another tab: the history starts over, still open if it was, with no
    /// transaction selected and the packs as they were.
    pub fn switch_filter(&mut self, filter: TxFilter) {
        let view = match self.packs() {
            None => StarsView::Closed,
            Some(packs) => StarsView::List { packs },
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
/// ([`ReceivedGifts::opened_for`]) and closing empties it, keeping only the
/// lock of a change still out; `view` says whether the box is open.
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

    /// The box closed: no owner, list, page or refetch mark outlives it, so
    /// nothing asks for or mutates a list nobody sees. A change in flight
    /// keeps its lock until its own answer (its late `changed` mark is
    /// dropped by the next open).
    pub fn close(&mut self) {
        let mut op = self.op;
        op.refetched();
        *self = Self {
            op,
            ..Self::default()
        };
    }
}
