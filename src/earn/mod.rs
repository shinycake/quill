//! Channel and bot revenue (TON ad rewards, Stars proceeds, withdrawals
//! through Fragment) and affiliate programs of mini apps, after Telegram
//! Desktop's `info/channel_statistics/earn/*`, `info/bot/earn/*`,
//! `info/bot/starref/*` and `api/api_earn.cpp`.
//!
//! Pure core: the TDLib types, formatting, the page state, the requests
//! and the parser. The driver is `connect::earn`, the reducer
//! `state::domains::payments::earn`, the screens `ui::earn`. Nothing here
//! moves funds: a withdrawal only fetches a Fragment URL for the browser.
#![deny(clippy::indexing_slicing)]

mod affiliate;
mod format;
mod payload;
mod requests;
mod revenue;
mod state;
#[cfg(test)]
mod tests;

pub use affiliate::*;
pub use format::*;
pub use payload::*;
pub use requests::*;
pub use revenue::*;
pub use state::*;

use crate::telegram::envelope::MessageSender;
use serde_json::{Value, json};

pub(crate) fn int(value: Option<&Value>) -> i64 {
    value
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

pub(crate) fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Whose revenue or affiliate links a screen is about: a user (the
/// signed-in user or a bot) or a chat (a channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EarnPeer {
    User(i64),
    Chat(i64),
}

impl EarnPeer {
    pub fn from_sender(sender: MessageSender) -> Self {
        match sender {
            MessageSender::User { user_id } => Self::User(user_id),
            MessageSender::Chat { chat_id } => Self::Chat(chat_id),
        }
    }

    pub fn sender(self) -> MessageSender {
        match self {
            Self::User(user_id) => MessageSender::User { user_id },
            Self::Chat(chat_id) => MessageSender::Chat { chat_id },
        }
    }

    /// `messageSenderUser` / `messageSenderChat`.
    pub fn sender_json(self) -> Value {
        match self {
            Self::User(user_id) => json!({"@type": "messageSenderUser", "user_id": user_id}),
            Self::Chat(chat_id) => json!({"@type": "messageSenderChat", "chat_id": chat_id}),
        }
    }

    /// The chat of the peer: a user's private chat has the user's id.
    pub fn chat_id(self) -> i64 {
        match self {
            Self::User(id) | Self::Chat(id) => id,
        }
    }

    /// `AffiliateType` for this peer as seen by `my_user_id`.
    pub fn affiliate_json(self, my_user_id: Option<i64>) -> Value {
        match self {
            Self::User(id) if Some(id) == my_user_id => {
                json!({"@type": "affiliateTypeCurrentUser"})
            }
            Self::User(id) => json!({"@type": "affiliateTypeBot", "user_id": id}),
            Self::Chat(id) => json!({"@type": "affiliateTypeChannel", "chat_id": id}),
        }
    }
}

/// In-flight requests of the revenue and affiliate screens; wrapped as
/// `PaymentsPurpose::Earn`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnPurpose {
    /// `getChatRevenueStatistics` for the open page.
    TonStats { peer: EarnPeer },
    /// `getChatRevenueStatistics` for a profile balance row only.
    TonBalance { peer: EarnPeer },
    /// `getChatRevenueTransactions`.
    TonTransactions { peer: EarnPeer, append: bool },
    /// `getStarRevenueStatistics` for the open page.
    StarStats { peer: EarnPeer },
    /// `getStarTransactions` for the open page's history.
    StarTransactions {
        peer: EarnPeer,
        filter: crate::premium_hub::TxFilter,
        append: bool,
    },
    /// `getStarTransactions` with one row, for a profile balance.
    StarBalance { peer: EarnPeer },
    /// `getStarAdAccountUrl`.
    AdAccountUrl { peer: EarnPeer },
    /// `getChatRevenueWithdrawalUrl` / `getStarWithdrawalUrl`;
    /// `with_password` is false for the first, password-less try.
    WithdrawalUrl { with_password: bool },
    /// `getApplicationConfig` for the affiliate switches.
    AppConfig,
    /// `setChatAffiliateProgram`; `end` when ending the program.
    SetProgram { bot_user_id: i64, end: bool },
    /// `searchAffiliatePrograms` for the join page or the programs box.
    SearchPrograms { for_box: bool, append: bool },
    /// `getConnectedAffiliatePrograms`.
    ConnectedPrograms { append: bool },
    /// `getConnectedAffiliateProgram` after a click on a program;
    /// `choose_recipient` for the user's own page and the programs box.
    ConnectedProgram {
        bot_user_id: i64,
        recipient: EarnPeer,
        choose_recipient: bool,
    },
    /// `connectAffiliateProgram` from the join box.
    Connect { recipient: EarnPeer },
    /// `disconnectAffiliateProgram` from the revoke box.
    Disconnect,
    /// `getCreatedPublicChats` for the recipients.
    RecipientChats,
    /// `getOwnedBots` for the recipients.
    RecipientBots,
}

/// TDLib errors the revenue screens tell apart (tdesktop
/// `HandleWithdrawalButton`, `PrePasswordErrorBox`, the megagroup check of
/// `ChannelEarn`), and the low balance of a paid suggested post.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EarnErrorHint {
    /// `AMOUNT_TOO_SMALL`.
    AmountTooSmall,
    /// `PASSWORD_MISSING`: two-step verification is off.
    PasswordMissing,
    /// `PASSWORD_TOO_FRESH_*` / `SESSION_TOO_FRESH_*`: come back later.
    TooFresh,
    /// `PASSWORD_HASH_INVALID`: the password is wrong (or was empty).
    PasswordInvalid,
    /// `BROADCAST_REQUIRED`: a group has no TON ad revenue.
    BroadcastRequired,
    /// `BALANCE_TOO_LOW`: not enough Stars or Grams to pay for a suggested
    /// post (tdesktop `api_suggest_post.cpp` asks for a top-up).
    BalanceTooLow,
}

/// Classify a TDLib error message; `None` for anything else.
pub fn error_hint(message: &str) -> Option<EarnErrorHint> {
    Some(match message {
        "AMOUNT_TOO_SMALL" => EarnErrorHint::AmountTooSmall,
        "PASSWORD_MISSING" => EarnErrorHint::PasswordMissing,
        "PASSWORD_HASH_INVALID" => EarnErrorHint::PasswordInvalid,
        "BROADCAST_REQUIRED" => EarnErrorHint::BroadcastRequired,
        "BALANCE_TOO_LOW" => EarnErrorHint::BalanceTooLow,
        m if m.starts_with("PASSWORD_TOO_FRESH_") || m.starts_with("SESSION_TOO_FRESH_") => {
            EarnErrorHint::TooFresh
        }
        _ => return None,
    })
}
