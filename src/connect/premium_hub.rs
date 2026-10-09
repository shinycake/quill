//! Connect driver: Stars history, received gifts and the Premium explainer
//! (`crate::premium_hub`). Read-only apart from the two own-gift mutations;
//! no payment, transfer or send request is built here.
use super::*;
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::state::RequestPurpose;
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests_premium::{
    get_premium_features, get_premium_state, get_received_gifts, get_star_transactions, sell_gift,
    toggle_gift_is_saved,
};

/// Rows per `getStarTransactions` page.
const TX_PAGE: i32 = 30;
/// Gifts per `getReceivedGifts` page.
const GIFT_PAGE: i32 = 24;

impl<S: JsonSender> ConnectDriver<S> {
    /// Fetch the first page of the Stars history for the current filter
    /// (once, unless the filter changed or a conversion invalidated it).
    pub fn maybe_fetch_star_transactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.hub.tx_loaded || self.session.hub.tx_loading {
            return Ok(None);
        }
        self.fetch_star_transactions_page(false).map(Some)
    }

    /// Next page of the current filter; `Ok(None)` = no more pages.
    pub fn fetch_more_star_transactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.hub.tx_offset.is_empty() || self.session.hub.tx_loading {
            return Ok(None);
        }
        self.fetch_star_transactions_page(true).map(Some)
    }

    /// Switch the All / Incoming / Outgoing tab and refetch from the top.
    pub fn set_star_transactions_filter(
        &mut self,
        filter: TxFilter,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.hub.filter == filter && self.session.hub.tx_loaded {
            return Ok(None);
        }
        let hub = &mut self.session.hub;
        hub.filter = filter;
        hub.transactions.clear();
        hub.tx_offset.clear();
        hub.tx_loaded = false;
        hub.tx_selected = None;
        hub.tx_loading = false;
        self.fetch_star_transactions_page(false).map(Some)
    }

    fn fetch_star_transactions_page(
        &mut self,
        append: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let my_user_id = self
            .session
            .my_user_id
            .ok_or(ConnectSendError::InvalidRequest)?;
        let offset = if append {
            self.session.hub.tx_offset.clone()
        } else {
            String::new()
        };
        let filter = self.session.hub.filter;
        let extra = self
            .session
            .request(RequestPurpose::GetStarTransactions { append }, None);
        self.session.hub.tx_loading = true;
        self.session.hub.tx_error = None;
        self.session.hub.tx_request = extra.0;
        let json = get_star_transactions(extra, my_user_id, filter, &offset, TX_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.hub.tx_loading = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Open the gifts list of `owner` and fetch it (replaces any list of a
    /// previous owner).
    pub fn open_received_gifts(
        &mut self,
        owner: MessageSender,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let hub = &mut self.session.hub;
        hub.gifts_open = true;
        hub.gifts_owner = Some(owner);
        hub.gifts.clear();
        hub.gifts_total = 0;
        hub.gifts_offset.clear();
        hub.gifts_loaded = false;
        hub.gifts_error = None;
        hub.gift_selected = None;
        hub.gift_convert_confirm = None;
        hub.gift_mutating = false;
        hub.gifts_stale = false;
        self.fetch_received_gifts_page(false)
    }

    /// Next page of the open gifts list.
    pub fn fetch_more_received_gifts(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.hub.gifts_offset.is_empty() || self.session.hub.gifts_loading {
            return Ok(None);
        }
        self.fetch_received_gifts_page(true).map(Some)
    }

    fn fetch_received_gifts_page(&mut self, append: bool) -> Result<RequestId, ConnectSendError> {
        let owner = self
            .session
            .hub
            .gifts_owner
            .ok_or(ConnectSendError::InvalidRequest)?;
        let offset = if append {
            self.session.hub.gifts_offset.clone()
        } else {
            String::new()
        };
        let extra = self
            .session
            .request(RequestPurpose::GetReceivedGifts { append }, None);
        self.session.hub.gifts_loading = true;
        self.session.hub.gifts_request = extra.0;
        let json = get_received_gifts(extra, owner, &offset, GIFT_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.hub.gifts_loading = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Refetch the open gifts list after a mutation marked it stale (never
    /// optimistic: the toggle/convert shows once the server list returns).
    pub fn refresh_received_gifts_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.session.hub.gifts_stale || self.session.hub.gifts_owner.is_none() {
            return Ok(None);
        }
        self.session.hub.gifts_stale = false;
        self.fetch_received_gifts_page(false).map(Some)
    }

    /// `toggleGiftIsSaved` — own gifts only. One mutation at a time.
    pub fn toggle_gift_saved(
        &mut self,
        received_gift_id: &str,
        saved: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.hub.gift_mutating
            || !self.session.hub.gifts_are_mine(self.session.my_user_id)
            || !self
                .session
                .hub
                .gifts
                .iter()
                .any(|gift| gift.id == received_gift_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::ToggleGiftSaved { saved }, None);
        self.session.hub.gift_mutating = true;
        self.session.hub.gifts_error = None;
        let json = toggle_gift_is_saved(extra, received_gift_id, saved);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.hub.gift_mutating = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// `sellGift` (convert to Stars) — own, convertible gifts only; the UI
    /// asks for confirmation before calling this.
    pub fn convert_gift_to_stars(
        &mut self,
        received_gift_id: &str,
    ) -> Result<RequestId, ConnectSendError> {
        let convertible = self
            .session
            .hub
            .gifts
            .iter()
            .any(|gift| gift.id == received_gift_id && gift.can_convert());
        if !self.chats_path_active()
            || self.session.hub.gift_mutating
            || !self.session.hub.gifts_are_mine(self.session.my_user_id)
            || !convertible
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(RequestPurpose::SellGift, None);
        self.session.hub.gift_mutating = true;
        self.session.hub.gifts_error = None;
        let json = sell_gift(extra, received_gift_id);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.hub.gift_mutating = false;
            return Err(err);
        }
        Ok(extra)
    }

    /// Fetch the Premium explainer (`getPremiumFeatures` + `getPremiumState`).
    pub fn maybe_fetch_premium(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.hub.premium.is_some() || self.session.hub.premium_loading {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPremiumFeatures, None);
        self.session.hub.premium_loading = true;
        self.session.hub.premium_error = None;
        if let Err(err) = self.send_json_request(extra, &get_premium_features(extra)) {
            self.session.hub.premium_loading = false;
            return Err(err);
        }
        let state_extra = self.session.request(RequestPurpose::GetPremiumState, None);
        // The state text is optional decoration; a send failure is ignored.
        let _ = self.send_json_request(state_extra, &get_premium_state(state_extra));
        Ok(Some(extra))
    }
}
