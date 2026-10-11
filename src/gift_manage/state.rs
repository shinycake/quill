//! State of the gift management and send-gift dialogs. Lives in
//! `PaymentsState::gifts_manage`, so the poll loop and reducer own every
//! transition.

use super::{AvailableGift, GiftCollection, GiftLimits, GiftRecipient, PremiumGiftOption};

/// The inline form or confirmation that is open for the selected gift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiftFlow {
    /// Confirm `upgradeGift` (costs Stars unless prepaid).
    Upgrade {
        id: String,
        keep_details: bool,
    },
    /// Confirm `transferGift` to the open chat.
    Transfer {
        id: String,
    },
    /// Edit the resale price.
    Resale {
        id: String,
    },
    NewCollection,
    RenameCollection(i32),
    DeleteCollection(i32),
}

/// The send-gift dialog (`GiftBox` in tdesktop).
#[derive(Debug, Clone, Default)]
pub struct SendGiftState {
    pub open: bool,
    pub recipient: Option<GiftRecipient>,
    pub gifts: Vec<AvailableGift>,
    pub gifts_loading: bool,
    pub gifts_loaded: bool,
    pub selected: Option<i64>,
    pub premium_options: Vec<PremiumGiftOption>,
    pub premium_loaded: bool,
    pub sending: bool,
    pub error: Option<String>,
    pub done: Option<String>,
}

impl SendGiftState {
    pub fn selected_gift(&self) -> Option<&AvailableGift> {
        let id = self.selected?;
        self.gifts.iter().find(|g| g.id == id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GiftManageState {
    pub limits: GiftLimits,
    pub limits_asked: bool,
    pub collections: Vec<GiftCollection>,
    pub collections_loading: bool,
    pub collections_stale: bool,
    /// The collection tab that is open; 0 = all gifts.
    pub active_collection: i32,
    pub flow: Option<GiftFlow>,
    /// A one-line result of the last action ("Pinned", "Upgraded to ...").
    pub note: Option<String>,
    pub send: SendGiftState,
}

impl GiftManageState {
    pub fn collection(&self, id: i32) -> Option<&GiftCollection> {
        self.collections.iter().find(|c| c.id == id)
    }

    /// Forget the per-owner state when another gifts list opens.
    pub fn reset_for_owner(&mut self) {
        self.collections.clear();
        self.collections_loading = false;
        self.collections_stale = false;
        self.active_collection = 0;
        self.flow = None;
        self.note = None;
    }
}
