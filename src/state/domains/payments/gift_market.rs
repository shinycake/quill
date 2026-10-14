//! Reducer for the gift market (`crate::gift_market`): resale pages and
//! purchases, auction states and bids, crafting candidates and results.
use crate::gift_market::{
    CraftGift, CraftResult, GiftMarketPayload, GiftMarketPurpose, bid_done_toast, error_note,
};
use crate::marketplace::GiftPurchaseResult;
use crate::state::*;

fn market_purpose(pending: Option<&PendingRequest>) -> Option<GiftMarketPurpose> {
    match pending.map(|p| p.purpose) {
        Some(RequestPurpose::Payments(PaymentsPurpose::GiftMarket(purpose))) => Some(purpose),
        _ => None,
    }
}

impl Session {
    pub(crate) fn apply_gift_market(
        &mut self,
        payload: GiftMarketPayload,
        pending: Option<&PendingRequest>,
    ) {
        let purpose = market_purpose(pending);
        let request = pending.map_or(0, |p| p.id.0);
        match payload {
            GiftMarketPayload::ResalePage(page) => {
                let Some(GiftMarketPurpose::SearchResale { page: index, .. }) = purpose else {
                    return;
                };
                let files: Vec<ParsedFile> = page
                    .listings
                    .iter()
                    .flat_map(|listing| listing.gift.files().cloned())
                    .chain(page.attributes.files().cloned())
                    .collect();
                self.remember_files(&files);
                let market = &mut self.payments.gift_market;
                for listing in &page.listings {
                    market.remember_title(listing.gift.regular_gift_id, &listing.gift.title);
                }
                let me = self.my_user_id;
                let market = &mut self.payments.gift_market;
                if let Some(search) = market.resale.as_mut().filter(|s| s.request_id == request) {
                    search.apply(page, me, index > 0);
                } else if let Some(search) = market
                    .craft
                    .as_mut()
                    .map(|craft| &mut craft.market)
                    .filter(|s| s.request_id == request)
                {
                    search.apply(page, me, index > 0);
                }
            }
            GiftMarketPayload::AuctionState(state) => {
                self.remember_files(&state.gift.files);
                let market = &mut self.payments.gift_market;
                if request != 0 && market.auction_request == request {
                    market.auction_loading = false;
                    market.auction_ready = Some(state.gift_id());
                }
                market.store_auction(*state);
            }
            GiftMarketPayload::AuctionUpdated(state) => {
                self.remember_files(&state.gift.files);
                self.payments.gift_market.store_auction(*state);
            }
            GiftMarketPayload::ActiveAuctions(list) => {
                let files: Vec<ParsedFile> = list
                    .iter()
                    .flat_map(|state| state.gift.files.iter().cloned())
                    .collect();
                self.remember_files(&files);
                let market = &mut self.payments.gift_market;
                for state in &list {
                    market.auctions.insert(state.gift_id(), state.clone());
                }
                market.active = list;
            }
            GiftMarketPayload::AcquiredGifts(list) => {
                if let Some(GiftMarketPurpose::AcquiredGifts { gift_id }) = purpose {
                    self.payments.gift_market.acquired.insert(gift_id, list);
                }
            }
            GiftMarketPayload::UpgradePreview(preview) => {
                if let Some(GiftMarketPurpose::UpgradePreview { gift_id }) = purpose {
                    let files: Vec<ParsedFile> = preview.files().cloned().collect();
                    self.remember_files(&files);
                    self.payments.gift_market.previews.insert(gift_id, preview);
                }
            }
            GiftMarketPayload::CraftCandidates(candidates) => {
                let Some(GiftMarketPurpose::GiftsForCrafting { page, .. }) = purpose else {
                    return;
                };
                let files: Vec<ParsedFile> = candidates
                    .gifts
                    .iter()
                    .flat_map(|gift| gift.gift.files().cloned())
                    .collect();
                self.remember_files(&files);
                let Some(craft) = self
                    .payments
                    .gift_market
                    .craft
                    .as_mut()
                    .filter(|craft| craft.candidates_request == request)
                else {
                    return;
                };
                craft.candidates_loading = false;
                craft.candidates_loaded = true;
                craft.candidates_total = candidates.total_count;
                craft.candidates_offset = if candidates.gifts.is_empty() {
                    String::new()
                } else {
                    candidates.next_offset
                };
                craft.set_probabilities(candidates.probabilities);
                if page > 0 {
                    craft.candidates.extend(candidates.gifts);
                } else {
                    craft.candidates = candidates.gifts;
                }
            }
            GiftMarketPayload::CraftResult(result) => {
                if purpose != Some(GiftMarketPurpose::Craft) {
                    return;
                }
                if let CraftResult::Success { gift, .. } = &result {
                    let files: Vec<ParsedFile> = gift.files().cloned().collect();
                    self.remember_files(&files);
                }
                if let Some(craft) = self.payments.gift_market.craft.as_mut() {
                    craft.crafting = false;
                    craft.result = Some(result);
                }
            }
        }
    }

    /// Whether a `giftResaleResult*` answers the market's own purchase.
    pub(crate) fn is_gift_market_buy(&self, pending: Option<&PendingRequest>) -> bool {
        market_purpose(pending) == Some(GiftMarketPurpose::BuyResale)
    }

    pub(crate) fn apply_gift_market_purchase(&mut self, result: GiftPurchaseResult) {
        let market = &mut self.payments.gift_market;
        let Some(buy) = market.buy.as_mut() else {
            return;
        };
        buy.sending = false;
        match result {
            GiftPurchaseResult::Sent(received_gift_id) => {
                buy.done = Some(received_gift_id.clone());
                let listing = buy.listing.clone();
                let recipient = buy.recipient.clone();
                let for_craft = buy.for_craft;
                if let Some(search) = market.resale.as_mut() {
                    search
                        .listings
                        .retain(|entry| entry.gift.name != listing.gift.name);
                    search.total_count = search.total_count.saturating_sub(1);
                }
                if for_craft && let Some(craft) = market.craft.as_mut() {
                    craft
                        .market
                        .listings
                        .retain(|entry| entry.gift.name != listing.gift.name);
                    let mut gift = listing.gift;
                    gift.resale = None;
                    craft.add(CraftGift {
                        gift,
                        received_gift_id,
                        craft_date: 0,
                    });
                } else if recipient.is_self {
                    market.toasts.push((
                        String::new(),
                        format!("{} is now yours.", listing.gift.full_name()),
                    ));
                } else {
                    market.toasts.push((
                        "Gift Sent!".into(),
                        format!("{} has been notified about your gift.", recipient.name),
                    ));
                }
                market.buy = None;
            }
            GiftPurchaseResult::PriceIncreased(price) => {
                buy.price_changed = price;
                if let Some(crate::marketplace::GiftPrice::TonCents(_)) = price {
                    buy.pay_in_grams = true;
                }
            }
        }
    }

    /// `ok` for a bid: the toast; the state itself comes as an update.
    pub(crate) fn apply_gift_market_ok(&mut self, pending: Option<&PendingRequest>) {
        let market = &mut self.payments.gift_market;
        match market_purpose(pending) {
            Some(
                GiftMarketPurpose::PlaceBid { gift_id }
                | GiftMarketPurpose::IncreaseBid { gift_id },
            ) => {
                market.bidding = None;
                let per_round = market
                    .auctions
                    .get(&gift_id)
                    .map_or(1, |state| state.per_round());
                let increased = matches!(
                    market_purpose(pending),
                    Some(GiftMarketPurpose::IncreaseBid { .. })
                );
                let (title, text) = bid_done_toast(increased, per_round);
                market.toasts.push((title.into(), text));
            }
            Some(GiftMarketPurpose::Craft) => {
                if let Some(craft) = market.craft.as_mut() {
                    craft.crafting = false;
                }
            }
            _ => {}
        }
    }

    pub(crate) fn apply_gift_market_error(&mut self, err: &TdError, purpose: GiftMarketPurpose) {
        let market = &mut self.payments.gift_market;
        let note = error_note(purpose, &error_reason(err));
        let low = err.earn_hint == Some(crate::earn::EarnErrorHint::BalanceTooLow);
        match purpose {
            GiftMarketPurpose::SearchResale { .. } => {
                for search in market
                    .resale
                    .iter_mut()
                    .chain(market.craft.as_mut().map(|craft| &mut craft.market))
                {
                    if search.loading {
                        search.loading = false;
                        search.loaded = true;
                        search.error = Some(note.clone());
                    }
                }
            }
            GiftMarketPurpose::BuyResale => {
                if let Some(buy) = market.buy.as_mut() {
                    buy.sending = false;
                }
                if low {
                    market.balance_low = true;
                } else {
                    market.notes.push(note);
                }
            }
            GiftMarketPurpose::AuctionState { .. } => {
                market.auction_loading = false;
                market.notes.push(note);
            }
            GiftMarketPurpose::PlaceBid { .. } | GiftMarketPurpose::IncreaseBid { .. } => {
                market.bidding = None;
                if low {
                    market.balance_low = true;
                } else {
                    market.notes.push(note);
                }
            }
            GiftMarketPurpose::GiftsForCrafting { .. } => {
                if let Some(craft) = market.craft.as_mut() {
                    craft.candidates_loading = false;
                    craft.candidates_loaded = true;
                }
                market.notes.push(note);
            }
            GiftMarketPurpose::Craft => {
                if let Some(craft) = market.craft.as_mut() {
                    craft.crafting = false;
                }
                market.notes.push(note);
            }
            GiftMarketPurpose::OpenAuction
            | GiftMarketPurpose::CloseAuction
            | GiftMarketPurpose::AcquiredGifts { .. }
            | GiftMarketPurpose::UpgradePreview { .. } => {}
        }
    }

    /// The collection title of a regular gift: from the send list, then
    /// from listings seen.
    pub fn gift_market_title(&self, gift_id: i64) -> Option<String> {
        self.payments
            .gifts_manage
            .send
            .gifts
            .iter()
            .find(|gift| gift.id == gift_id && !gift.title.is_empty())
            .map(|gift| gift.title.clone())
            .or_else(|| self.payments.gift_market.title(gift_id).map(str::to_string))
    }

    /// Takes the one-shot error toasts.
    pub fn take_gift_market_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.payments.gift_market.notes)
    }

    /// Takes the one-shot success toasts (title, text).
    pub fn take_gift_market_toasts(&mut self) -> Vec<(String, String)> {
        std::mem::take(&mut self.payments.gift_market.toasts)
    }

    /// Takes the "not enough Stars" flag.
    pub fn take_gift_market_balance_low(&mut self) -> bool {
        std::mem::take(&mut self.payments.gift_market.balance_low)
    }
}
