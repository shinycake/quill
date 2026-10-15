//! Connect driver: Settings → My Grams (`crate::premium_hub::ton`). Sends
//! only `getTonTransactions` and `getGramRevenueStatistics`; nothing here
//! moves Grams.
use super::*;
use crate::ids::RequestId;
use crate::premium_hub::TxFilter;
use crate::premium_hub::ton::{
    GramsPurpose, PAGE, get_gram_revenue_statistics, get_ton_transactions,
};
use crate::state::{PaymentsPurpose, RequestPurpose};

impl<S: JsonSender> ConnectDriver<S> {
    fn send_grams(
        &mut self,
        purpose: GramsPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::Grams(purpose)),
            None,
        );
        let json = build(extra);
        self.send_json_request(extra, &json)?;
        Ok(extra)
    }

    /// Ask for the Grams balance once per session, for the Settings row
    /// (tdesktop `Credits::tonLoad`). `Ok(None)` when already asked.
    pub fn probe_grams_balance(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        if self.session.payments.hub.grams.probed {
            return Ok(None);
        }
        let sent = self.send_grams(GramsPurpose::Probe, |extra| {
            get_ton_transactions(extra, TxFilter::All, "", 1)
        })?;
        self.session.payments.hub.grams.probed = true;
        Ok(Some(sent))
    }

    /// Open the Grams screen: the first All page, the revenue for the
    /// withdrawal section and the app config for the USD rate.
    pub fn open_grams(&mut self, is_dark: bool) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let grams = &mut self.session.payments.hub.grams;
        grams.open = true;
        grams.restart(TxFilter::All);
        let page = self.fetch_grams_page(false)?;
        let _ = self.send_grams(GramsPurpose::Revenue, |extra| {
            get_gram_revenue_statistics(extra, is_dark)
        });
        let _ = self.fetch_ton_explorer_url();
        Ok(page)
    }

    /// Next page of the current tab; `Ok(None)` = no more pages.
    pub fn fetch_more_grams(&mut self) -> Result<Option<RequestId>, ConnectSendError> {
        let grams = &self.session.payments.hub.grams;
        if !grams.has_more() || grams.loading {
            return Ok(None);
        }
        self.fetch_grams_page(true).map(Some)
    }

    /// Switch the All / Incoming / Outgoing tab and refetch from the top.
    pub fn set_grams_filter(
        &mut self,
        filter: TxFilter,
    ) -> Result<Option<RequestId>, ConnectSendError> {
        let grams = &mut self.session.payments.hub.grams;
        if grams.filter == filter && grams.loaded {
            return Ok(None);
        }
        grams.restart(filter);
        self.fetch_grams_page(false).map(Some)
    }

    fn fetch_grams_page(&mut self, append: bool) -> Result<RequestId, ConnectSendError> {
        let grams = &self.session.payments.hub.grams;
        let offset = if append {
            grams.next_offset.clone()
        } else {
            String::new()
        };
        let filter = grams.filter;
        let sent = self.send_grams(GramsPurpose::Page { append }, |extra| {
            get_ton_transactions(extra, filter, &offset, PAGE)
        });
        let grams = &mut self.session.payments.hub.grams;
        match sent {
            Ok(extra) => {
                grams.loading = true;
                grams.error = None;
                grams.request = extra.0;
                Ok(extra)
            }
            Err(err) => {
                grams.loading = false;
                Err(err)
            }
        }
    }
}
