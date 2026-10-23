//! Stripe card tokenization: tdesktop's `stripe/stripe_api_client.cpp`,
//! `stripe_form_encoder.cpp`, `stripe_card_params.cpp`, `stripe_token.cpp`
//! and `stripe_error.cpp`. One `POST /v1/tokens` with the provider's
//! publishable key; the answer's token id becomes the credentials.
#![deny(clippy::indexing_slicing)]

use super::transport::{HttpRequest, HttpResponse};
use super::validate::CardBrand;
use super::{CardDetails, NewCredentials, ProviderError, card_title};
use serde_json::Value;
use std::fmt::Write as _;
use zeroize::Zeroizing;

const TOKENS_URL: &str = "https://api.stripe.com/v1/tokens";
const API_VERSION: &str = "2015-10-12";
/// tdesktop sends the iOS SDK's agent details, keys sorted as Qt writes
/// them.
const USER_AGENT_DETAILS: &str = r#"{"bindings_version":"9.1.0","lang":"objective-c"}"#;

/// `QUrl::toPercentEncoding`: everything but `A-Za-z0-9-._~`.
fn percent_encode(value: &str, out: &mut String) {
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
}

/// `FormEncoder::formEncodedDataForObject(CardParams)`: the non-empty
/// `card[...]` fields, sorted by key, percent-encoded.
pub fn form_body(card: &CardDetails) -> Zeroizing<String> {
    let month = Zeroizing::new(card.expiry_month.to_string());
    let year = Zeroizing::new(card.expiry_year.to_string());
    let fields: [(&str, &str); 7] = [
        ("address_country", &card.address_country),
        ("address_zip", &card.address_zip),
        ("cvc", &card.cvc),
        ("exp_month", &month),
        ("exp_year", &year),
        ("name", &card.cardholder_name),
        ("number", &card.number),
    ];
    let capacity = fields
        .iter()
        .map(|(key, value)| key.len() * 3 + value.len() * 3 + 16)
        .sum();
    let mut body = Zeroizing::new(String::with_capacity(capacity));
    for (key, value) in fields.iter().filter(|(_, value)| !value.is_empty()) {
        if !body.is_empty() {
            body.push('&');
        }
        percent_encode(&format!("card[{key}]"), &mut body);
        body.push('=');
        percent_encode(value, &mut body);
    }
    body
}

/// The token request for `publishable_key`.
pub fn token_request(publishable_key: &str, card: &CardDetails) -> HttpRequest {
    let body = form_body(card);
    HttpRequest {
        url: TOKENS_URL.to_string(),
        headers: vec![
            ("X-Stripe-User-Agent", USER_AGENT_DETAILS.to_string()),
            ("Stripe-Version", API_VERSION.to_string()),
            ("Authorization", format!("Bearer {publishable_key}")),
            (
                "Content-Type",
                "application/x-www-form-urlencoded".to_string(),
            ),
        ],
        body: Zeroizing::new(body.as_bytes().to_vec()),
    }
}

/// `Error::DecodedObjectFromResponse`: the `error` object Stripe sends
/// with a failed request, as the error ids tdesktop's checkout maps.
fn decode_error(object: &Value) -> Option<ProviderError> {
    let error = object.get("error")?.as_object()?;
    let text = |key: &str| error.get(key).and_then(Value::as_str).unwrap_or("");
    let (kind, message) = (text("type"), text("message"));
    if kind.is_empty() || message.is_empty() {
        return Some(ProviderError::new("GenericError"));
    }
    if kind != "card_error" {
        return Some(match kind {
            "api_error" | "invalid_request_error" => ProviderError::new("GenericError"),
            other => ProviderError::new(other),
        });
    }
    let id = match text("code") {
        "incorrect_number" => "IncorrectNumber",
        "invalid_number" => "InvalidNumber",
        "invalid_expiry_month" => "InvalidExpiryMonth",
        "invalid_expiry_year" => "InvalidExpiryYear",
        "invalid_cvc" => "InvalidCVC",
        "expired_card" => "ExpiredCard",
        "incorrect_cvc" => "IncorrectCVC",
        "card_declined" => "CardDeclined",
        "processing_error" => "ProcessingError",
        other => other,
    };
    Some(ProviderError::new(id))
}

/// The answer to [`token_request`]: credentials `{"id":…,"type":"card"}`
/// titled like the server titles saved cards ("visa *4242"), or the error
/// id.
pub fn parse_token_response(response: &HttpResponse) -> Result<NewCredentials, ProviderError> {
    let document = super::parse_body(&response.body)?;
    if let Some(error) = document.as_ref().and_then(decode_error) {
        return Err(error);
    }
    if !response.is_success() {
        return Err(ProviderError::request(response.status));
    }
    let token = document.unwrap_or(Value::Null);
    let has = |key: &str| token.get(key).is_some();
    let id = token.get("id").and_then(Value::as_str).unwrap_or("");
    if id.is_empty() || !has("livemode") || !has("created") {
        return Err(ProviderError::new("InvalidTokenJson"));
    }
    let card = token.get("card");
    let card_text = |key: &str| {
        card.and_then(|card| card.get(key))
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let brand = CardBrand::from_stripe_name(card_text("brand"));
    Ok(NewCredentials::new(
        card_title(brand.title_name(), card_text("last4")),
        serde_json::json!({ "id": id, "type": "card" }).to_string(),
    ))
}
