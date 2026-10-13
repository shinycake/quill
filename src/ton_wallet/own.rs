//! The signed-in user's own wallet, read-only: the state TDLib reports
//! (`updateTonWalletState`), the transaction history
//! (`getTonWalletTransactions`) and the NFTs (`getTonWalletNfts`). Nothing
//! here holds a key: an encrypted comment is kept only as a flag, never
//! as its bytes.

use crate::telegram::envelope::ParsedFile;
use crate::telegram::envelope::parse_file;
use serde_json::Value;

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// TDLib sends `int53` as a number, `int64` as a string; accept both.
fn int(value: &Value, key: &str) -> i64 {
    match value.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// `tonWalletState` without the public key: what the Money page shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnWallet {
    /// May be empty while the wallet is being created.
    pub address: String,
    /// Balance in nano-Grams.
    pub gram_amount: i64,
    pub is_being_created: bool,
    /// The encrypted server backup is on (shown as a status only).
    pub is_backup_enabled: bool,
}

impl OwnWallet {
    /// `updateTonWalletState.state` → the wallet.
    pub fn parse(state: &Value) -> Self {
        Self {
            address: text(state, "address"),
            gram_amount: int(state, "gram_amount"),
            is_being_created: flag(state, "is_being_created"),
            is_backup_enabled: flag(state, "is_backup_enabled"),
        }
    }

    /// Whether there is a wallet to show (TDLib: "address may be empty if
    /// none").
    pub fn exists(&self) -> bool {
        !self.address.is_empty()
    }
}

/// One NFT (`tonNft`), with only what a read-only list shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalletNft {
    pub address: String,
    pub collection_address: String,
    /// Decimal index within the collection.
    pub index: String,
    pub name: String,
    pub description: String,
    /// `(trait_type, value)` pairs.
    pub attributes: Vec<(String, String)>,
    /// The thumbnail (or the image when there is none) to draw.
    pub thumbnail: Option<ParsedFile>,
}

impl WalletNft {
    pub fn parse(value: &Value) -> Self {
        let photo = |key: &str| {
            value
                .get(key)
                .filter(|size| !size.is_null())
                .and_then(|size| parse_file(size.get("photo")).ok())
        };
        Self {
            address: text(value, "address"),
            collection_address: text(value, "collection_address"),
            index: text(value, "index"),
            name: text(value, "name"),
            description: text(value, "description"),
            attributes: value
                .get("attributes")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .map(|a| (text(a, "trait_type"), text(a, "value")))
                        .filter(|(kind, value)| !kind.is_empty() || !value.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            thumbnail: photo("thumbnail").or_else(|| photo("image")),
        }
    }

    /// The name, or "Collectible" when the NFT has none (iOS
    /// `Wallet.Transaction.Collectible`).
    pub fn title(&self) -> &str {
        if self.name.trim().is_empty() {
            "Collectible"
        } else {
            self.name.as_str()
        }
    }
}

/// `tonNfts`: a page of NFTs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NftPage {
    pub nfts: Vec<WalletNft>,
    pub next_offset: String,
}

impl NftPage {
    pub fn parse(value: &Value) -> Self {
        Self {
            nfts: value
                .get("nfts")
                .and_then(Value::as_array)
                .map(|list| list.iter().map(WalletNft::parse).collect())
                .unwrap_or_default(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// `TonWalletTransactionType`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxKind {
    /// A purchase through an on-ramp provider landed.
    OnRampDeposit { amount: i64, provider: String },
    /// Grams in or out; `amount` is negative for outgoing.
    Transfer {
        amount: i64,
        is_gasless: bool,
        comment: String,
        is_comment_encrypted: bool,
    },
    /// An NFT in or out.
    NftTransfer {
        nft: Box<WalletNft>,
        is_outgoing: bool,
        comment: String,
        is_comment_encrypted: bool,
    },
    /// The wallet key changed.
    KeyChange,
    /// A type this build does not know.
    Unknown,
}

/// One `tonWalletTransaction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalletTx {
    pub id: String,
    pub peer_address: String,
    /// 0 when unknown.
    pub peer_user_id: i64,
    pub peer_domain: String,
    pub date: i32,
    pub fee_amount: i64,
    /// `None` when the transaction failed.
    pub tx_hash: Option<String>,
    pub kind: TxKind,
}

impl WalletTx {
    pub fn parse(value: &Value) -> Option<Self> {
        let id = text(value, "id");
        if id.is_empty() {
            return None;
        }
        let state = value.get("state");
        let tx_hash = match state.and_then(|s| s.get("@type")).and_then(Value::as_str) {
            Some("tonWalletTransactionStateSucceeded") => {
                Some(state.map(|s| text(s, "tx_hash")).unwrap_or_default())
            }
            _ => None,
        };
        let kind_value = value.get("type").cloned().unwrap_or(Value::Null);
        let kind = match kind_value.get("@type").and_then(Value::as_str) {
            Some("tonWalletTransactionTypeOnRampDeposit") => TxKind::OnRampDeposit {
                amount: int(&kind_value, "amount"),
                provider: text(&kind_value, "provider_name"),
            },
            Some("tonWalletTransactionTypeTransfer") => TxKind::Transfer {
                amount: int(&kind_value, "amount"),
                is_gasless: flag(&kind_value, "is_gasless"),
                comment: text(&kind_value, "comment"),
                is_comment_encrypted: flag(&kind_value, "is_comment_encrypted"),
            },
            Some("tonWalletTransactionTypeNftTransfer") => TxKind::NftTransfer {
                nft: Box::new(
                    kind_value
                        .get("nft")
                        .map(WalletNft::parse)
                        .unwrap_or_default(),
                ),
                is_outgoing: flag(&kind_value, "is_outgoing"),
                comment: text(&kind_value, "comment"),
                is_comment_encrypted: flag(&kind_value, "is_comment_encrypted"),
            },
            Some("tonWalletTransactionTypeKeyChange") => TxKind::KeyChange,
            _ => TxKind::Unknown,
        };
        Some(Self {
            id,
            peer_address: text(value, "peer_address"),
            peer_user_id: int(value, "peer_user_id"),
            peer_domain: text(value, "peer_domain"),
            date: i32::try_from(int(value, "date")).unwrap_or(0),
            fee_amount: int(value, "fee_amount"),
            tx_hash,
            kind,
        })
    }

    pub fn failed(&self) -> bool {
        self.tx_hash.is_none()
    }

    /// Whether value came in (iOS `.incoming`).
    pub fn is_incoming(&self) -> bool {
        match &self.kind {
            TxKind::OnRampDeposit { .. } => true,
            TxKind::Transfer { amount, .. } => *amount > 0,
            TxKind::NftTransfer { is_outgoing, .. } => !is_outgoing,
            TxKind::KeyChange | TxKind::Unknown => false,
        }
    }

    /// An incoming Gram transfer: the kind that brings up the first-transfer
    /// hint (iOS `checkFirstGramsOnAppear`).
    pub fn is_incoming_gram_transfer(&self) -> bool {
        matches!(self.kind, TxKind::Transfer { amount, .. } if amount > 0)
    }

    /// The public comment, if any. An encrypted one is never returned.
    pub fn public_comment(&self) -> Option<&str> {
        match &self.kind {
            TxKind::Transfer {
                comment,
                is_comment_encrypted: false,
                ..
            }
            | TxKind::NftTransfer {
                comment,
                is_comment_encrypted: false,
                ..
            } if !comment.trim().is_empty() => Some(comment.as_str()),
            _ => None,
        }
    }

    pub fn has_encrypted_comment(&self) -> bool {
        matches!(
            &self.kind,
            TxKind::Transfer { comment, is_comment_encrypted: true, .. }
                | TxKind::NftTransfer { comment, is_comment_encrypted: true, .. }
                if !comment.is_empty()
        )
    }

    /// The row's subtitle (iOS `WalletTransactionItemComponent`):
    /// "Incoming transfer" from a user, "Deposit" from an address, the
    /// provider for a top-up, "Failed" when it failed.
    pub fn subtitle(&self) -> String {
        if self.failed() {
            return "Failed".into();
        }
        let from_user = self.peer_user_id != 0;
        match &self.kind {
            TxKind::KeyChange => "Key Update".into(),
            TxKind::OnRampDeposit { provider, .. } if !provider.is_empty() => provider.clone(),
            TxKind::NftTransfer { .. } if self.is_incoming() => "Incoming collectible".into(),
            TxKind::NftTransfer { .. } => "Outgoing collectible".into(),
            _ if self.is_incoming() && from_user => "Incoming transfer".into(),
            _ if self.is_incoming() => "Deposit".into(),
            TxKind::Unknown => String::new(),
            _ if from_user => "Outgoing transfer".into(),
            _ => "Withdrawal".into(),
        }
    }

    /// The row's amount: "+2.5", "–1" (an en dash, as iOS), "+1 item" for
    /// an NFT; the key change shows what it cost.
    pub fn amount_text(&self) -> String {
        let signed = |value: i64, decimals: usize| {
            let sign = if value > 0 { '+' } else { '\u{2013}' };
            format!(
                "{sign}{}",
                super::amount::format_gram_amount_with(value, decimals)
            )
        };
        match &self.kind {
            TxKind::Transfer { amount, .. } => signed(*amount, 3),
            TxKind::OnRampDeposit { amount, .. } => signed(amount.saturating_abs(), 3),
            TxKind::NftTransfer { .. } => {
                format!(
                    "{}1 item",
                    if self.is_incoming() { '+' } else { '\u{2013}' }
                )
            }
            TxKind::KeyChange => signed(-self.fee_amount.saturating_abs(), 5),
            TxKind::Unknown => String::new(),
        }
    }

    /// The Gram amount moved, signed; 0 for an NFT or a key change.
    pub fn gram_amount(&self) -> i64 {
        match &self.kind {
            TxKind::Transfer { amount, .. } => *amount,
            TxKind::OnRampDeposit { amount, .. } => amount.saturating_abs(),
            _ => 0,
        }
    }
}

/// `tonWalletTransactions`: a page of history and the balance it was read
/// with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TxPage {
    pub gram_amount: i64,
    pub transactions: Vec<WalletTx>,
    pub next_offset: String,
}

impl TxPage {
    pub fn parse(value: &Value) -> Self {
        Self {
            gram_amount: int(value, "gram_amount"),
            transactions: value
                .get("transactions")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(WalletTx::parse).collect())
                .unwrap_or_default(),
            next_offset: text(value, "next_offset"),
        }
    }
}

/// The other side of a transaction for the title (iOS
/// `walletTransactionCounterparty`): the domain, else the address as
/// "EQAB…wxyz", else "Unknown Address". A known user's name is the
/// caller's job.
pub fn counterparty(peer_address: &str, peer_domain: &str) -> String {
    let domain = peer_domain.trim();
    if !domain.is_empty() {
        return domain.to_string();
    }
    let chars: Vec<char> = peer_address.chars().collect();
    if chars.is_empty() {
        return "Unknown Address".into();
    }
    const EDGE: usize = 4;
    if chars.len() <= EDGE * 2 {
        return peer_address.to_string();
    }
    let head: String = chars.iter().take(EDGE).collect();
    let tail: String = chars
        .iter()
        .skip(chars.len().saturating_sub(EDGE))
        .collect();
    format!("{head}\u{2026}{tail}")
}
