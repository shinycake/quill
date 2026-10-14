//! Wallet links (`tg://sendgrams`, `t.me/sendgrams`, `tc://`): TDLib
//! resolves them to `internalLinkTypeTonWalletTransfer` and
//! `internalLinkTypeTonConnect`. The official iOS app opens its Send or
//! TON Connect screen, or says "Wallet is currently unavailable." when the
//! wallet is off (`OpenResolvedUrl.swift`, `.sendGrams`). Quill holds no
//! keys, so it never prefills or sends anything: it shows a notice.

use crate::deep_link_types::InternalLink;

/// iOS `Wallet.Unavailable`.
pub const UNAVAILABLE: &str = "Wallet is currently unavailable.";

/// The notice for a "send Grams" link.
pub const TRANSFER_NOTICE: &str =
    "Quill can't send Grams. To send them, open this link in an official Telegram app.";

/// The notice for a TON Connect link.
pub const TON_CONNECT_NOTICE: &str = "Quill can't connect your wallet to apps. To continue, open this link in an official Telegram app.";

/// The text to show for a wallet link, or `None` when `link` is not one.
/// `can_use_wallet` is the `can_use_ton_wallet` option.
pub fn link_notice(link: &InternalLink, can_use_wallet: bool) -> Option<&'static str> {
    let InternalLink::Other(name) = link else {
        return None;
    };
    let notice = match name.as_str() {
        "TonWalletTransfer" => TRANSFER_NOTICE,
        "TonConnect" => TON_CONNECT_NOTICE,
        _ => return None,
    };
    Some(if can_use_wallet { notice } else { UNAVAILABLE })
}
