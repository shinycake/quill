//! Telegram's TON wallet (Gram wallet): display of what arrives with
//! messages, plus the state of the shared "not supported" notice. See
//! `docs/decisions/research-ton-wallet.md` and
//! `docs/decisions/wallet-not-supported-notice.md`.
//!
//! Quill never holds wallet keys, never signs and never moves value. This
//! module only parses and words what TDLib 1.8.69 delivers:
//! `textEntityTypeTonAddress` / `richTextTonAddress` (the address helpers
//! in [`address`]), `messageTonWalletTransfer` and `messageTonConnectRequest`
//! (the contents in [`message`]), and the `pushMessageContentTon*` texts
//! (in [`push`]). Telegram Desktop has a real wallet that Quill does not
//! build; the wording follows the official iOS app (`Notification.GramTransfer*`, `Wallet.*`,
//! `MESSAGE_GRAM_TRANSFER*` in its `Localizable.strings`).
//!
//! The profile and own-wallet side (`docs/decisions/ton-wallet-profile-and-overview.md`)
//! adds a user's public address on their profile ([`ProfileAddress`]), the
//! Money page with the own address, balance, fiat value, backup status,
//! history and NFTs ([`own`], [`fiat`]), the notice for wallet links
//! ([`links`]) and the first-transfer hint. Everything that would act on the
//! wallet opens the notice ([`notice`]) instead: the methods Quill may send
//! are [`READ_ONLY_METHODS`], the ones it must never send are
//! [`FORBIDDEN_METHODS`], and every wallet getter waits for the
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

use crate::state::fetch::Fetch;
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
    /// The lookup is out under `request`; only its answer or error lands.
    Loading { request: crate::ids::RequestId },
    /// The user has a wallet at this address.
    Found(String),
    /// TDLib answered without an address for the user: no wallet.
    NoWallet,
    /// The lookup failed; the next time the profile opens asks again.
    Failed,
}

/// The rows of a paged list and where its next page starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_offset: String,
}

/// A paged, read-only list (history or NFTs): one value for "not asked",
/// "a page is in flight", "shown" and "failed". The rows already shown stay
/// reachable while another page loads or after it failed. A page is applied
/// only when its answer carries the request the list waits for, so a late
/// answer of an earlier list (the Money page was closed and opened again)
/// lands nowhere.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Paged<T> {
    /// Never asked.
    #[default]
    Idle,
    /// `request` asks for the first page, or the next one (`append`).
    Loading {
        request: crate::ids::RequestId,
        append: bool,
        shown: Option<Page<T>>,
    },
    /// The latest page landed. An empty list then means "nothing yet".
    Ready(Page<T>),
    /// The latest request failed; Retry asks again.
    Failed {
        error: String,
        shown: Option<Page<T>>,
    },
}

impl<T> Paged<T> {
    /// A list that already holds its first page (a demo or a test seed).
    pub fn set_ready(&mut self, items: Vec<T>, next_offset: String) {
        *self = Self::Ready(Page { items, next_offset });
    }

    fn shown(&self) -> Option<&Page<T>> {
        match self {
            Self::Idle => None,
            Self::Ready(page) => Some(page),
            Self::Loading { shown, .. } | Self::Failed { shown, .. } => shown.as_ref(),
        }
    }

    fn take_shown(&mut self) -> Option<Page<T>> {
        match std::mem::take(self) {
            Self::Idle => None,
            Self::Ready(page) => Some(page),
            Self::Loading { shown, .. } | Self::Failed { shown, .. } => shown,
        }
    }

    /// The rows shown, oldest request first.
    pub fn items(&self) -> &[T] {
        self.shown().map_or(&[], |page| page.items.as_slice())
    }

    pub fn next_offset(&self) -> &str {
        self.shown().map_or("", |page| page.next_offset.as_str())
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// The first page arrived (an empty list then means "nothing yet").
    pub fn is_loaded(&self) -> bool {
        self.shown().is_some()
    }

    pub fn error(&self) -> Option<&str> {
        match self {
            Self::Failed { error, .. } => Some(error),
            _ => None,
        }
    }

    pub fn has_more(&self) -> bool {
        self.shown()
            .is_some_and(|page| !page.next_offset.is_empty())
    }

    /// The request that asks for a page went out. The rows shown stay and
    /// the last error is gone.
    pub fn begin(&mut self, request: crate::ids::RequestId, append: bool) {
        let shown = self.take_shown();
        *self = Self::Loading {
            request,
            append,
            shown,
        };
    }

    /// Apply the answer of `request`: a first page replaces the list.
    /// `false` (and nothing changes) when the list waits for another one.
    pub fn apply(
        &mut self,
        request: Option<crate::ids::RequestId>,
        items: Vec<T>,
        next_offset: String,
    ) -> bool {
        let Self::Loading {
            request: waiting,
            append,
            shown,
        } = self
        else {
            return false;
        };
        if Some(*waiting) != request {
            return false;
        }
        let mut rows = match shown.take() {
            Some(page) if *append => page.items,
            _ => Vec::new(),
        };
        rows.extend(items);
        *self = Self::Ready(Page {
            items: rows,
            next_offset,
        });
        true
    }

    /// The error of `request`: the rows shown stay. `false` when the list
    /// waits for another request.
    pub fn fail(&mut self, request: Option<crate::ids::RequestId>, error: String) -> bool {
        let Self::Loading {
            request: waiting, ..
        } = self
        else {
            return false;
        };
        if Some(*waiting) != request {
            return false;
        }
        let shown = self.take_shown();
        *self = Self::Failed { error, shown };
        true
    }

    /// The request could not be sent: back to what the list held.
    pub fn send_failed(&mut self, error: String) {
        let shown = self.take_shown();
        *self = Self::Failed { error, shown };
    }
}

/// The Money page's tabs (iOS `Wallet.Transactions`, `Wallet.Collectibles`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MoneyTab {
    #[default]
    Transactions,
    Collectibles,
}

/// What the Money page shows over its lists: nothing (the overview), one
/// transaction, one NFT, or the lookup of a clicked transfer card's
/// transaction. One at a time, so "back" is one assignment.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum MoneyDetail {
    #[default]
    Overview,
    /// The transaction whose details are shown; `hint` brings "Your first
    /// Grams!" up over them until "Got it".
    Tx { tx: Box<WalletTx>, hint: bool },
    /// The NFT whose details are shown (by address).
    Nft(String),
    /// The lookup of a clicked transfer card's transaction. The answer
    /// shows the details only while the lookup is for the request that
    /// answers; "back" or another click replaces it, so a late answer for
    /// an earlier card is dropped.
    Lookup { request: crate::ids::RequestId },
    /// The text shown instead of the details.
    LookupFailed(String),
}

/// The Money page (iOS `WalletScreen`, read-only).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoneyPage {
    pub tab: MoneyTab,
    pub history: Paged<WalletTx>,
    pub nfts: Paged<WalletNft>,
    pub detail: MoneyDetail,
}

impl MoneyPage {
    /// Whether the card lookup waits for the answer of `request`.
    pub fn tx_lookup_waits_for(&self, request: Option<crate::ids::RequestId>) -> bool {
        request.is_some_and(|id| self.detail == MoneyDetail::Lookup { request: id })
    }

    /// The text of a failed lookup.
    pub fn tx_error(&self) -> Option<&str> {
        match &self.detail {
            MoneyDetail::LookupFailed(text) => Some(text),
            _ => None,
        }
    }

    /// The transaction whose details are shown.
    pub fn selected_tx(&self) -> Option<&WalletTx> {
        match &self.detail {
            MoneyDetail::Tx { tx, .. } => Some(tx),
            _ => None,
        }
    }

    /// "Your first Grams!" is up over the details.
    pub fn hint_open(&self) -> bool {
        matches!(self.detail, MoneyDetail::Tx { hint: true, .. })
    }

    /// The NFT whose details are shown.
    pub fn selected_nft(&self) -> Option<&WalletNft> {
        let MoneyDetail::Nft(address) = &self.detail else {
            return None;
        };
        self.nfts.items().iter().find(|nft| nft.address == *address)
    }
}

/// Wallet state Quill keeps. Nothing secret: the address, balance and
/// history are public on the blockchain.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TonWalletState {
    /// `ton_blockchain_explorer_url` from `getApplicationConfig` (`None`
    /// when the config names none): asked once per session, again after it
    /// failed. [`address::DEFAULT_EXPLORER_URL`] is used until it is known.
    pub explorer: Fetch<Option<String>>,
    /// The `can_use_ton_wallet` option: when false nothing wallet-related
    /// is shown and no wallet getter is sent.
    pub can_use: bool,
    /// `ton_usd_rate` from the app config: the Gram's price in USD.
    pub ton_usd_rate: Option<f64>,
    /// The own wallet, from `updateTonWalletState` (`loadTonWalletState`
    /// is sent once per session, again after it failed; its updates also
    /// land whenever TDLib sends them).
    pub own: Fetch<OwnWallet>,
    /// `currencyExchangeRates`: `(code, units per USD)`, asked once per
    /// session, again after it failed.
    pub rates: Fetch<Vec<(String, f64)>>,
    /// The fiat currency the user picked; `None` until the account's prefs
    /// are read ([`fiat::DEFAULT_CURRENCY`] is used meanwhile).
    pub currency: Option<String>,
    /// The Money page while it is open; `None` when closed.
    pub money: Option<MoneyPage>,
    /// Public addresses of profiles opened this session.
    pub profile_addresses: HashMap<i64, ProfileAddress>,
    /// The surface whose notice is open, if any.
    pub notice: Option<WalletSurface>,
}

impl TonWalletState {
    /// The app config asked for the explorer came back as `request`, with
    /// the explorer it carries. Any app-config answer carries the current
    /// one, so one that names none leaves a known explorer in place.
    pub fn explorer_answered(
        &mut self,
        request: Option<crate::ids::RequestId>,
        url: Option<String>,
    ) {
        let url = url.or_else(|| self.explorer.data().cloned().flatten());
        self.explorer.answered(request, url);
    }

    /// The own wallet, once known.
    pub fn own(&self) -> Option<&OwnWallet> {
        self.own.data()
    }

    /// The exchange rates, once known.
    pub fn rates(&self) -> &[(String, f64)] {
        self.rates.data().map_or(&[], Vec::as_slice)
    }

    /// The explorer address, once the config named one.
    pub fn explorer_url(&self) -> Option<&str> {
        self.explorer.data().and_then(|url| url.as_deref())
    }

    /// Whether the Money page is open.
    pub fn money_open(&self) -> bool {
        self.money.is_some()
    }

    /// The Money page, or the empty page while it is closed.
    pub fn money_page(&self) -> &MoneyPage {
        static CLOSED: std::sync::LazyLock<MoneyPage> =
            std::sync::LazyLock::new(MoneyPage::default);
        self.money.as_ref().unwrap_or(&CLOSED)
    }

    /// Where "View in Explorer" opens `address`.
    pub fn explorer_url_for(&self, address: &str) -> String {
        explorer_url(self.explorer_url(), address)
    }

    /// The fiat currency in use.
    pub fn currency(&self) -> &str {
        self.currency.as_deref().unwrap_or(fiat::DEFAULT_CURRENCY)
    }

    /// Units of the chosen currency one Gram is worth, once both rates are
    /// known.
    pub fn units_per_gram(&self) -> Option<f64> {
        fiat::units_per_gram(self.ton_usd_rate, self.rates(), self.currency())
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

    /// Apply the `can_use_ton_wallet` option. Turning it off drops
    /// everything the wallet loaded (and any answer still in flight, which
    /// no fetch waits for any more), closes the Money page and its notice.
    /// What the app config gave (the explorer, the Gram's price) and the
    /// currency the user picked stay.
    pub fn set_can_use(&mut self, on: bool) {
        if on {
            self.can_use = true;
            return;
        }
        *self = Self {
            explorer: std::mem::take(&mut self.explorer),
            ton_usd_rate: self.ton_usd_rate,
            currency: self.currency.take(),
            ..Self::default()
        };
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
