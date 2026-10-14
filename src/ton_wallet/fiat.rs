//! The fiat value of a Gram amount, as the official iOS app computes it
//! (`WalletFiatRates.swift`, `TonFormat.swift`): the Gram's USD price is
//! the app-config `ton_usd_rate`, and `getCurrencyExchangeRates` gives how
//! many units of each currency one USD buys. The user picks the currency
//! from the iOS list; USD is the default.

use serde_json::Value;

/// The iOS wallet's currencies (`WalletContext.FiatCurrency`), in its
/// order, with the symbols it prints; a currency without one prints its
/// code.
const CURRENCIES: &[(&str, &str)] = &[
    ("USD", "$"),
    ("EUR", "€"),
    ("RUB", "₽"),
    ("CNY", "¥"),
    ("AED", ""),
    ("AFN", "؋"),
    ("ALL", ""),
    ("AMD", "֏"),
    ("ARS", ""),
    ("AUD", "A$"),
    ("AZN", "₼"),
    ("BAM", ""),
    ("BDT", "৳"),
    ("BGN", ""),
    ("BHD", ""),
    ("BND", ""),
    ("BOB", ""),
    ("BRL", "R$"),
    ("BYN", ""),
    ("CAD", "CA$"),
    ("CHF", ""),
    ("CLP", ""),
    ("COP", ""),
    ("CRC", "₡"),
    ("CZK", ""),
    ("DKK", ""),
    ("DOP", ""),
    ("DZD", ""),
    ("EGP", "E£"),
    ("ETB", ""),
    ("GBP", "£"),
    ("GEL", "₾"),
    ("GHS", "GH₵"),
    ("GTQ", ""),
    ("HKD", "HK$"),
    ("HNL", ""),
    ("HRK", ""),
    ("HUF", ""),
    ("IDR", ""),
    ("ILS", "₪"),
    ("INR", "₹"),
    ("IQD", ""),
    ("IRR", ""),
    ("ISK", ""),
    ("JMD", ""),
    ("JOD", ""),
    ("JPY", "JP¥"),
    ("KES", ""),
    ("KGS", ""),
    ("KRW", "₩"),
    ("KZT", "₸"),
    ("LBP", ""),
    ("LKR", ""),
    ("MAD", ""),
    ("MDL", ""),
    ("MMK", ""),
    ("MNT", "₮"),
    ("MOP", ""),
    ("MUR", ""),
    ("MVR", ""),
    ("MXN", "MX$"),
    ("MYR", ""),
    ("MZN", ""),
    ("NGN", "₦"),
    ("NIO", ""),
    ("NOK", ""),
    ("NPR", ""),
    ("NZD", "NZ$"),
    ("PAB", ""),
    ("PEN", ""),
    ("PHP", "₱"),
    ("PKR", ""),
    ("PLN", ""),
    ("PYG", "₲"),
    ("QAR", ""),
    ("RON", ""),
    ("RSD", ""),
    ("SAR", ""),
    ("SEK", ""),
    ("SGD", ""),
    ("SYP", ""),
    ("THB", "฿"),
    ("TJS", ""),
    ("TRY", "₺"),
    ("TTD", ""),
    ("TWD", "NT$"),
    ("TZS", ""),
    ("UAH", "₴"),
    ("UGX", ""),
    ("UYU", ""),
    ("UZS", ""),
    ("VND", "₫"),
    ("YER", ""),
    ("ZAR", ""),
];

/// The default currency (iOS `selectedFiatCurrency = .usd`).
pub const DEFAULT_CURRENCY: &str = "USD";

/// What the fiat value is printed with: the iOS symbol, or the code.
pub fn currency_symbol(code: &str) -> &str {
    CURRENCIES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(c, symbol)| if symbol.is_empty() { *c } else { *symbol })
        .unwrap_or(code)
}

/// Whether `code` is one of the iOS wallet's currencies.
pub fn is_known_currency(code: &str) -> bool {
    CURRENCIES.iter().any(|(c, _)| *c == code)
}

/// The currencies with a usable rate, in the iOS order (the picker list).
pub fn available_currencies(rates: &[(String, f64)]) -> Vec<&'static str> {
    CURRENCIES
        .iter()
        .map(|(code, _)| *code)
        .filter(|code| rate_for(rates, code).is_some())
        .collect()
}

fn rate_for(rates: &[(String, f64)], code: &str) -> Option<f64> {
    rates
        .iter()
        .find(|(c, _)| c == code)
        .map(|(_, rate)| *rate)
        .filter(|rate| rate.is_finite() && *rate > 0.0)
}

/// `currencyExchangeRates` → `(code, units per USD)`; broken entries are
/// dropped.
pub fn parse_exchange_rates(value: &Value) -> Vec<(String, f64)> {
    value
        .get("rates")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|rate| {
                    let code = rate.get("currency")?.as_str()?.trim();
                    let value = rate.get("rate")?.as_f64()?;
                    (!code.is_empty() && value.is_finite() && value > 0.0)
                        .then(|| (code.to_string(), value))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `ton_usd_rate` from a `jsonValueObject` app config (the iOS app's
/// `configuration.data["ton_usd_rate"]`); `None` when missing or not a
/// positive number.
pub fn ton_usd_rate_from_config(config: &Value) -> Option<f64> {
    config
        .get("members")
        .and_then(Value::as_array)?
        .iter()
        .find(|member| member.get("key").and_then(Value::as_str) == Some("ton_usd_rate"))
        .and_then(|member| member.get("value")?.get("value")?.as_f64())
        .filter(|rate| rate.is_finite() && *rate > 0.0)
}

/// Units of `currency` one Gram is worth (iOS `unitsPerGram`).
pub fn units_per_gram(
    ton_usd_rate: Option<f64>,
    rates: &[(String, f64)],
    currency: &str,
) -> Option<f64> {
    let per_usd = if currency == "USD" {
        rate_for(rates, currency).unwrap_or(1.0)
    } else {
        rate_for(rates, currency)?
    };
    let per_gram = per_usd * ton_usd_rate?;
    (per_gram.is_finite() && per_gram > 0.0).then_some(per_gram)
}

/// iOS `formatFiatValue`: the symbol, the integer part grouped with commas,
/// then `decimals` places ("$1,234.50", "AED12.00").
pub fn format_fiat(value: f64, symbol: &str, decimals: usize) -> String {
    let value = if value.is_finite() { value.abs() } else { 0.0 };
    let text = format!("{value:.decimals$}");
    let (integer, fraction) = match text.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (text.as_str(), None),
    };
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3);
    for (index, ch) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    match fraction {
        Some(fraction) => format!("{symbol}{grouped}.{fraction}"),
        None => format!("{symbol}{grouped}"),
    }
}

/// The fiat line under a balance (iOS `formatTonFiatValue`): two decimals,
/// none for a zero balance, "—" while no rate is known.
pub fn fiat_balance(nanos: i64, units_per_gram: Option<f64>, currency: &str) -> String {
    let Some(rate) = units_per_gram else {
        return "\u{2014}".to_string();
    };
    let grams = nanos as f64 / 1_000_000_000.0;
    format_fiat(
        grams * rate,
        currency_symbol(currency),
        if nanos == 0 { 0 } else { 2 },
    )
}
