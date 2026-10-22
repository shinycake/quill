//! Wallet links (`tg://sendgrams`, `t.me/sendgrams`, `tc://`): TDLib
//! resolves them to `internalLinkTypeTonWalletTransfer` and
//! `internalLinkTypeTonConnect`. The official iOS app opens its Send or
//! TON Connect screen, or says "Wallet is currently unavailable." when the
//! wallet is off (`OpenResolvedUrl.swift`, `.sendGrams`). Quill holds no
//! keys, so it never prefills or sends anything: with the wallet on it
//! opens the shared notice ([`super::notice`]).

use super::notice::WalletSurface;
use crate::deep_link_types::InternalLink;

/// iOS `Wallet.Unavailable`.
pub const UNAVAILABLE: &str = "Wallet is currently unavailable.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalletLink {
    /// The wallet is off for this account: say so.
    Unavailable,
    Notice(WalletSurface),
}

/// `can_use_wallet` is the `can_use_ton_wallet` option.
pub fn wallet_link(link: &InternalLink, can_use_wallet: bool) -> Option<WalletLink> {
    let InternalLink::Other(name) = link else {
        return None;
    };
    let surface = match name.as_str() {
        "TonWalletTransfer" => WalletSurface::TransferLink,
        "TonConnect" => WalletSurface::ConnectLink,
        _ => return None,
    };
    Some(if can_use_wallet {
        WalletLink::Notice(surface)
    } else {
        WalletLink::Unavailable
    })
}
