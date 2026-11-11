//! The pages of the Stars history and of the received-gifts list, as the
//! items one [`Paged`](crate::state::paged::Paged) value holds.
use super::{ReceivedGift, StarTx};
use crate::state::paged::Page;

/// The Stars history so far.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TxList {
    pub transactions: Vec<StarTx>,
    pub next_offset: String,
}

impl Page for TxList {
    fn next_offset(&self) -> &str {
        &self.next_offset
    }

    fn append(&mut self, later: Self) {
        self.transactions.extend(later.transactions);
        self.next_offset = later.next_offset;
    }
}

/// The open gifts list so far.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GiftList {
    pub gifts: Vec<ReceivedGift>,
    pub total: i32,
    pub next_offset: String,
}

impl Page for GiftList {
    fn next_offset(&self) -> &str {
        &self.next_offset
    }

    fn append(&mut self, later: Self) {
        self.gifts.extend(later.gifts);
        self.total = later.total;
        self.next_offset = later.next_offset;
    }
}
