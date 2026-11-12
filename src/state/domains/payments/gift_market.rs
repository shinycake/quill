//! Reducer for the gift market (`crate::gift_market`): resale pages and
//! purchases, auction states and bids, crafting candidates and results.
use crate::gift_market::{
    MarketEvent,
    AuctionFetch, AuctionFollow, CraftGift, CraftResult, CraftRun, GiftMarketPayload,
    GiftMarketPurpose, PageLoad, bid_done_toast, error_note,
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
        let request = pending.map_or(crate::ids::RequestId(0), |p| p.id);
        let asked = pending.map(|p| p.id);
        match payload {
            GiftMarketPayload::ResalePage(page) => {
                let Some(GiftMarketPurpose::SearchResale { .. }) = purpose else {
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
                if let Some(search) = market.resale.as_mut().filter(|s| s.load.waits_for(request)) {
                    let append = search.load.appends(request);
                    search.apply(page, me, append);
                } else if let Some(search) = market
                    .craft
                    .as_mut()
                    .map(|craft| &mut craft.market)
                    .filter(|s| s.load.waits_for(request))
                {
                    let append = search.load.appends(request);
                    search.apply(page, me, append);
                }
            }
            GiftMarketPayload::AuctionState(state) => {
                self.remember_files(&state.gift.files);
                let market = &mut self.payments.gift_market;
                if request.0 != 0 && market.auction == (AuctionFetch::Loading { request }) {
                    market.auction = AuctionFetch::Ready {
                        gift_id: state.gift_id(),
                        request,
                    };
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
                if let Some(GiftMarketPurpose::AcquiredGifts { gift_id }) = purpose
                    && let Some(fetch) = self.payments.gift_market.acquired.get_mut(&gift_id)
                {
                    fetch.answered(asked, list);
                }
            }
            GiftMarketPayload::UpgradePreview(preview) => {
                if let Some(GiftMarketPurpose::UpgradePreview { gift_id }) = purpose
                    && self
                        .payments
                        .gift_market
                        .previews
                        .get(&gift_id)
                        .is_some_and(|fetch| asked.is_some_and(|id| fetch.waits_for(id)))
                {
                    let files: Vec<ParsedFile> = preview.files().cloned().collect();
                    self.remember_files(&files);
                    if let Some(fetch) = self.payments.gift_market.previews.get_mut(&gift_id) {
                        fetch.answered(asked, preview);
                    }
                }
            }
            GiftMarketPayload::CraftCandidates(candidates) => {
                let Some(GiftMarketPurpose::GiftsForCrafting { .. }) = purpose else {
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
                    .filter(|craft| craft.candidates_load.waits_for(request))
                else {
                    return;
                };
                let append = craft.candidates_load.appends(request);
                craft.candidates_load = PageLoad::Ready;
                craft.candidates_total = candidates.total_count;
                craft.candidates_offset = if candidates.gifts.is_empty() {
                    String::new()
                } else {
                    candidates.next_offset
                };
                craft.set_probabilities(candidates.probabilities);
                if append {
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
                // The box that sent this craft: one opened after it closed
                // shows nothing of it.
                if let Some(craft) = self
                    .payments
                    .gift_market
                    .craft
                    .as_mut()
                    .filter(|craft| craft.run == (CraftRun::Crafting { request }))
                {
                    craft.run = CraftRun::Done(result);
                }
            }
        }
    }

    /// Whether a `giftResaleResult*` answers the market's own purchase.
    pub(crate) fn is_gift_market_buy(&self, pending: Option<&PendingRequest>) -> bool {
        market_purpose(pending) == Some(GiftMarketPurpose::BuyResale)
    }

    /// The answer to the buy `request`, for the box that sent it only.
    pub(crate) fn apply_gift_market_purchase(
        &mut self,
        result: GiftPurchaseResult,
        request: Option<crate::ids::RequestId>,
    ) {
        let market = &mut self.payments.gift_market;
        let Some(buy) = market
            .buy
            .as_mut()
            .filter(|buy| request.is_some_and(|id| buy.waits_for(id)))
        else {
            return;
        };
        buy.send_failed();
        match result {
            GiftPurchaseResult::Sent(received_gift_id) => {
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
                    market.events.push(MarketEvent::Toast {
                        title: String::new(),
                        text: format!("{} is now yours.", listing.gift.full_name()),
                    });
                } else {
                    market.events.push(MarketEvent::Toast {
                        title: "Gift Sent!".into(),
                        text: format!("{} has been notified about your gift.", recipient.name),
                    });
                }
                market.buy = None;
            }
            GiftPurchaseResult::PriceIncreased(price) => {
                buy.price_increased(price);
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
                // Only the bid in flight settles it; another request's
                // answer leaves the button waiting.
                let id = pending.map(|p| p.id);
                if market
                    .bidding
                    .is_some_and(|(_, request)| Some(request) == id)
                {
                    market.bidding = None;
                }
                let per_round = market
                    .auctions
                    .get(&gift_id)
                    .map_or(1, |state| state.per_round());
                let increased = matches!(
                    market_purpose(pending),
                    Some(GiftMarketPurpose::IncreaseBid { .. })
                );
                let (title, text) = bid_done_toast(increased, per_round);
                market.events.push(MarketEvent::Toast {
                    title: title.into(),
                    text,
                });
            }
            Some(GiftMarketPurpose::Craft) => {
                let request = pending.map_or(crate::ids::RequestId(0), |p| p.id);
                if let Some(craft) = market
                    .craft
                    .as_mut()
                    .filter(|craft| craft.run == (CraftRun::Crafting { request }))
                {
                    craft.run = CraftRun::Idle;
                }
            }
            _ => {}
        }
    }

    /// `request` is the failed request's id: a buy or a craft fails only
    /// the box that sent it.
    pub(crate) fn apply_gift_market_error(
        &mut self,
        err: &TdError,
        purpose: GiftMarketPurpose,
        asked: Option<crate::ids::RequestId>,
    ) {
        let request = asked;
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
                    // Only the search that sent this request fails; a newer
                    // filter's search (or the other box's) keeps loading.
                    if request.is_some_and(|id| search.load.waits_for(id)) {
                        search.load = PageLoad::Failed(note.clone());
                    }
                }
            }
            GiftMarketPurpose::BuyResale => {
                let Some(buy) = market
                    .buy
                    .as_mut()
                    .filter(|buy| request.is_some_and(|id| buy.waits_for(id)))
                else {
                    return;
                };
                buy.send_failed();
                if low {
                    market.events.push(MarketEvent::BalanceLow);
                } else {
                    market.events.push(MarketEvent::Note(note));
                }
            }
            GiftMarketPurpose::AuctionState { .. } => {
                let Some(request) = request else {
                    return;
                };
                if market.auction != (AuctionFetch::Loading { request }) {
                    return;
                }
                market.auction = AuctionFetch::Idle;
                market.events.push(MarketEvent::Note(note));
            }
            GiftMarketPurpose::PlaceBid { .. } | GiftMarketPurpose::IncreaseBid { .. } => {
                if market
                    .bidding
                    .is_some_and(|(_, waiting)| request == Some(waiting))
                {
                    market.bidding = None;
                }
                if low {
                    market.events.push(MarketEvent::BalanceLow);
                } else {
                    market.events.push(MarketEvent::Note(note));
                }
            }
            GiftMarketPurpose::GiftsForCrafting { .. } => {
                let Some(craft) = market
                    .craft
                    .as_mut()
                    .filter(|craft| request.is_some_and(|id| craft.candidates_load.waits_for(id)))
                else {
                    return;
                };
                craft.candidates_load = PageLoad::Failed(note.clone());
                market.events.push(MarketEvent::Note(note));
            }
            GiftMarketPurpose::Craft => {
                let Some(craft) = market.craft.as_mut().filter(|craft| {
                    request.is_some_and(|request| craft.run == (CraftRun::Crafting { request }))
                }) else {
                    return;
                };
                craft.run = CraftRun::Idle;
                market.events.push(MarketEvent::Note(note));
            }
            GiftMarketPurpose::AcquiredGifts { gift_id } => {
                if let Some(fetch) = market.acquired.get_mut(&gift_id) {
                    fetch.failed(asked, error_reason(err), false);
                }
            }
            GiftMarketPurpose::OpenAuction => {
                // Only the follow that asked fails; the box shows no error
                // and does not ask again while it stays open.
                if let AuctionFollow::Following {
                    gift_id,
                    request: asked,
                } = market.follow
                    && request == Some(asked)
                {
                    market.follow = AuctionFollow::Failed(gift_id);
                }
            }
            GiftMarketPurpose::CloseAuction => {}
            GiftMarketPurpose::UpgradePreview { gift_id } => {
                if let Some(fetch) = market.previews.get_mut(&gift_id) {
                    fetch.failed(asked, error_reason(err), false);
                }
            }
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

    /// Takes the one-shot events (toasts, the low-balance prompt), oldest
    /// first.
    pub fn take_gift_market_events(&mut self) -> Vec<MarketEvent> {
        std::mem::take(&mut self.payments.gift_market.events)
    }
}
