//! The read-only wallet requests in flight and the TDLib objects they
//! answer with. Both ride inside the payments domain
//! (`PaymentsPurpose::TonWallet`, `PaymentsPayload::TonWallet`), the way
//! gift management does.

use super::own::{NftPage, OwnWallet, TxPage, WalletTx};
use serde_json::Value;

/// A wallet request in flight. Every one of them only reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TonWalletPurpose {
    /// `loadTonWalletState`; answered by `ok`, the state comes as
    /// `updateTonWalletState`.
    LoadState,
    /// `getCurrencyExchangeRates`.
    ExchangeRates,
    /// `getTonWalletTransactions`; `append` = a follow-up page.
    Transactions { append: bool },
    /// `getTonWalletNfts`; `append` = a follow-up page.
    Nfts { append: bool },
    /// `getUserTonWalletAddresses` for one profile (the user id is on the
    /// pending request).
    UserAddress,
    /// `getTonWalletTransaction` for a transfer card that was clicked.
    Transaction,
}

/// What the wallet requests and updates bring.
#[derive(Debug, Clone, PartialEq)]
pub enum TonWalletPayload {
    /// `updateTonWalletState`.
    State(OwnWallet),
    /// `tonWalletTransactions`.
    Transactions(TxPage),
    /// `tonWalletTransaction`.
    Transaction(Box<WalletTx>),
    /// `tonNfts`.
    Nfts(NftPage),
    /// `userTonWalletAddresses`: `(user_id, address)` pairs; users without
    /// a wallet are left out by TDLib.
    UserAddresses(Vec<(i64, String)>),
    /// `currencyExchangeRates`: `(code, units per USD)`.
    ExchangeRates(Vec<(String, f64)>),
}

/// The wallet's TDLib types; `None` leaves `type_name` to the others.
pub fn parse_payload(type_name: &str, value: &Value) -> Option<TonWalletPayload> {
    Some(match type_name {
        "updateTonWalletState" => {
            TonWalletPayload::State(OwnWallet::parse(value.get("state").unwrap_or(&Value::Null)))
        }
        "tonWalletTransactions" => TonWalletPayload::Transactions(TxPage::parse(value)),
        "tonWalletTransaction" => TonWalletPayload::Transaction(Box::new(WalletTx::parse(value)?)),
        "tonNfts" => TonWalletPayload::Nfts(NftPage::parse(value)),
        "userTonWalletAddresses" => TonWalletPayload::UserAddresses(
            value
                .get("addresses")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|entry| {
                            let user_id = match entry.get("user_id") {
                                Some(Value::Number(n)) => n.as_i64()?,
                                Some(Value::String(s)) => s.parse().ok()?,
                                _ => return None,
                            };
                            let address = entry.get("wallet_address")?.as_str()?.trim();
                            (user_id != 0 && !address.is_empty())
                                .then(|| (user_id, address.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default(),
        ),
        "currencyExchangeRates" => {
            TonWalletPayload::ExchangeRates(super::fiat::parse_exchange_rates(value))
        }
        _ => return None,
    })
}
