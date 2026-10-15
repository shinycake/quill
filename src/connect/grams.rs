//! Connect driver: Settings → My Grams (`crate::premium_hub::ton`). Sends
//! only reads (`getTonTransactions`, `getGramRevenueStatistics`,
//! `getUser`, `getChat`); nothing here moves Grams.
use super::*;
use crate::ids::{ChatId, RequestId};
use crate::premium_hub::TxFilter;
use crate::premium_hub::ton::{
    GramsPurpose, PAGE, get_gram_revenue_statistics, get_ton_transactions, get_user,
};
use crate::state::{PaymentsPurpose, RequestPurpose};
use crate::telegram::envelope::MessageSender;

impl<S: JsonSender> ConnectDriver<S> {
    fn send_grams(
        &mut self,
        purpose: GramsPurpose,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        self.send_grams_for(purpose, None, build)
    }

    fn send_grams_for(
        &mut self,
        purpose: GramsPurpose,
        peer: Option<ChatId>,
        build: impl FnOnce(RequestId) -> String,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self.session.request(
            RequestPurpose::Payments(PaymentsPurpose::Grams(purpose)),
            peer,
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

    /// Called from the poll loop: probes the balance while Settings is
    /// open (so the "My Grams" row can appear) and asks once for each user
    /// or chat a landed page names that the session does not know.
    /// Returns whether anything was sent.
    pub fn grams_tick(&mut self, settings_open: bool) -> bool {
        let mut sent = false;
        if settings_open && !self.session.payments.hub.grams.probed {
            sent |= matches!(self.probe_grams_balance(), Ok(Some(_)));
        }
        if !self.session.payments.hub.grams.peers_dirty || !self.chats_path_active() {
            return sent;
        }
        let grams = &mut self.session.payments.hub.grams;
        grams.peers_dirty = false;
        let peers: Vec<MessageSender> = grams.peers().collect();
        for peer in peers {
            let grams = &self.session.payments.hub.grams;
            let ask = match peer {
                MessageSender::User { user_id } => (self.session.user(user_id).is_none()
                    && !grams.asked_users.contains(&user_id))
                .then_some((GramsPurpose::PeerUser, user_id)),
                MessageSender::Chat { chat_id } => (!self.session.chats.contains_key(&chat_id)
                    && !grams.asked_chats.contains(&chat_id))
                .then_some((GramsPurpose::PeerChat, chat_id)),
            };
            let Some((purpose, id)) = ask else {
                continue;
            };
            let result = self.send_grams_for(purpose, Some(ChatId(id)), |extra| {
                if purpose == GramsPurpose::PeerUser {
                    get_user(extra, id)
                } else {
                    crate::telegram::requests::get_chat(extra, ChatId(id))
                }
            });
            let grams = &mut self.session.payments.hub.grams;
            if purpose == GramsPurpose::PeerUser {
                grams.asked_users.insert(id);
            } else {
                grams.asked_chats.insert(id);
            }
            sent |= result.is_ok();
        }
        sent
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
        let page = match self.fetch_grams_page(false) {
            Ok(page) => page,
            Err(err) => {
                // The dialog is open: say why it is empty.
                self.session.payments.hub.grams.error =
                    Some(crate::lang::tr(crate::lang::keys::Q_GRAMS_COULD_NOT_LOAD).to_string());
                return Err(err);
            }
        };
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
