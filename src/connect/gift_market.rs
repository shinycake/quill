//! Connect driver: the gift market (`crate::gift_market`). Resale search
//! and purchase, auction state, following and bids, crafting candidates
//! and the craft itself. The ones that spend Stars (`sendResoldGift`,
//! `placeGiftAuctionBid`, `increaseGiftAuctionBid`, `craftGift`) are only
//! called from a button that names the price or the gifts.
use super::*;
use crate::gift_market::{
    CraftGift, CraftSession, GiftMarketPurpose, ResaleBuy, ResaleFilter, ResaleSearch,
};
use crate::ids::{ChatId, RequestId};
use crate::local_time::now_unix;
use crate::marketplace::GiftPrice;
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests as rq;

impl<S: JsonSender> ConnectDriver<S> {
    fn send_market(
        &mut self,
        purpose: GiftMarketPurpose,
        json: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GiftMarket(purpose)),
            None,
        );
        self.send_json_request(extra, &json(extra))
    }

    fn market_balance_covers(&self, stars: i64) -> bool {
        self.session
            .payments
            .hub
            .balance
            .is_none_or(|balance| balance.stars >= stars)
    }

    // ---- Resale -------------------------------------------------------

    /// Opens the resale list of a regular gift with the default filter.
    pub fn open_gift_resale(
        &mut self,
        gift_id: i64,
        title: &str,
    ) -> Result<RequestId, ConnectSendError> {
        if gift_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let market = &mut self.session.payments.gift_market;
        market.remember_title(gift_id, title);
        market.resale = Some(ResaleSearch::new(gift_id, ResaleFilter::default(), false));
        market.buy = None;
        self.search_resale_page(false, false)
    }

    /// Applies a new sort, payment filter or attribute pick and reloads.
    pub fn set_gift_resale_filter(
        &mut self,
        filter: ResaleFilter,
    ) -> Result<RequestId, ConnectSendError> {
        let search = self
            .session
            .payments
            .gift_market
            .resale
            .as_mut()
            .ok_or(ConnectSendError::InvalidRequest)?;
        if search.filter == filter {
            return Err(ConnectSendError::InvalidRequest);
        }
        search.filter = filter;
        self.search_resale_page(false, false)
    }

    /// The next page of the resale list (`craft` = the craft box's list).
    pub fn load_more_gift_resale(&mut self, craft: bool) -> Result<RequestId, ConnectSendError> {
        let market = &self.session.payments.gift_market;
        let search = if craft {
            market.craft.as_ref().map(|craft| &craft.market)
        } else {
            market.resale.as_ref()
        };
        if !search.is_some_and(ResaleSearch::has_more) {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.search_resale_page(craft, true)
    }

    fn search_resale_page(
        &mut self,
        craft: bool,
        more: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let market = &mut self.session.payments.gift_market;
        let search = if craft {
            market.craft.as_mut().map(|craft| &mut craft.market)
        } else {
            market.resale.as_mut()
        }
        .ok_or(ConnectSendError::InvalidRequest)?;
        if !more {
            search.page = 0;
            search.next_offset.clear();
            search.loaded = false;
        }
        search.loading = true;
        search.error = None;
        let (gift_id, page) = (search.gift_id, search.page);
        let filter = search.filter.clone();
        let offset = search.next_offset.clone();
        let for_crafting = search.for_crafting;
        let result = self.send_market(GiftMarketPurpose::SearchResale { gift_id, page }, |extra| {
            rq::search_gifts_for_resale(extra, gift_id, &filter, for_crafting, &offset)
        });
        let market = &mut self.session.payments.gift_market;
        let search = if craft {
            market.craft.as_mut().map(|craft| &mut craft.market)
        } else {
            market.resale.as_mut()
        };
        if let Some(search) = search {
            match &result {
                Ok(extra) => {
                    search.request_id = extra.0;
                    search.page = page.saturating_add(1);
                }
                Err(_) => search.loading = false,
            }
        }
        result
    }

    /// Starts buying the listing named `name` for the user or channel of
    /// `chat_id`, or for the current user (`None`, and always for a craft).
    pub fn start_gift_resale_buy(
        &mut self,
        name: &str,
        chat_id: Option<ChatId>,
        for_craft: bool,
    ) -> Result<(), ConnectSendError> {
        let market = &self.session.payments.gift_market;
        let listing = market
            .resale
            .iter()
            .chain(market.craft.as_ref().map(|craft| &craft.market))
            .flat_map(|search| search.listings.iter())
            .find(|listing| listing.gift.name == name && !listing.is_mine())
            .cloned()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let me = self
            .session
            .my_user_id
            .ok_or(ConnectSendError::InvalidRequest)?;
        let recipient = match chat_id.filter(|_| !for_craft) {
            Some(chat_id) => self
                .session
                .gift_recipient(chat_id)
                .ok_or(ConnectSendError::InvalidRequest)?,
            None => crate::gift_manage::GiftRecipient {
                sender: MessageSender::User { user_id: me },
                name: String::new(),
                is_self: true,
                is_channel: false,
            },
        };
        self.session.payments.gift_market.buy = Some(ResaleBuy::new(listing, recipient, for_craft));
        Ok(())
    }

    /// `sendResoldGift` at the price the confirm step showed.
    pub fn buy_gift_resale(
        &mut self,
        price: GiftPrice,
        text: &str,
        hide_name: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let buy = self
            .session
            .payments
            .gift_market
            .buy
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let limit = self.session.payments.gift_text_length_max;
        let text = text.trim();
        if buy.sending
            || buy.done.is_some()
            || buy.price() != Some(price)
            || !price.valid()
            || (!text.is_empty() && limit.is_some_and(|max| text.chars().count() > max))
            || matches!(price, GiftPrice::Stars(stars) if !self.market_balance_covers(stars))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let name = buy.listing.gift.name.clone();
        let owner = buy.recipient.sender;
        let result = self.send_market(GiftMarketPurpose::BuyResale, |extra| {
            crate::telegram::requests_payments::send_marketplace_gift(
                extra, &name, owner, price, text, hide_name,
            )
        });
        if let Some(buy) = self.session.payments.gift_market.buy.as_mut() {
            buy.sending = result.is_ok();
            if result.is_ok() {
                buy.price_changed = None;
            }
        }
        result
    }

    // ---- Auctions -----------------------------------------------------

    /// `getGiftAuctionState`; the reducer marks the gift ready for the UI.
    pub fn fetch_gift_auction(&mut self, auction_id: &str) -> Result<RequestId, ConnectSendError> {
        if auction_id.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let gift_id = self
            .session
            .payments
            .gift_market
            .auctions
            .values()
            .find(|state| state.auction_id() == Some(auction_id))
            .map_or(0, |state| state.gift_id());
        let result = self.send_market(GiftMarketPurpose::AuctionState { gift_id }, |extra| {
            rq::get_gift_auction_state(extra, auction_id)
        });
        let market = &mut self.session.payments.gift_market;
        if let Ok(extra) = &result {
            market.auction_request = extra.0;
            market.auction_ready = None;
            market.auction_loading = true;
        }
        result
    }

    /// Follows one auction's updates while its box shows (`openGiftAuction`)
    /// and stops following the previous one.
    pub fn follow_gift_auction(&mut self, gift_id: Option<i64>) -> Result<(), ConnectSendError> {
        let was = self.session.payments.gift_market.followed;
        if was == gift_id {
            return Ok(());
        }
        if let Some(old) = was {
            let _ = self.send_market(GiftMarketPurpose::CloseAuction, |extra| {
                rq::follow_gift_auction(extra, old, false)
            });
        }
        self.session.payments.gift_market.followed = gift_id;
        if let Some(new) = gift_id {
            self.send_market(GiftMarketPurpose::OpenAuction, |extra| {
                rq::follow_gift_auction(extra, new, true)
            })?;
        }
        Ok(())
    }

    /// Places a first bid or raises one to `stars` in total. `user_id` is
    /// who gets the gift.
    pub fn bid_gift_auction(
        &mut self,
        gift_id: i64,
        stars: i64,
        user_id: i64,
        text: &str,
        hide_name: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let market = &self.session.payments.gift_market;
        let state = market
            .auctions
            .get(&gift_id)
            .ok_or(ConnectSendError::InvalidRequest)?;
        let mine = state.my_stars();
        let add = stars.saturating_sub(mine);
        if market.bidding.is_some()
            || state.finished(now_unix())
            || user_id <= 0
            || stars < state.min_bid_for_me()
            || stars > crate::gift_market::MAX_CUSTOM_BID
            || add <= 0
            || !self.market_balance_covers(add)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let text = text.trim().to_string();
        let result = if mine > 0 {
            self.send_market(GiftMarketPurpose::IncreaseBid { gift_id }, |extra| {
                rq::increase_gift_auction_bid(extra, gift_id, stars)
            })
        } else {
            self.send_market(GiftMarketPurpose::PlaceBid { gift_id }, |extra| {
                rq::place_gift_auction_bid(extra, gift_id, stars, user_id, &text, hide_name)
            })
        };
        if result.is_ok() {
            self.session.payments.gift_market.bidding = Some(gift_id);
        }
        result
    }

    /// `getGiftAuctionAcquiredGifts` for "N items bought".
    pub fn fetch_gift_auction_acquired(
        &mut self,
        gift_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_market(GiftMarketPurpose::AcquiredGifts { gift_id }, |extra| {
            rq::get_gift_auction_acquired_gifts(extra, gift_id)
        })
    }

    /// `getGiftUpgradePreview`, once per gift.
    pub fn fetch_gift_upgrade_preview(
        &mut self,
        gift_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        let purpose = GiftMarketPurpose::UpgradePreview { gift_id };
        if gift_id == 0
            || self
                .session
                .payments
                .gift_market
                .previews
                .contains_key(&gift_id)
            || self.session.requests.has_purpose(RequestPurpose::Payments(
                PaymentsPurpose::GiftMarket(purpose),
            ))
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.send_market(purpose, |extra| {
            rq::get_gift_upgrade_preview(extra, gift_id)
        })
    }

    // ---- Crafting -----------------------------------------------------

    /// Opens the craft box with `primary` in the first slot and loads the
    /// other gifts of its collection and the suitable listings.
    pub fn open_gift_craft(&mut self, primary: CraftGift) -> Result<(), ConnectSendError> {
        if primary.gate(true, now_unix()) != crate::gift_market::CraftGate::Ready {
            return Err(ConnectSendError::InvalidRequest);
        }
        let gift_id = primary.gift.regular_gift_id;
        let title = primary.gift.title.clone();
        let market = &mut self.session.payments.gift_market;
        market.remember_title(gift_id, &title);
        market.craft = Some(CraftSession::new(primary));
        let _ = self.preload_craft_lists();
        Ok(())
    }

    /// Loads the first page of both "Select Gifts" lists when not loaded.
    pub fn preload_craft_lists(&mut self) -> Result<(), ConnectSendError> {
        let craft = self
            .session
            .payments
            .gift_market
            .craft
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let (own, market) = (
            !craft.candidates_loaded && !craft.candidates_loading,
            !craft.market.loaded && !craft.market.loading,
        );
        if own {
            self.craft_candidates_page(false)?;
        }
        if market {
            self.search_resale_page(true, false)?;
        }
        Ok(())
    }

    /// The next page of the user's own gifts for crafting.
    pub fn load_more_craft_candidates(&mut self) -> Result<RequestId, ConnectSendError> {
        let craft = self
            .session
            .payments
            .gift_market
            .craft
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        if craft.candidates_loading || craft.candidates_offset.is_empty() {
            return Err(ConnectSendError::InvalidRequest);
        }
        self.craft_candidates_page(true)
    }

    fn craft_candidates_page(&mut self, more: bool) -> Result<RequestId, ConnectSendError> {
        let craft = self
            .session
            .payments
            .gift_market
            .craft
            .as_mut()
            .ok_or(ConnectSendError::InvalidRequest)?;
        if !more {
            craft.candidates_page = 0;
            craft.candidates_offset.clear();
        }
        craft.candidates_loading = true;
        let (gift_id, page) = (craft.regular_gift_id, craft.candidates_page);
        let offset = craft.candidates_offset.clone();
        let result = self.send_market(
            GiftMarketPurpose::GiftsForCrafting { gift_id, page },
            |extra| rq::get_gifts_for_crafting(extra, gift_id, &offset),
        );
        if let Some(craft) = self.session.payments.gift_market.craft.as_mut() {
            match &result {
                Ok(extra) => {
                    craft.candidates_request = extra.0;
                    craft.candidates_page = page.saturating_add(1);
                }
                Err(_) => craft.candidates_loading = false,
            }
        }
        result
    }

    /// `craftGift` with the gifts in the slots.
    pub fn craft_gift(&mut self) -> Result<RequestId, ConnectSendError> {
        let craft = self
            .session
            .payments
            .gift_market
            .craft
            .as_ref()
            .ok_or(ConnectSendError::InvalidRequest)?;
        let now = now_unix();
        if craft.crafting
            || craft.result.is_some()
            || craft.slots.is_empty()
            || craft.slots.iter().enumerate().any(|(index, slot)| {
                slot.gate(index == 0, now) != crate::gift_market::CraftGate::Ready
            })
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let ids: Vec<String> = craft
            .slots
            .iter()
            .map(|slot| slot.received_gift_id.clone())
            .collect();
        let result = self.send_market(GiftMarketPurpose::Craft, |extra| {
            rq::craft_gift(extra, &ids)
        });
        if result.is_ok()
            && let Some(craft) = self.session.payments.gift_market.craft.as_mut()
        {
            craft.crafting = true;
        }
        result
    }
}
