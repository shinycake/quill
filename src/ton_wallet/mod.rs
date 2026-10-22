//! Telegram's TON wallet (Gram wallet), phase 1: read-only display of what
//! arrives with messages. See `docs/decisions/research-ton-wallet.md`.
//!
//! Quill never holds wallet keys, never signs and never moves value. This
//! module only parses and words what TDLib 1.8.68 delivers:
//! `textEntityTypeTonAddress` / `richTextTonAddress` (the address helpers
//! in [`address`]), `messageTonWalletTransfer` and `messageTonConnectRequest`
//! (the contents in [`message`]), and the `pushMessageContentTon*` texts
//! (in [`push`]). Telegram Desktop has none of this; the wording follows
//! the official iOS app (`Notification.GramTransfer*`, `Wallet.*`,
//! `MESSAGE_GRAM_TRANSFER*` in its `Localizable.strings`).
//!
//! The profile and own-wallet side (`docs/decisions/ton-wallet-profile-and-overview.md`)
//! adds a user's public address on their profile ([`ProfileAddress`]), the
//! Money page with the own address, balance, fiat value, backup status,
//! history and NFTs ([`own`], [`fiat`]), the notice for wallet links
//! ([`links`]) and the first-transfer hint. All of it only reads: the
//! methods it may send are [`READ_ONLY_METHODS`], the ones it must never
//! send are [`FORBIDDEN_METHODS`], and every wallet getter waits for the
//! `can_use_ton_wallet` option.
#![deny(clippy::indexing_slicing)]

pub mod address;
pub mod amount;
pub mod fiat;
pub mod links;
pub mod message;
pub mod notice;
pub mod own;
pub mod payload;
pub mod push;

pub use address::{explorer_url, group_address, is_valid_ton_address, transaction_explorer_url};
pub use amount::{format_gram_amount, format_gram_amount_with, format_grams, signed_gram_amount};
pub use message::{TonConnectRequest, TonConnectRequestState, TonWalletTransfer};
pub use notice::{OFFICIAL_APPS_URL, WalletSurface};
pub use own::{NftPage, OwnWallet, TxKind, TxPage, WalletNft, WalletTx};
pub use payload::{TonWalletPayload, TonWalletPurpose, parse_payload};

use std::collections::HashMap;

/// The "Telegram" service-notifications chat (user 777000). Transfers
/// that arrive there have no known sender: "Someone sent you …".
pub const SERVICE_NOTIFICATIONS_CHAT_ID: i64 = 777_000;

/// TDLib methods a read-only Quill must never send: they need the wallet's
/// private key, reveal or store the secret phrase, move value, or claim a
/// TON Connect request away from the user's official device. A test scans
/// the request builders and the connect driver for them.
pub const FORBIDDEN_METHODS: &[&str] = &[
    "getTonWalletSecretPhrase",
    "enableTonWalletBackup",
    "disableTonWalletBackup",
    "disableTonWalletBackupWithProof",
    "getTonWalletOwnershipProofChallenge",
    "replaceTonWallet",
    "deleteTonWallet",
    "sendTonWalletTransfer",
    "createUserTonWallet",
    "createTonConnectSession",
    "setTonConnectSessionWalletClientId",
    "sendTonConnectSessionConnectResult",
    "claimTonConnectRequest",
    "answerTonConnectRequest",
    "disconnectTonConnectSession",
    "createOnRampPaymentSession",
    "sendTonCenterApiRequest",
];

/// The wallet methods Quill sends, all read-only and none needing a key:
/// the own state, history and NFTs, a profile's public address and the
/// exchange rates for the fiat value. A test checks that no other wallet
/// method is built.
pub const READ_ONLY_METHODS: &[&str] = &[
    "loadTonWalletState",
    "getTonWalletTransactions",
    "getTonWalletTransaction",
    "getTonWalletNfts",
    "getUserTonWalletAddresses",
    "getCurrencyExchangeRates",
];

/// The suggested action behind the first-transfer hint.
pub const FIRST_TRANSFER_HINT: &str = "suggestedActionViewFirstTonWalletTransferHint";

/// The option that turns the wallet on (`getOption("can_use_ton_wallet")`).
pub const CAN_USE_OPTION: &str = "can_use_ton_wallet";

/// Transactions asked per page.
pub const HISTORY_PAGE: i32 = 20;

/// NFTs asked per page: `getTonWalletNfts` allows at most 20.
pub const NFT_PAGE: i32 = 20;

/// A user's public wallet address, fetched when their profile opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileAddress {
    Loading,
    /// The user has a wallet at this address.
    Found(String),
    /// The user has no wallet, or the lookup failed.
    None,
}

/// A paged, read-only list (history or NFTs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub next_offset: String,
    pub loading: bool,
    /// The first page arrived (an empty list then means "nothing yet").
    pub loaded: bool,
    pub error: Option<String>,
}

impl<T> Default for Paged<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            next_offset: String::new(),
            loading: false,
            loaded: false,
            error: None,
        }
    }
}

impl<T> Paged<T> {
    /// Apply one page: a first page replaces the list.
    pub fn apply(&mut self, items: Vec<T>, next_offset: String, append: bool) {
        if !append {
            self.items.clear();
        }
        self.items.extend(items);
        self.next_offset = next_offset;
        self.loading = false;
        self.loaded = true;
        self.error = None;
    }

    /// The request failed: stop loading and keep what is shown.
    pub fn fail(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    pub fn has_more(&self) -> bool {
        self.loaded && !self.next_offset.is_empty()
    }
}

/// The Money page's tabs (iOS `Wallet.Transactions`, `Wallet.Collectibles`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MoneyTab {
    #[default]
    Transactions,
    Collectibles,
}

/// The Money page (iOS `WalletScreen`, read-only).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoneyPage {
    pub open: bool,
    pub tab: MoneyTab,
    pub history: Paged<WalletTx>,
    pub nfts: Paged<WalletNft>,
    /// The transaction whose details are shown.
    pub selected_tx: Option<WalletTx>,
    /// The NFT whose details are shown (by address).
    pub selected_nft: Option<String>,
    /// A clicked transfer card's transaction is being looked up.
    pub tx_loading: bool,
    /// The lookup failed: the text shown instead of the details.
    pub tx_error: Option<String>,
    /// The "Your first Grams!" info is up over the details.
    pub hint_open: bool,
}

impl MoneyPage {
    /// The NFT whose details are shown.
    pub fn selected_nft(&self) -> Option<&WalletNft> {
        let address = self.selected_nft.as_deref()?;
        self.nfts.items.iter().find(|nft| nft.address == address)
    }
}

/// Wallet state Quill keeps. Nothing secret: the address, balance and
/// history are public on the blockchain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TonWalletState {
    /// `ton_blockchain_explorer_url` from `getApplicationConfig`, once
    /// known; [`address::DEFAULT_EXPLORER_URL`] is used until then.
    pub explorer_url: Option<String>,
    /// A `getApplicationConfig` for the explorer address is in flight or
    /// done, so it is asked once per session.
    pub explorer_url_requested: bool,
    /// The `can_use_ton_wallet` option: when false nothing wallet-related
    /// is shown and no wallet getter is sent.
    pub can_use: bool,
    /// `ton_usd_rate` from the app config: the Gram's price in USD.
    pub ton_usd_rate: Option<f64>,
    /// The own wallet, from `updateTonWalletState`.
    pub own: Option<OwnWallet>,
    /// `loadTonWalletState` was sent this session.
    pub state_requested: bool,
    /// `currencyExchangeRates`: `(code, units per USD)`.
    pub rates: Vec<(String, f64)>,
    /// `getCurrencyExchangeRates` is in flight or done.
    pub rates_requested: bool,
    /// The fiat currency the user picked; `None` until the account's prefs
    /// are read ([`fiat::DEFAULT_CURRENCY`] is used meanwhile).
    pub currency: Option<String>,
    pub money: MoneyPage,
    /// Public addresses of profiles opened this session.
    pub profile_addresses: HashMap<i64, ProfileAddress>,
    pub notice: Option<WalletSurface>,
}

impl TonWalletState {
    /// Where "View in Explorer" opens `address`.
    pub fn explorer_url_for(&self, address: &str) -> String {
        explorer_url(self.explorer_url.as_deref(), address)
    }

    /// The fiat currency in use.
    pub fn currency(&self) -> &str {
        self.currency.as_deref().unwrap_or(fiat::DEFAULT_CURRENCY)
    }

    /// Units of the chosen currency one Gram is worth, once both rates are
    /// known.
    pub fn units_per_gram(&self) -> Option<f64> {
        fiat::units_per_gram(self.ton_usd_rate, &self.rates, self.currency())
    }

    /// The fiat value of `nanos` in the chosen currency ("—" until known).
    pub fn fiat_text(&self, nanos: i64) -> String {
        fiat::fiat_balance(nanos, self.units_per_gram(), self.currency())
    }

    /// The public address shown on `user_id`'s profile, if any. Nothing
    /// shows while the wallet option is off.
    pub fn profile_address(&self, user_id: i64) -> Option<&str> {
        if !self.can_use {
            return None;
        }
        match self.profile_addresses.get(&user_id) {
            Some(ProfileAddress::Found(address)) => Some(address.as_str()),
            _ => None,
        }
    }

    /// Apply the `can_use_ton_wallet` option. Turning it off drops what was
    /// loaded and closes the Money page.
    pub fn set_can_use(&mut self, on: bool) {
        self.can_use = on;
        if !on {
            self.own = None;
            self.state_requested = false;
            self.money = MoneyPage::default();
            self.profile_addresses.clear();
        }
    }
}

/// The account's wallet preferences (`ton_wallet_prefs.json`): only the
/// fiat currency, like the iOS app's stored `selectedFiatCurrency`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WalletPrefs {
    #[serde(default)]
    pub currency: Option<String>,
}

const PREFS_FILE: &str = "ton_wallet_prefs.json";

/// Load the wallet prefs; a missing or broken file gives the defaults.
pub fn load_prefs(paths: &crate::settings::AccountPaths) -> WalletPrefs {
    std::fs::read(paths.root.join(PREFS_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Save the wallet prefs.
pub fn save_prefs(
    paths: &crate::settings::AccountPaths,
    prefs: &WalletPrefs,
) -> std::io::Result<()> {
    crate::settings::write_json_atomic(&paths.root.join(PREFS_FILE), prefs)
}

#[cfg(test)]
mod own_tests;
#[cfg(test)]
mod tests;
