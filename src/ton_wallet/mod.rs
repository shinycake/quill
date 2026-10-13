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
//! Later wallet work (profile addresses, own balance) builds on this module;
//! the methods it must never call are listed in [`FORBIDDEN_METHODS`].

pub mod address;
pub mod amount;
pub mod message;
pub mod push;

pub use address::{explorer_url, group_address, is_valid_ton_address};
pub use amount::{format_gram_amount, format_grams, signed_gram_amount};
pub use message::{TonConnectRequest, TonConnectRequestState, TonWalletTransfer};

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

/// Wallet state Quill keeps: only the app-config explorer address for now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TonWalletState {
    /// `ton_blockchain_explorer_url` from `getApplicationConfig`, once
    /// known; [`address::DEFAULT_EXPLORER_URL`] is used until then.
    pub explorer_url: Option<String>,
    /// A `getApplicationConfig` for the explorer address is in flight or
    /// done, so it is asked once per session.
    pub explorer_url_requested: bool,
}

impl TonWalletState {
    /// Where "View in Explorer" opens `address`.
    pub fn explorer_url_for(&self, address: &str) -> String {
        explorer_url(self.explorer_url.as_deref(), address)
    }
}

#[cfg(test)]
mod tests;
