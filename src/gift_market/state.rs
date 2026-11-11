//! The session state of the gift market (`PaymentsState::gift_market`).

use super::{
    AcquiredGift, AuctionState, CraftGift, CraftResult, ResaleAttributes, ResaleFilter,
    ResaleListing, ResalePage, UpgradePreview,
};
use crate::gift_manage::GiftRecipient;
use crate::marketplace::GiftPrice;
use crate::state::fetch::Fetch;
use std::collections::HashMap;

/// One page request of a list in the gift market. The rows already loaded
/// stay under every stage (a failed next page keeps the list).
///
/// `Loading` leaves on its own answer (`Ready`), its error (`Failed`), or a
/// send that did not leave (`Failed`); `Failed` leaves on a retry; the
/// list itself going away ends it. An answer for any other request id is
/// dropped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageLoad {
    /// Never asked.
    #[default]
    Idle,
    /// The page is out as `request`. `more`: it extends a list that already
    /// had an answer.
    Loading {
        request: u64,
        more: bool,
    },
    Ready,
    /// The page was refused or did not leave; the text is drawn with a
    /// Retry.
    Failed(String),
}

impl PageLoad {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// Whether a page of this list was answered, well or not (a next page
    /// in flight extends such a list).
    pub fn answered(&self) -> bool {
        match self {
            Self::Idle => false,
            Self::Loading { more, .. } => *more,
            Self::Ready | Self::Failed(_) => true,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// The page out as `request`, if it is this one.
    pub fn waits_for(&self, request: u64) -> bool {
        request != 0 && matches!(self, Self::Loading { request: asked, .. } if *asked == request)
    }

    /// The page out as `request` extends the list instead of replacing it.
    pub fn appends(&self, request: u64) -> bool {
        request != 0
            && matches!(self, Self::Loading { request: asked, more: true } if *asked == request)
    }

    /// A page went out as `request`: a next page extends the list.
    pub fn begin(&mut self, request: u64, more: bool) {
        *self = Self::Loading { request, more };
    }

    /// The page just asked did not leave.
    pub fn send_failed(&mut self, reason: String) {
        *self = Self::Failed(reason);
    }
}

/// The craft in flight and its outcome, one stage at a time. `Crafting`
/// leaves on its answer (`Done`, or `Idle` on an error); `Done` leaves
/// when the box is dismissed or crafts again.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CraftRun {
    #[default]
    Idle,
    /// `craftGift` is out as `request` (0: the demo's), whose answer this
    /// box shows.
    Crafting {
        request: u64,
    },
    Done(CraftResult),
}

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
    /// The newest page request of this filter.
    pub load: PageLoad,
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

    /// Whether another page can be asked for. A failed next page still
    /// offers Show more, so asking again retries it; a failed first page has
    /// no offset and offers nothing.
    pub fn has_more(&self) -> bool {
        matches!(self.load, PageLoad::Ready | PageLoad::Failed(_)) && !self.next_offset.is_empty()
    }

    /// Takes a page: the first replaces the list, later ones append. Own
    /// listings are dropped in craft mode (tdesktop skips them).
    pub fn apply(&mut self, page: ResalePage, my_user_id: Option<i64>, append: bool) {
        self.load = PageLoad::Ready;
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

/// Buying one resale listing: the listing, who gets it, the currency the
/// buyer picked and the one stage the box is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResaleBuy {
    pub listing: ResaleListing,
    pub recipient: GiftRecipient,
    /// The currency tab the buyer picked (a choice that outlives a refused
    /// purchase).
    pub pay_in_grams: bool,
    /// Where the box is ([`BuyStage`]).
    pub stage: BuyStage,
    /// Bought for a craft slot: the gift lands there instead of a toast.
    pub for_craft: bool,
}

/// The box of buying a listing, one stage at a time: the three steps, a
/// price TDLib changed under the buyer, and the purchase in flight.
///
/// `Sending` leaves on its own answer (the box goes on
/// `giftResaleResultOk`, back to `Confirm` for a refusal, to `Changed` for a
/// price change) or its error; only that answer applies here, not one for a
/// box closed before it came. The steps leave on the buyer's Back and Buy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuyStage {
    #[default]
    View,
    Send,
    Confirm,
    /// The purchase was refused with a new price ("Price change!"); the
    /// Confirm step shows it and the next Buy sends at it.
    Changed(GiftPrice),
    /// The purchase is out as `request`; the Confirm step shows "Buying".
    Sending {
        request: crate::ids::RequestId,
    },
}

impl ResaleBuy {
    pub fn new(listing: ResaleListing, recipient: GiftRecipient, for_craft: bool) -> Self {
        let pay_in_grams = listing.gift.resale.is_some_and(|resale| resale.gram_only);
        Self {
            listing,
            recipient,
            pay_in_grams,
            stage: BuyStage::View,
            for_craft,
        }
    }

    /// The step the box draws.
    pub fn step(&self) -> BuyStep {
        match self.stage {
            BuyStage::View => BuyStep::View,
            BuyStage::Send => BuyStep::Send,
            BuyStage::Confirm | BuyStage::Changed(_) | BuyStage::Sending { .. } => BuyStep::Confirm,
        }
    }

    /// Moves to `step`. A purchase in flight keeps its stage, and the
    /// Confirm step keeps a price change it already shows.
    pub fn go(&mut self, step: BuyStep) {
        self.stage = match (self.stage, step) {
            (BuyStage::Sending { request }, _) => BuyStage::Sending { request },
            (BuyStage::Changed(price), BuyStep::Confirm) => BuyStage::Changed(price),
            (_, BuyStep::View) => BuyStage::View,
            (_, BuyStep::Send) => BuyStage::Send,
            (_, BuyStep::Confirm) => BuyStage::Confirm,
        };
    }

    /// The buyer picked a currency; a changed price no longer applies.
    pub fn choose_grams(&mut self, grams: bool) {
        let gram_only = self.listing.gift.resale.is_some_and(|r| r.gram_only);
        self.pay_in_grams = grams || gram_only;
        if matches!(self.stage, BuyStage::Changed(_)) {
            self.stage = BuyStage::Confirm;
        }
    }

    /// The price TDLib reported after a refused purchase, if the box shows
    /// one.
    pub fn price_changed(&self) -> Option<GiftPrice> {
        match self.stage {
            BuyStage::Changed(price) => Some(price),
            _ => None,
        }
    }

    /// The purchase is out and unanswered.
    pub fn sending(&self) -> bool {
        matches!(self.stage, BuyStage::Sending { .. })
    }

    /// `request` is the purchase this box sent.
    pub fn waits_for(&self, request: u64) -> bool {
        request != 0
            && self.stage
                == (BuyStage::Sending {
                    request: crate::ids::RequestId(request),
                })
    }

    /// The purchase went out as `request`.
    pub fn begin_send(&mut self, request: u64) {
        self.stage = BuyStage::Sending {
            request: crate::ids::RequestId(request),
        };
    }

    /// The purchase did not happen (refused, or it never left): back to the
    /// Confirm step.
    pub fn send_failed(&mut self) {
        if self.sending() {
            self.stage = BuyStage::Confirm;
        }
    }

    /// The purchase was refused for a new price: the Confirm step shows it
    /// (none: plain Confirm), and a Grams price moves the buyer to Grams.
    pub fn price_increased(&mut self, price: Option<GiftPrice>) {
        self.stage = match price {
            Some(price) => BuyStage::Changed(price),
            None => BuyStage::Confirm,
        };
        if let Some(GiftPrice::TonCents(_)) = price {
            self.pay_in_grams = true;
        }
    }

    /// The price the buy sends.
    pub fn price(&self) -> Option<GiftPrice> {
        self.price_changed()
            .or_else(|| super::listing_price(&self.listing.gift, self.pay_in_grams))
    }
}

/// tdesktop's fallback attribute persistence table (per mille), used
/// until TDLib sends one: row `n - 1` is for `n` gifts in the craft.
pub fn default_craft_probabilities() -> Vec<Vec<i32>> {
    vec![
        vec![90],
        vec![80, 200],
        vec![70, 190, 460],
        vec![60, 180, 450, 1000],
    ]
}

/// The craft box: the chosen gifts, the candidates and the outcome.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CraftSession {
    pub regular_gift_id: i64,
    pub slots: Vec<CraftGift>,
    pub candidates: Vec<CraftGift>,
    pub candidates_total: i32,
    pub candidates_offset: String,
    /// The newest page request of the user's own candidate gifts.
    pub candidates_load: PageLoad,
    /// `attribute_persistence_probabilities`.
    pub probabilities: Vec<Vec<i32>>,
    /// Suitable listings on sale (resale in craft mode).
    pub market: ResaleSearch,
    /// The craft in flight and its outcome ([`CraftRun`]).
    pub run: CraftRun,
}

impl CraftSession {
    pub fn new(primary: CraftGift) -> Self {
        let regular_gift_id = primary.gift.regular_gift_id;
        Self {
            regular_gift_id,
            slots: vec![primary],
            probabilities: default_craft_probabilities(),
            market: ResaleSearch::new(regular_gift_id, ResaleFilter::default(), true),
            ..Self::default()
        }
    }

    /// Takes TDLib's `attribute_persistence_probabilities` when it sent a
    /// table; an empty one keeps tdesktop's default.
    pub fn set_probabilities(&mut self, table: Vec<Vec<i32>>) {
        if !table.is_empty() && table.iter().all(|row| !row.is_empty()) {
            self.probabilities = table;
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

    /// Fresh lists for the next craft: the candidates and the market
    /// search start again, and an answer to a page asked before is dropped
    /// by its request id ([`PageLoad::waits_for`]).
    pub fn reset_lists(&mut self) {
        self.candidates.clear();
        self.candidates_total = 0;
        self.candidates_load = PageLoad::Idle;
        self.candidates_offset.clear();
        self.candidates_page = 0;
        self.market = ResaleSearch::new(self.regular_gift_id, ResaleFilter::default(), true);
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

    /// A craft is out and unanswered.
    pub fn crafting(&self) -> bool {
        matches!(self.run, CraftRun::Crafting { .. })
    }

    /// The outcome of the last craft.
    pub fn result(&self) -> Option<&CraftResult> {
        match &self.run {
            CraftRun::Done(result) => Some(result),
            _ => None,
        }
    }

    /// The outcome is gone (dismissed, or a new craft starts).
    pub fn clear_result(&mut self) {
        if matches!(self.run, CraftRun::Done(_)) {
            self.run = CraftRun::Idle;
        }
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
    /// "N items bought" per auction: a failed list says so and offers Retry.
    pub acquired: HashMap<i64, Fetch<Vec<AcquiredGift>>>,
    /// The upgrade preview per gift (the cover cycle, the craftable models).
    pub previews: HashMap<i64, Fetch<UpgradePreview>>,
    /// The auction `openGiftAuction` follows.
    pub follow: AuctionFollow,
    /// The `getGiftAuctionState` the UI waits for (a link or a tile).
    pub auction: AuctionFetch,
    /// The bid in flight: its gift and the request that answers it.
    pub bidding: Option<(i64, crate::ids::RequestId)>,
    pub craft: Option<CraftSession>,
    /// One-shot error toasts.
    pub notes: Vec<String>,
    /// One-shot success toasts: title and text.
    pub toasts: Vec<(String, String)>,
    /// A paid request was refused for too few Stars.
    pub balance_low: bool,
}

/// The `getGiftAuctionState` the UI waits for. Its answer and its error
/// count only while they carry the request asked last; an error is a toast
/// and ends the stage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuctionFetch {
    #[default]
    Idle,
    Loading {
        request: u64,
    },
    /// The state of this gift arrived as the answer to `request`; the box
    /// that asked takes it once, and one that did not ask leaves it.
    Ready {
        gift_id: i64,
        request: u64,
    },
}

/// The auction a bid box follows (`openGiftAuction`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuctionFollow {
    #[default]
    None,
    /// The open request went out as `request`; updates arrive for it.
    Following { gift_id: i64, request: u64 },
    /// The open request was refused or could not be sent. It is not asked
    /// again while the same box shows (that would be a request every
    /// frame); closing the box and opening it again follows afresh.
    Failed(i64),
}

impl AuctionFollow {
    /// The auction this follow is for, followed or failed.
    pub fn gift_id(self) -> Option<i64> {
        match self {
            Self::None => None,
            Self::Following { gift_id, .. } | Self::Failed(gift_id) => Some(gift_id),
        }
    }

    pub fn is_following(self, gift_id: i64) -> bool {
        matches!(self, Self::Following { gift_id: id, .. } if id == gift_id)
    }
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
