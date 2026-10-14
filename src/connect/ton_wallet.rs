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
    FIRST_TRANSFER_HINT, HISTORY_PAGE, MoneyPage, MoneyTab, NFT_PAGE, ProfileAddress,
    TonWalletPurpose, WalletPrefs, fiat,
};

impl<S: JsonSender> ConnectDriver<S> {
    /// Ask `getApplicationConfig` once per session for
    /// `ton_blockchain_explorer_url`, the prefix "View in Explorer" opens
    /// (Telegram Desktop's `TonAddressUrl`), and `ton_usd_rate`, the
    /// Gram's price for the fiat value. `Ok(false)` when already asked.
    pub fn fetch_ton_explorer_url(&mut self) -> Result<bool, ConnectSendError> {
        if !self.chats_path_active() {
            return Err(ConnectSendError::InvalidRequest);
        }
        if self.session.ton_wallet.explorer_url_requested {
            return Ok(false);
        }
        let extra = self
            .session
            .request(RequestPurpose::GetTonExplorerUrl, None);
        let json = crate::telegram::requests::get_application_config(extra);
        self.send_json_request(extra, &json)?;
        self.session.ton_wallet.explorer_url_requested = true;
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
        wallet.money = MoneyPage {
            open: true,
            ..MoneyPage::default()
        };
        if !wallet.state_requested {
            wallet.state_requested = true;
            self.send_wallet(TonWalletPurpose::LoadState, None, load_ton_wallet_state)?;
        }
        if !self.session.ton_wallet.rates_requested {
            self.session.ton_wallet.rates_requested = true;
            self.send_wallet(
                TonWalletPurpose::ExchangeRates,
                None,
                get_currency_exchange_rates,
            )?;
        }
        self.fetch_ton_explorer_url()?;
        self.fetch_wallet_history(false)
    }

    /// Close the Money page (its lists are dropped; the state is kept).
    pub fn close_money_page(&mut self) {
        self.session.ton_wallet.money = MoneyPage::default();
    }

    /// The first (`append == false`) or next page of transactions.
    pub fn fetch_wallet_history(&mut self, append: bool) -> Result<(), ConnectSendError> {
        if !self.wallet_ready() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let history = &mut self.session.ton_wallet.money.history;
        if history.loading || (append && !history.has_more()) {
            return Ok(());
        }
        let offset = if append {
            history.next_offset.clone()
        } else {
            String::new()
        };
        history.loading = true;
        history.error = None;
        let sent = self.send_wallet(TonWalletPurpose::Transactions { append }, None, |extra| {
            get_ton_wallet_transactions(extra, &offset, HISTORY_PAGE)
        });
        if sent.is_err() {
            self.session.ton_wallet.money.history.loading = false;
        }
        sent.map(|_| ())
    }

    /// The first or next page of NFTs.
    pub fn fetch_wallet_nfts(&mut self, append: bool) -> Result<(), ConnectSendError> {
        if !self.wallet_ready() {
            return Err(ConnectSendError::InvalidRequest);
        }
        let nfts = &mut self.session.ton_wallet.money.nfts;
        if nfts.loading || (append && !nfts.has_more()) {
            return Ok(());
        }
        let offset = if append {
            nfts.next_offset.clone()
        } else {
            String::new()
        };
        nfts.loading = true;
        nfts.error = None;
        let sent = self.send_wallet(TonWalletPurpose::Nfts { append }, None, |extra| {
            get_ton_wallet_nfts(extra, &offset, NFT_PAGE)
        });
        if sent.is_err() {
            self.session.ton_wallet.money.nfts.loading = false;
        }
        sent.map(|_| ())
    }

    /// Switch the Money page's tab; Collectibles loads on first view.
    pub fn set_money_tab(&mut self, tab: MoneyTab) -> Result<(), ConnectSendError> {
        let money = &mut self.session.ton_wallet.money;
        money.tab = tab;
        money.selected_tx = None;
        money.selected_nft = None;
        if tab == MoneyTab::Collectibles && !money.nfts.loaded && !money.nfts.loading {
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
        if user_id <= 0
            || self.session.my_user_id == Some(user_id)
            || self
                .session
                .ton_wallet
                .profile_addresses
                .contains_key(&user_id)
        {
            return Ok(false);
        }
        self.session
            .ton_wallet
            .profile_addresses
            .insert(user_id, ProfileAddress::Loading);
        let sent = self.send_wallet(TonWalletPurpose::UserAddress, Some(user_id), |extra| {
            get_user_ton_wallet_addresses(extra, &[user_id])
        });
        if sent.is_err() {
            self.session.ton_wallet.profile_addresses.remove(&user_id);
        }
        sent.map(|_| true)
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
        if !self.session.ton_wallet.money.open {
            self.open_money_page()?;
        }
        let known = self
            .session
            .ton_wallet
            .money
            .history
            .items
            .iter()
            .find(|tx| tx.id == transaction_id)
            .cloned();
        if let Some(tx) = known {
            self.session.show_wallet_transaction(tx);
            return Ok(true);
        }
        let money = &mut self.session.ton_wallet.money;
        money.selected_tx = None;
        money.tx_error = None;
        money.tx_loading = true;
        let id = transaction_id.to_string();
        let sent = self.send_wallet(TonWalletPurpose::Transaction, None, |extra| {
            get_ton_wallet_transaction(extra, &id)
        });
        if sent.is_err() {
            self.session.ton_wallet.money.tx_loading = false;
        }
        sent.map(|_| true)
    }

    /// "Got it" (or closing) on "Your first Grams!": the hint is done, so
    /// TDLib stops suggesting it (`hideSuggestedAction`, as the iOS app
    /// dismisses the server suggestion).
    pub fn dismiss_first_transfer_hint(&mut self) -> Result<(), ConnectSendError> {
        self.session.ton_wallet.money.hint_open = false;
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
