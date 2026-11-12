//! Connect driver: Stars history, received gifts and the Premium explainer
//! (`crate::premium_hub`). Read-only apart from the two own-gift mutations;
//! no payment, transfer or send request is built here.
use super::*;
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::premium_hub::page_stage::{PageKind, PageStage};
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

fn page_kind(append: bool) -> PageKind {
    if append {
        PageKind::Next
    } else {
        PageKind::First
    }
}

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
        if !self.session.payments.hub.stars.page.first_page_due() {
            return Ok(None);
        }
        self.fetch_star_transactions_page(false).map(Some)
    }

    /// Next page of the current filter; `Ok(None)` = no more pages.
    pub fn fetch_more_star_transactions(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.payments.hub.stars.offset.is_empty()
            || self.session.payments.hub.stars.page.is_loading()
        {
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
        if self.session.payments.hub.stars.filter == filter
            && self.session.payments.hub.stars.page.is_loaded()
        {
            return Ok(None);
        }
        self.session.payments.hub.stars.switch_filter(filter);
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
            self.session.payments.hub.stars.offset.to_string()
        } else {
            String::new()
        };
        let filter = self.session.payments.hub.stars.filter;
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetStarTransactions { append }),
            None,
        );
        let kind = page_kind(append);
        self.session.payments.hub.stars.page = PageStage::Loading {
            request: extra,
            kind,
        };
        let json = get_star_transactions(extra, my_user_id, filter, &offset, TX_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            // A first page that never left ends in the error line, not in an
            // empty list with nothing drawn.
            let hub = &mut self.session.payments.hub;
            hub.stars.page = PageStage::Failed {
                error: crate::lang::tr(crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_STARS_HISTORY)
                    .into(),
                kind,
            };
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
        // The list starts over for the owner; a toggle or sale still out
        // keeps its lock until its own answer (another request would pay
        // twice).
        let received = &mut self.session.payments.hub.received;
        *received = crate::premium_hub::ReceivedGifts::opened_for(owner, received.op);
        self.begin_gift_manage(owner);
        self.fetch_received_gifts_page(false)
    }

    /// Next page of the open gifts list.
    pub fn fetch_more_received_gifts(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.payments.hub.received.offset.is_empty()
            || self.session.payments.hub.received.page.is_loading()
        {
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
            .received
            .owner
            .ok_or(ConnectSendError::InvalidRequest)?;
        let offset = if append {
            self.session.payments.hub.received.offset.to_string()
        } else {
            String::new()
        };
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::GetReceivedGifts { append }),
            None,
        );
        let kind = page_kind(append);
        self.session.payments.hub.received.page = PageStage::Loading {
            request: extra,
            kind,
        };
        let collection = self.session.payments.gifts_manage.active_collection;
        let json = get_received_gifts_in(extra, owner, collection, &offset, GIFT_PAGE);
        if let Err(err) = self.send_json_request(extra, &json) {
            let hub = &mut self.session.payments.hub;
            hub.received.page = PageStage::Failed {
                error: crate::lang::tr(crate::lang::keys::Q_MONEY_COULD_NOT_LOAD_GIFTS).into(),
                kind,
            };
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
        // A page already out settles first (its answer is not the refetch);
        // the list stays stale until a first page answers or fails.
        if !hub.received.op.is_stale()
            || hub.received.owner.is_none()
            || hub.received.page.is_loading()
        {
            return Ok(None);
        }
        let sent = self.fetch_received_gifts_page(false);
        if sent.is_err() {
            // Not asked again every frame: the error line says so.
            self.session.payments.hub.received.op.refetched();
        }
        sent.map(Some)
    }

    /// `toggleGiftIsSaved` — own gifts only. One mutation at a time.
    pub fn toggle_gift_saved(
        &mut self,
        received_gift_id: &str,
        saved: bool,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active()
            || self.session.payments.hub.received.op.is_mutating()
            || !self
                .session
                .payments
                .hub
                .gifts_are_mine(self.session.my_user_id)
            || !self
                .session
                .payments
                .hub
                .received
                .rows
                .iter()
                .any(|gift| gift.id == received_gift_id)
        {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::ToggleGiftSaved { saved }),
            None,
        );
        self.session.payments.hub.received.op.begin(extra);
        self.session.payments.hub.received.notice = None;
        let json = toggle_gift_is_saved(extra, received_gift_id, saved);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.hub.received.op.failed(Some(extra));
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
            .payments
            .hub
            .received
            .rows
            .iter()
            .any(|gift| gift.id == received_gift_id && gift.can_convert());
        if !self.chats_path_active()
            || self.session.payments.hub.received.op.is_mutating()
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
        self.session.payments.hub.received.op.begin(extra);
        self.session.payments.hub.received.notice = None;
        let json = sell_gift(extra, received_gift_id);
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.payments.hub.received.op.failed(Some(extra));
            return Err(err);
        }
        Ok(extra)
    }

    /// Fetch the example stickers for the stickers feature preview, once.
    pub fn maybe_fetch_premium_stickers(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.payments.hub.premium_stickers.can_ask() {
            return Ok(None);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetPremiumStickerExamples, None);
        self.session.payments.hub.premium_stickers.begin();
        self.session.payments.hub.premium_stickers.sent(extra);
        if let Err(err) = self.send_json_request(extra, &get_premium_sticker_examples(extra)) {
            // The next look at the preview asks again.
            self.session.payments.hub.premium_stickers.cancel();
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
        let hub = &self.session.payments.hub;
        let features = hub.premium.can_ask();
        let state = hub.premium_state.can_ask() || hub.premium_state.is_outdated();
        let mut asked = None;
        if features {
            let extra = self
                .session
                .request(RequestPurpose::GetPremiumFeatures, None);
            self.session.payments.hub.premium.begin();
            if let Err(err) = self.send_json_request(extra, &get_premium_features(extra)) {
                self.session.payments.hub.premium.cancel();
                return Err(err);
            }
            self.session.payments.hub.premium.sent(extra);
            asked = Some(extra);
        }
        if state {
            let state_extra = self.session.request(RequestPurpose::GetPremiumState, None);
            let hub = &mut self.session.payments.hub;
            hub.premium_state.begin();
            // The state text is optional decoration: a send that fails
            // leaves the plans out, and the next opening asks again.
            match self.send_json_request(state_extra, &get_premium_state(state_extra)) {
                Ok(_) => self.session.payments.hub.premium_state.sent(state_extra),
                Err(_) => self.session.payments.hub.premium_state.fail("not sent"),
            }
        }
        Ok(asked)
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
            self.session.payments.hub.business_features.cancel();
            return Err(err);
        }
        self.session.payments.hub.business_features.sent(extra);
        Ok(Some(extra))
    }
}
