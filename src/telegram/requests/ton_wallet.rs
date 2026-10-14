//! The read-only TON wallet requests (`crate::ton_wallet::READ_ONLY_METHODS`).
//! None of them needs the wallet's key, and every one but
//! `getCurrencyExchangeRates` is gated on `can_use_ton_wallet`.
use crate::ids::RequestId;
use serde_json::json;

/// `loadTonWalletState`: the state arrives as `updateTonWalletState`.
pub fn load_ton_wallet_state(extra: RequestId) -> String {
    json!({"@type": "loadTonWalletState", "@extra": extra.as_extra()}).to_string()
}

/// `getTonWalletTransactions` for every direction (`direction` null).
pub fn get_ton_wallet_transactions(extra: RequestId, offset: &str, limit: i32) -> String {
    json!({
        "@type": "getTonWalletTransactions",
        "@extra": extra.as_extra(),
        "direction": null,
        "offset": offset,
        "limit": limit,
    })
    .to_string()
}

/// `getTonWalletTransaction` by id (a transfer message's `transaction_id`).
pub fn get_ton_wallet_transaction(extra: RequestId, transaction_id: &str) -> String {
    json!({
        "@type": "getTonWalletTransaction",
        "@extra": extra.as_extra(),
        "transaction_id": transaction_id,
    })
    .to_string()
}

/// `getTonWalletNfts`; `limit` is at most 20.
pub fn get_ton_wallet_nfts(extra: RequestId, offset: &str, limit: i32) -> String {
    json!({
        "@type": "getTonWalletNfts",
        "@extra": extra.as_extra(),
        "offset": offset,
        "limit": limit.clamp(1, 20),
    })
    .to_string()
}

/// `getUserTonWalletAddresses` for the users whose profiles are open.
pub fn get_user_ton_wallet_addresses(extra: RequestId, user_ids: &[i64]) -> String {
    json!({
        "@type": "getUserTonWalletAddresses",
        "@extra": extra.as_extra(),
        "user_ids": user_ids,
    })
    .to_string()
}

/// `getCurrencyExchangeRates`: each currency's rate to USD.
pub fn get_currency_exchange_rates(extra: RequestId) -> String {
    json!({"@type": "getCurrencyExchangeRates", "@extra": extra.as_extra()}).to_string()
}
