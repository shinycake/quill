//! Connect driver: the read-only TON wallet (`crate::ton_wallet`). It asks
//! `getApplicationConfig` for the explorer address and the Gram's USD
//! price, and, only while `can_use_ton_wallet` is on, the wallet getters
//! in `crate::ton_wallet::READ_ONLY_METHODS`. Nothing it sends needs a key
//! or moves value.
use super::*;
use crate::state::{PaymentsPurpose, RequestPurpose, SettingsPurpose};
use crate::telegram::requests::{
    get_currency_exchange_rates, get_ton_wallet_nfts, get_ton_wallet_transaction,
    get_ton_wallet_transactions, get_user_ton_wallet_addresses, load_ton_wallet_state,
};
use crate::telegram::requests_privacy::hide_suggested_action;
use crate::ton_wallet::{
    FIRST_TRANSFER_HINT, HISTORY_PAGE, MoneyDetail, MoneyPage, MoneyTab, NFT_PAGE, ProfileAddress,
    TonWalletPurpose, WalletPrefs, fiat,
};

/// The line a list shows when its page request could not be sent.
const WALLET_SEND_FAILED: &str = "The request could not be sent.";

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask `getApplicationConfig` once per session for
    /// `ton_blockchain_explorer_url`, the prefix "View in Explorer" opens
    /// (Telegram Desktop's `TonAddressUrl`), and `ton_usd_rate`, the
    /// Gram's price for the fiat value. `Ok(false)` when already asked.
    pub fn fetch_ton_explorer_url(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if !self.session.ton_wallet.explorer.can_ask() {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetTonExplorerUrl, None);
        let json = crate::telegram::requests::get_application_config(extra);
        self.session.ton_wallet.explorer.begin();
        if let Err(err) = self.send_json_request(extra, &json) {
            self.session.ton_wallet.explorer.cancel();
            return Err(err);
        }
        self.session.ton_wallet.explorer.sent(extra);
        Ok(true)
    }

    /// Whether wallet getters may be sent: signed in and the server turned
    /// the wallet on (`can_use_ton_wallet`).
    fn wallet_ready(&self) -> bool {
        self.chats_path_active() && self.session.ton_wallet.can_use
    }

    fn send_wallet(
        &mut self,
        purpose: TonWalletPurpose,
        user_id: Option<i64>,
        build: impl FnOnce(crate::ids::RequestId) -> String,
    ) -> Result<crate::ids::RequestId, ConnectSendError> {
        let purpose = RequestPurpose::Payments(PaymentsPurpose::TonWallet(purpose));
        let extra = match user_id {
            Some(user_id) => self.session.request_for_user(purpose, user_id),
            None => self.session.request(purpose, None),
        };
        let json = build(extra);
        self.send_json_request(extra, &json)
    }

    /// Open the Money page: load the wallet state, the rates for the fiat
    /// value, the explorer and the first page of history. The currency the
    /// user picked earlier is read from the account's prefs.
    pub fn open_money_page(&mut self) -> Result<(), ConnectSendError> {
        if !self.wallet_ready() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.ton_wallet.currency.is_none() {
            let prefs = crate::ton_wallet::load_prefs(&self.paths);
            let currency = prefs
                .currency
                .filter(|code| fiat::is_known_currency(code))
                .unwrap_or_else(|| fiat::DEFAULT_CURRENCY.to_string());
            self.session.ton_wallet.currency = Some(currency);
        }
        let wallet = &mut self.session.ton_wallet;
        wallet.money = Some(MoneyPage::default());
        // Each ask is independent: a refused one leaves its own stage to be
        // asked again, and never keeps the history from being requested.
        let mut first_error = None;
        if self.session.ton_wallet.own.can_ask() {
            self.session.ton_wallet.own.begin();
            match self.send_wallet(TonWalletPurpose::LoadState, None, load_ton_wallet_state) {
                Ok(request) => self.session.ton_wallet.own.sent(request),
                Err(err) => {
                    self.session.ton_wallet.own.cancel();
                    first_error.get_or_insert(err);
                }
            }
        }
        if self.session.ton_wallet.rates.can_ask() {
            self.session.ton_wallet.rates.begin();
            match self.send_wallet(
                TonWalletPurpose::ExchangeRates,
                None,
                get_currency_exchange_rates,
            ) {
                Ok(request) => self.session.ton_wallet.rates.sent(request),
                Err(err) => {
                    self.session.ton_wallet.rates.cancel();
                    first_error.get_or_insert(err);
                }
            }
        }
        if let Err(err) = self.fetch_ton_explorer_url() {
            first_error.get_or_insert(err);
        }
        if let Err(err) = self.fetch_wallet_history(false) {
            first_error.get_or_insert(err);
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Close the Money page (its lists are dropped; the state is kept).
    pub fn close_money_page(&mut self) {
        self.session.ton_wallet.money = None;
    }

    /// The first (`append == false`) or next page of transactions.
    pub fn fetch_wallet_history(&mut self, append: bool) -> Result<(), ConnectSendError> {
        if !self.wallet_ready() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(money) = self.session.ton_wallet.money.as_mut() else {
            return Ok(());
        };
        let history = &money.history;
        if history.is_loading() || (append && !history.has_more()) {
            return Ok(());
        }
        let offset = if append {
            history.next_offset().to_string()
        } else {
            String::new()
        };
        let sent = match self.send_wallet(TonWalletPurpose::Transactions, None, |extra| {
            get_ton_wallet_transactions(extra, &offset, HISTORY_PAGE)
        }) {
            Ok(sent) => sent,
            Err(err) => {
                if let Some(money) = self.session.ton_wallet.money.as_mut() {
                    money.history.send_failed(WALLET_SEND_FAILED.into());
                }
                return Err(err);
            }
        };
        if let Some(money) = self.session.ton_wallet.money.as_mut() {
            money.history.begin(sent, append);
        }
        Ok(())
    }

    /// The first or next page of NFTs.
    pub fn fetch_wallet_nfts(&mut self, append: bool) -> Result<(), ConnectSendError> {
        if !self.wallet_ready() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let Some(money) = self.session.ton_wallet.money.as_mut() else {
            return Ok(());
        };
        let nfts = &money.nfts;
        if nfts.is_loading() || (append && !nfts.has_more()) {
            return Ok(());
        }
        let offset = if append {
            nfts.next_offset().to_string()
        } else {
            String::new()
        };
        let sent = match self.send_wallet(TonWalletPurpose::Nfts, None, |extra| {
            get_ton_wallet_nfts(extra, &offset, NFT_PAGE)
        }) {
            Ok(sent) => sent,
            Err(err) => {
                if let Some(money) = self.session.ton_wallet.money.as_mut() {
                    money.nfts.send_failed(WALLET_SEND_FAILED.into());
                }
                return Err(err);
            }
        };
        if let Some(money) = self.session.ton_wallet.money.as_mut() {
            money.nfts.begin(sent, append);
        }
        Ok(())
    }

    /// Switch the Money page's tab; Collectibles loads on first view.
    pub fn set_money_tab(&mut self, tab: MoneyTab) -> Result<(), ConnectSendError> {
        let Some(money) = self.session.ton_wallet.money.as_mut() else {
            return Ok(());
        };
        money.tab = tab;
        money.detail = MoneyDetail::Overview;
        if tab == MoneyTab::Collectibles && !money.nfts.is_loaded() && !money.nfts.is_loading() {
            return self.fetch_wallet_nfts(false);
        }
        Ok(())
    }

    /// `getUserTonWalletAddresses` for the user whose profile opened, once
    /// per session. Never for yourself, and never in bulk.
    pub fn fetch_user_ton_wallet_address(
        &mut self,
        user_id: i64,
    ) -> Result<bool, ConnectSendError> {
        if !self.wallet_ready() {
            return Ok(false);
        }
        // A failed lookup is asked again; a lookup out, an address and "no
        // wallet" are not.
        if user_id <= 0
            || self.session.my_user_id == Some(user_id)
            || self
                .session
                .ton_wallet
                .profile_addresses
                .get(&user_id)
                .is_some_and(|known| !matches!(known, ProfileAddress::Failed))
        {
            return Ok(false);
        }
        let request = self.send_wallet(TonWalletPurpose::UserAddress, Some(user_id), |extra| {
            get_user_ton_wallet_addresses(extra, &[user_id])
        })?;
        self.session
            .ton_wallet
            .profile_addresses
            .insert(user_id, ProfileAddress::Loading { request });
        Ok(true)
    }

    /// A transfer card was clicked: open the Money page on that
    /// transaction (the iOS card opens the wallet's transaction screen).
    /// `Ok(false)` when the wallet is off or the card has no transaction.
    pub fn open_wallet_transaction(
        &mut self,
        transaction_id: &str,
    ) -> Result<bool, ConnectSendError> {
        if !self.wallet_ready() || transaction_id.is_empty() {
            return Ok(false);
        }
        if self.session.ton_wallet.money.is_none() {
            self.open_money_page()?;
        }
        let known = self.session.ton_wallet.money.as_ref().and_then(|money| {
            money
                .history
                .items()
                .iter()
                .find(|tx| tx.id == transaction_id)
                .cloned()
        });
        if let Some(tx) = known {
            self.session.show_wallet_transaction(tx);
            return Ok(true);
        }
        if let Some(money) = self.session.ton_wallet.money.as_mut() {
            money.detail = MoneyDetail::Overview;
        }
        let id = transaction_id.to_string();
        let sent = self.send_wallet(TonWalletPurpose::Transaction, None, |extra| {
            get_ton_wallet_transaction(extra, &id)
        });
        if let Some(money) = self.session.ton_wallet.money.as_mut() {
            money.detail = match &sent {
                Ok(request) => MoneyDetail::Lookup { request: *request },
                Err(_) => MoneyDetail::Overview,
            };
        }
        sent.map(|_| true)
    }

    /// "Got it" (or closing) on "Your first Grams!": the hint is done, so
    /// TDLib stops suggesting it (`hideSuggestedAction`, as the iOS app
    /// dismisses the server suggestion).
    pub fn dismiss_first_transfer_hint(&mut self) -> Result<(), ConnectSendError> {
        if let Some(money) = self.session.ton_wallet.money.as_mut()
            && let MoneyDetail::Tx { hint, .. } = &mut money.detail
        {
            *hint = false;
        }
        if !self.chats_path_active()
            || !self
                .session
                .chat_list
                .suggestions
                .actions
                .remove(FIRST_TRANSFER_HINT)
        {
            return Ok(());
        }
        let extra = self.session.request(
            RequestPurpose::Settings(SettingsPurpose::HideSuggestedAction {
                action: FIRST_TRANSFER_HINT,
            }),
            None,
        );
        let json = hide_suggested_action(extra, FIRST_TRANSFER_HINT);
        self.send_json_request(extra, &json).map(|_| ())
    }

    /// Pick the fiat currency and remember it for the account.
    pub fn set_wallet_currency(&mut self, code: &str) -> std::io::Result<()> {
        if !fiat::is_known_currency(code) {
            return Ok(());
        }
        self.session.ton_wallet.currency = Some(code.to_string());
        crate::ton_wallet::save_prefs(
            &self.paths,
            &WalletPrefs {
                currency: Some(code.to_string()),
            },
        )
    }
}
