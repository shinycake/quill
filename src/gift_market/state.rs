//! The session state of the gift market (`PaymentsState::gift_market`).

use super::{
    AcquiredGift, AuctionState, CraftGift, CraftResult, ResaleAttributes, ResaleFilter,
    ResaleListing, ResalePage, UpgradePreview,
};
use crate::gift_manage::GiftRecipient;
use crate::marketplace::GiftPrice;
use std::collections::HashMap;

/// One resale search: the resale box, or the market list of the craft
/// box's "Select Gifts".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResaleSearch {
    pub gift_id: i64,
    pub filter: ResaleFilter,
    pub for_crafting: bool,
    pub total_count: i32,
    pub listings: Vec<ResaleListing>,
    /// The attribute lists, kept from the first unfiltered page.
    pub attributes: ResaleAttributes,
    pub next_offset: String,
    /// The next page to ask for.
    pub page: u32,
    pub loading: bool,
    /// Whether any page arrived for the current filter.
    pub loaded: bool,
    /// The request whose answer this search waits for.
    pub request_id: u64,
    pub error: Option<String>,
}

impl ResaleSearch {
    pub fn new(gift_id: i64, filter: ResaleFilter, for_crafting: bool) -> Self {
        Self {
            gift_id,
            filter,
            for_crafting,
            ..Self::default()
        }
    }

    /// Whether another page can be asked for.
    pub fn has_more(&self) -> bool {
        self.loaded && !self.loading && !self.next_offset.is_empty()
    }

    /// Takes a page: the first replaces the list, later ones append. Own
    /// listings are dropped in craft mode (tdesktop skips them).
    pub fn apply(&mut self, page: ResalePage, my_user_id: Option<i64>, append: bool) {
        self.loading = false;
        self.loaded = true;
        self.error = None;
        self.total_count = page.total_count;
        self.next_offset = if page.listings.is_empty() {
            String::new()
        } else {
            page.next_offset
        };
        if !page.attributes.is_empty() {
            self.attributes = page.attributes;
        }
        let crafting = self.for_crafting;
        let listings = page.listings.into_iter().filter(|listing| {
            let mine = listing.gift.owner.as_ref().is_some_and(|owner| {
                matches!(owner, crate::telegram::envelope::MessageSender::User { user_id } if Some(*user_id) == my_user_id)
            });
            !(crafting && mine)
        });
        if append {
            self.listings.extend(listings);
        } else {
            self.listings = listings.collect();
        }
    }
}

/// The three steps of buying a listing (tdesktop
/// `ShowBuyResaleGiftBox`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuyStep {
    /// The collectible with its attributes and "Buy for ★N".
    #[default]
    View,
    /// "Send a Gift": message and "Hide My Name".
    Send,
    /// "Pay in Stars" / "Pay in Grams" and the final question.
    Confirm,
}

/// Buying one resale listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResaleBuy {
    pub listing: ResaleListing,
    pub recipient: GiftRecipient,
    pub step: BuyStep,
    pub pay_in_grams: bool,
    pub sending: bool,
    /// The price TDLib reported after a refused purchase ("Price change!").
    pub price_changed: Option<GiftPrice>,
    /// Bought for a craft slot: the gift lands there instead of a toast.
    pub for_craft: bool,
    /// The bought gift's id once `giftResaleResultOk` arrives.
    pub done: Option<String>,
}

impl ResaleBuy {
    pub fn new(listing: ResaleListing, recipient: GiftRecipient, for_craft: bool) -> Self {
        let pay_in_grams = listing.gift.resale.is_some_and(|resale| resale.gram_only);
        Self {
            listing,
            recipient,
            step: BuyStep::View,
            pay_in_grams,
            sending: false,
            price_changed: None,
            for_craft,
            done: None,
        }
    }

    /// The price the buy sends.
    pub fn price(&self) -> Option<GiftPrice> {
        self.price_changed
            .or_else(|| super::listing_price(&self.listing.gift, self.pay_in_grams))
    }
}

/// The craft box: the chosen gifts, the candidates and the outcome.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CraftSession {
    pub regular_gift_id: i64,
    pub slots: Vec<CraftGift>,
    pub candidates: Vec<CraftGift>,
    pub candidates_total: i32,
    pub candidates_offset: String,
    pub candidates_page: u32,
    pub candidates_loading: bool,
    pub candidates_loaded: bool,
    pub candidates_request: u64,
    /// `attribute_persistence_probabilities`.
    pub probabilities: Vec<Vec<i32>>,
    /// Suitable listings on sale (resale in craft mode).
    pub market: ResaleSearch,
    pub crafting: bool,
    pub result: Option<CraftResult>,
}

impl CraftSession {
    pub fn new(primary: CraftGift) -> Self {
        let regular_gift_id = primary.gift.regular_gift_id;
        Self {
            regular_gift_id,
            slots: vec![primary],
            market: ResaleSearch::new(regular_gift_id, ResaleFilter::default(), true),
            ..Self::default()
        }
    }

    /// Candidates not already in a slot.
    pub fn free_candidates(&self) -> impl Iterator<Item = &CraftGift> {
        self.candidates.iter().filter(|candidate| {
            !self
                .slots
                .iter()
                .any(|slot| slot.received_gift_id == candidate.received_gift_id)
        })
    }

    /// Adds a gift to the first free slot; `false` when full or present.
    pub fn add(&mut self, gift: CraftGift) -> bool {
        if self.slots.len() >= super::CRAFT_SLOTS
            || self
                .slots
                .iter()
                .any(|slot| slot.received_gift_id == gift.received_gift_id)
        {
            return false;
        }
        self.slots.push(gift);
        true
    }

    /// Removes a slot other than the primary gift.
    pub fn remove(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.slots.len() {
            return false;
        }
        self.slots.remove(index);
        true
    }

    pub fn success_per_mille(&self) -> i32 {
        super::success_per_mille(&self.slots)
    }
}

/// Everything the market screens read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GiftMarketState {
    /// Collection titles by regular gift id: a bare `gift` has none, so
    /// they come from `availableGift.title` and listings.
    pub titles: HashMap<i64, String>,
    pub resale: Option<ResaleSearch>,
    pub buy: Option<ResaleBuy>,
    /// Auctions by gift id, kept fresh by `updateGiftAuctionState`.
    pub auctions: HashMap<i64, AuctionState>,
    /// The auctions with a bid of the user (`updateActiveGiftAuctions`).
    pub active: Vec<AuctionState>,
    pub acquired: HashMap<i64, Vec<AcquiredGift>>,
    pub previews: HashMap<i64, UpgradePreview>,
    /// The auction `openGiftAuction` follows.
    pub followed: Option<i64>,
    /// A `getGiftAuctionState` the UI waits for (a link or a tile) and
    /// the gift it resolved to.
    pub auction_request: u64,
    pub auction_ready: Option<i64>,
    pub auction_loading: bool,
    /// A bid in flight, by gift id.
    pub bidding: Option<i64>,
    pub craft: Option<CraftSession>,
    /// One-shot error toasts.
    pub notes: Vec<String>,
    /// One-shot success toasts: title and text.
    pub toasts: Vec<(String, String)>,
    /// A paid request was refused for too few Stars.
    pub balance_low: bool,
}

impl GiftMarketState {
    /// The collection title of a regular gift, if known.
    pub fn title(&self, gift_id: i64) -> Option<&str> {
        self.titles
            .get(&gift_id)
            .map(String::as_str)
            .filter(|title| !title.is_empty())
    }

    pub fn remember_title(&mut self, gift_id: i64, title: &str) {
        if gift_id != 0 && !title.is_empty() {
            self.titles.insert(gift_id, title.to_string());
        }
    }

    /// Stores an auction state; an active auction with a user bid also
    /// refreshes its entry in the chat-list bar.
    pub fn store_auction(&mut self, state: AuctionState) {
        let id = state.gift_id();
        if let Some(entry) = self.active.iter_mut().find(|entry| entry.gift_id() == id) {
            if state.my_stars() > 0 && state.active().is_some() {
                *entry = state.clone();
            } else {
                self.active.retain(|entry| entry.gift_id() != id);
            }
        }
        self.auctions.insert(id, state);
    }

    /// The bar's list: auctions still running.
    pub fn active_auctions(&self, now: i64) -> Vec<AuctionState> {
        self.active
            .iter()
            .filter(|state| !state.finished(now) && state.my_stars() > 0)
            .cloned()
            .collect()
    }
}
