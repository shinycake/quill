//! Generated-once classification of every `TonWalletPurpose` variant.
use super::RequestKind;
use crate::ton_wallet::payload::TonWalletPurpose;

impl TonWalletPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::LoadState
            | Self::ExchangeRates
            | Self::Transactions { .. }
            | Self::Nfts { .. }
            | Self::UserAddress
            | Self::Transaction => RequestKind::Lookup,
        }
    }
}
