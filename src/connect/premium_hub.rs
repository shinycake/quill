//! Connect driver: Stars history, received gifts and the Premium explainer
//! (`crate::premium_hub`). Read-only apart from the two own-gift mutations;
//! no payment, transfer or send request is built here.
use super::*;
use crate::ids::RequestId;
use crate::premium_hub::{GiftMutation, TxFilter};
use crate::premium_limits::PremiumLimitKind;
use crate::state::PaymentsPurpose;
use crate::state::RequestPurpose;
use crate::telegram::envelope::MessageSender;
use crate::telegram::requests::get_premium_limit;
use crate::telegram::requests_gifts::get_received_gifts_in;
use crate::telegram::requests_premium::{
    get_business_features, get_premium_features, get_premium_state, get_premium_sticker_examples,
    get_star_transactions, sell_gift, toggle_gift_is_saved,
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
        if !self.session.payments.hub.tx.wants_first_page() {
            return Ok(None);
        }
        self.fetch_star_transactions_page(false).map(Some)
    }

    /// Next page of the current filter; `Ok(None)` = no more pages.
    pub fn fetch_more_star_transactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let tx = &self.session.payments.hub.tx;
        if tx.next_offset().is_empty() || tx.loading() {
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
        if self.session.payments.hub.filter == filter && self.session.payments.hub.tx.loaded() {
            return Ok(None);
        }
        let hub = &mut self.session.payments.hub;
        hub.filter = filter;
        // The filter owns the list: nothing of the old one is kept, and the
        // page of the old filter still in flight is dropped with its request.
        hub.tx.reset();
        hub.tx_selected = None;
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
            self.session.payments.hub.tx.next_offset().to_string()
        } else {
            String::new()
        };
        let filter = self.session.payments.hub.filter;
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append }),
            None,
        );
        self.session.payments.hub.tx.begin(extra, append);
        let json = get_star_transactions(extra, my_user_id, filter, &offset, TX_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            // A first page that never left ends in the error line, not in an
            // empty list with nothing drawn.
            self.session.payments.hub.tx.send_failed(crate::lang::tr(
                crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_STARS_HISTORY,
            ));
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
        let hub = &mut self.session.payments.hub;
        hub.gifts_open = true;
        hub.gifts_owner = Some(owner);
        hub.gifts.reset();
        hub.gifts_error = None;
        hub.gift_selected = None;
        hub.gift_convert_confirm = None;
        // `gift_mutation` is not reset: a toggle or sale still out keeps its
        // lock until its own answer (another request would pay twice).
        self.begin_gift_manage(owner);
        self.fetch_received_gifts_page(false)
    }

    /// Next page of the open gifts list.
    pub fn fetch_more_received_gifts(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let gifts = &self.session.payments.hub.gifts;
        if gifts.next_offset().is_empty() || gifts.loading() {
            return Ok(None);
        }
        self.fetch_received_gifts_page(true).map(Some)
    }

    pub(super) fn fetch_received_gifts_page(
        &mut self,
        append: bool,
    ) -> Result<RequestId, ConnectSendError> {
        let owner = self
            .session
            .payments
            .hub
            .gifts_owner
            .ok_or(ConnectSendError::InvalidRequest)?;
        let offset = if append {
            self.session.payments.hub.gifts.next_offset().to_string()
        } else {
            String::new()
        };
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append }),
            None,
        );
        self.session.payments.hub.gifts.begin(extra, append);
        let collection = self.session.payments.gifts_manage.active_collection;
        let json = get_received_gifts_in(extra, owner, collection, &offset, GIFT_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.hub.gifts.send_failed(crate::lang::tr(
                crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_GIFTS,
            ));
            return Err(err);
        }
        Ok(extra)
    }

    /// Refetch the open gifts list after a mutation marked it stale (never
    /// optimistic: the toggle/convert shows once the server list returns).
    pub fn refresh_received_gifts_if_stale(
        &mut self,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let hub = &self.session.payments.hub;
        // A mutation outdated the list and dropped any page asked before it.
        // A refused send ends in the error line and is not asked again every
        // frame.
        if !hub.gifts.is_outdated() || hub.gifts_owner.is_none() {
            return Ok(None);
        }
        self.fetch_received_gifts_page(false).map(Some)
    }

    /// `toggleGiftIsSaved` — own gifts only. One mutation at a time.
    pub fn toggle_gift_saved(
        &mut self,
        received_gift_id: &str,
        saved: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.payments.hub.gift_mutation.is_busy()
            || !self
                .session
                .payments
                .hub
                .gifts_are_mine(self.session.my_user_id)
            || !self
                .session
                .payments
                .hub
                .gift_list()
                .iter()
                .any(|gift| gift.id == received_gift_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved }),
            None,
        );
        self.session.payments.hub.gifts_error = None;
        let json = toggle_gift_is_saved(extra, received_gift_id, saved);
        self.send_json_request(extra, &json)?;
        self.session.payments.hub.gift_mutation = GiftMutation::Busy(extra);
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
            .payments
            .hub
            .gift_list()
            .iter()
            .any(|gift| gift.id == received_gift_id && gift.can_convert());
        if !self.chats_path_active()
            || self.session.payments.hub.gift_mutation.is_busy()
            || !self
                .session
                .payments
                .hub
                .gifts_are_mine(self.session.my_user_id)
            || !convertible
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request_for_subject(RequestPurpose::SellGift, received_gift_id);
        self.session.payments.hub.gifts_error = None;
        let json = sell_gift(extra, received_gift_id);
        self.send_json_request(extra, &json)?;
        self.session.payments.hub.gift_mutation = GiftMutation::Busy(extra);
        Ok(extra)
    }

    /// Fetch the example stickers for the stickers feature preview, once.
    pub fn maybe_fetch_premium_stickers(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let hub = &mut self.session.payments.hub;
        if !hub.premium_stickers.can_ask() {
            return Ok(None);
        }
        hub.premium_stickers.begin();
        let extra = self
            .session
            .request(RequestPurpose::GetPremiumStickerExamples, None);
        if let Err(err) = self.send_json_request(extra, &get_premium_sticker_examples(extra)) {
            self.session.payments.hub.premium_stickers.fail("not sent");
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// `getPremiumLimit` for a limit box or toast, unless the Premium info
    /// or an earlier answer already holds the pair, or it was asked for.
    pub fn ensure_premium_limit(
        &mut self,
        kind: PremiumLimitKind,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let hub = &self.session.payments.hub;
        let known = hub
            .premium
            .data()
            .and_then(|info| info.limit_pair(kind))
            .is_some();
        let Some(type_name) = kind.td_limit_type() else {
            return Ok(None);
        };
        if known || !hub.limits.should_ask(kind) {
            return Ok(None);
        }
        let extra = self.session.request(RequestPurpose::GetPremiumLimit, None);
        self.session.payments.hub.limits.note_asked(kind, extra);
        if let Err(err) = self.send_json_request(extra, &get_premium_limit(extra, type_name)) {
            self.session.payments.hub.limits.forget_asked(kind);
            return Err(err);
        }
        Ok(Some(extra))
    }

    /// Fetch the Premium explainer (`getPremiumFeatures` + `getPremiumState`).
    pub fn maybe_fetch_premium(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.payments.hub.premium.can_ask() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPremiumFeatures, None);
        self.session.payments.hub.premium.begin();
        if let Err(err) = self.send_json_request(extra, &get_premium_features(extra)) {
            self.session.payments.hub.premium.fail("not sent");
            return Err(err);
        }
        let state_extra = self.session.request(RequestPurpose::GetPremiumState, None);
        // The state text is optional decoration; a send failure is ignored.
        let _ = self.send_json_request(state_extra, &get_premium_state(state_extra));
        Ok(Some(extra))
    }

    /// Fetch the Business feature list (`getBusinessFeatures`) once.
    pub fn maybe_fetch_business_features(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let hub = &self.session.payments.hub;
        if !hub.business_features.can_ask() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetBusinessFeatures, None);
        self.session.payments.hub.business_features.begin();
        if let Err(err) = self.send_json_request(extra, &get_business_features(extra)) {
            self.session.payments.hub.business_features.fail("not sent");
            return Err(err);
        }
        Ok(Some(extra))
    }
}
