//! The two wallet message contents of TDLib 1.8.68 and their one-line
//! wording outside the history (chat list, notifications).

use super::SERVICE_NOTIFICATIONS_CHAT_ID;
use super::amount::format_grams;
use serde_json::Value;

/// `messageTonWalletTransfer` (td_api.tl:5870): a transfer with the
/// current user's TON wallet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TonWalletTransfer {
    /// Sender of the transfer; 0 if unknown.
    pub sender_user_id: i64,
    /// Empty while the transfer is not sent yet.
    pub transaction_id: String,
    /// TON address of the other side.
    pub peer_address: String,
    /// Nano-units; negative for an outgoing transfer.
    pub amount: i64,
    pub comment: String,
    /// The comment is encrypted for the wallets' keys, which Quill never
    /// holds, so it is never shown.
    pub is_comment_encrypted: bool,
}

impl TonWalletTransfer {
    pub fn from_td(value: &Value) -> Self {
        Self {
            sender_user_id: int(value, "sender_user_id"),
            transaction_id: string(value, "transaction_id"),
            peer_address: string(value, "peer_address"),
            amount: int(value, "amount"),
            comment: string(value, "comment"),
            is_comment_encrypted: value
                .get("is_comment_encrypted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// Received (TDLib makes outgoing amounts negative).
    pub fn is_received(&self) -> bool {
        self.amount >= 0
    }

    /// The comment the card prints: a public one only.
    pub fn public_comment(&self) -> Option<&str> {
        let comment = self.comment.trim();
        (!self.is_comment_encrypted && !comment.is_empty()).then_some(comment)
    }

    /// The card shows a hidden placeholder for an encrypted comment.
    pub fn has_encrypted_comment(&self) -> bool {
        self.is_comment_encrypted
    }

    /// The chat-list and notification line of a received transfer
    /// (`incomingGramTransferPreview`): "sent you 5 Grams" under the
    /// sender's chat, "Someone sent you 5 Grams" in the service chat.
    /// `None` for a sent transfer, which uses the service wording.
    pub fn received_preview(&self, chat_id: i64, with_diamond: bool) -> Option<String> {
        if !self.is_received() {
            return None;
        }
        let diamond = if with_diamond { "\u{1F48E}" } else { "" };
        let amount = format!("{diamond}{}", format_grams(self.amount));
        Some(if chat_id == SERVICE_NOTIFICATIONS_CHAT_ID {
            format!("Someone sent you {amount}")
        } else {
            format!("sent you {amount}")
        })
    }
}

/// `TonConnectRequestState` (td_api.tl:2379-2385).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TonConnectRequestState {
    Pending { expiration_date: i32 },
    Accepted,
    Rejected,
}

/// `messageTonConnectRequest` (td_api.tl:5878): a dApp request that only
/// the user's official app can answer. Quill never claims or answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TonConnectRequest {
    pub session_id: i64,
    pub state: TonConnectRequestState,
    /// May be empty if unknown.
    pub dapp_name: String,
    pub topic: String,
}

impl TonConnectRequest {
    pub fn from_td(value: &Value) -> Self {
        let state = value.get("state");
        let state = match state.and_then(|s| s.get("@type")).and_then(Value::as_str) {
            Some("tonConnectRequestStateAccepted") => TonConnectRequestState::Accepted,
            Some("tonConnectRequestStateRejected") => TonConnectRequestState::Rejected,
            _ => TonConnectRequestState::Pending {
                expiration_date: state
                    .map_or(0, |s| int(s, "expiration_date"))
                    .clamp(0, i64::from(i32::MAX)) as i32,
            },
        };
        Self {
            session_id: int(value, "session_id"),
            state,
            dapp_name: string(value, "dapp_name"),
            topic: string(value, "topic"),
        }
    }

    /// The service line (iOS `walletTonConnectRequest`): "Wallet request
    /// from Fragment", "… accepted", "… declined"; without a name,
    /// "Wallet request".
    pub fn service_line(&self) -> String {
        let name = self.dapp_name.trim();
        let base = if name.is_empty() {
            "Wallet request".to_string()
        } else {
            format!("Wallet request from {name}")
        };
        match self.state {
            TonConnectRequestState::Pending { .. } => base,
            TonConnectRequestState::Accepted => format!("{base} accepted"),
            TonConnectRequestState::Rejected => format!("{base} declined"),
        }
    }
}

fn int(value: &Value, key: &str) -> i64 {
    value
        .get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
        .unwrap_or(0)
}

fn string(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
