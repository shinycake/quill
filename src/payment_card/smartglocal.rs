//! Smart Glocal card tokenization: tdesktop's
//! `smartglocal/smartglocal_api_client.cpp`, `smartglocal_token.cpp`,
//! `smartglocal_card.cpp` and `smartglocal_error.cpp`. One JSON `POST` to
//! the provider's tokenize URL with its public token.
#![deny(clippy::indexing_slicing)]

use super::transport::{HttpRequest, HttpResponse};
use super::{CardDetails, NewCredentials, ProviderError, card_title};
use serde_json::Value;
use std::fmt::Write as _;
use zeroize::Zeroizing;

const HOST_SUFFIX: &str = ".smart-glocal.com";
const TOKENIZE_PATH: &str = "/cds/v1/tokenize/card";

/// `ComputeApiUrl`: the form's `tokenize_url` only when it is exactly an
/// https URL on a smart-glocal.com host with the tokenize path; otherwise
/// the provider's own production or test host.
pub fn tokenize_url(configured: &str, is_test: bool) -> String {
    let valid = configured
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(host, path)| {
            host.len() > HOST_SUFFIX.len()
                && host.ends_with(HOST_SUFFIX)
                && host
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
                && format!("/{path}") == TOKENIZE_PATH
        });
    if valid {
        return configured.to_string();
    }
    let host = if is_test {
        "tgb-playground.smart-glocal.com"
    } else {
        "tgb.smart-glocal.com"
    };
    format!("https://{host}{TOKENIZE_PATH}")
}

/// tdesktop's `ToJson(CardParams)`: number, two-digit month and year, and
/// the security code, keys sorted as Qt writes them. Every value is digits,
/// so nothing needs escaping.
pub fn json_body(card: &CardDetails) -> Zeroizing<String> {
    let mut body = Zeroizing::new(String::with_capacity(
        128 + card.number.len() + card.cvc.len(),
    ));
    let _ = write!(
        body,
        r#"{{"card":{{"expiration_month":"{:02}","expiration_year":"{:02}","number":"{}","security_code":"{}"}}}}"#,
        card.expiry_month,
        card.expiry_year % 100,
        card.number.as_str(),
        card.cvc.as_str(),
    );
    body
}

/// The tokenize request for `public_token`.
pub fn token_request(
    public_token: &str,
    configured_url: &str,
    is_test: bool,
    card: &CardDetails,
) -> HttpRequest {
    let body = json_body(card);
    HttpRequest {
        url: tokenize_url(configured_url, is_test),
        headers: vec![
            ("Content-Type", "application/json".to_string()),
            ("X-PUBLIC-TOKEN", public_token.to_string()),
        ],
        body: Zeroizing::new(body.as_bytes().to_vec()),
    }
}

/// `Error::DecodedObjectFromResponse`: anything but `"status":"ok"` is an
/// error, named by its `code` when it has one.
fn decode_error(object: &Value) -> Option<ProviderError> {
    if object.get("status").and_then(Value::as_str) == Some("ok") {
        return None;
    }
    let error = object.get("error").and_then(Value::as_object);
    let text = |key: &str| {
        error
            .and_then(|error| error.get(key))
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let (code, description) = (text("code"), text("description"));
    if code.is_empty() || description.is_empty() {
        return Some(ProviderError::new("GenericError"));
    }
    Some(ProviderError::new(code))
}

/// `Last4`: the four digits ending a masked number like "4242 42** **** 4242".
fn last4(masked: &str) -> &str {
    let tail_start = masked
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map_or(masked.len(), |(index, _)| index);
    let tail = masked.get(tail_start..).unwrap_or("");
    if tail_start == 0 || tail.len() < 4 {
        return "";
    }
    tail.get(tail.len() - 4..).unwrap_or("")
}

/// The answer to [`token_request`]: credentials `{"token":…,"type":"card"}`
/// titled "visa *4242", or the error id.
pub fn parse_token_response(response: &HttpResponse) -> Result<NewCredentials, ProviderError> {
    let document = super::parse_body(&response.body)?;
    if let Some(error) = document.as_ref().and_then(decode_error) {
        return Err(error);
    }
    if !response.is_success() {
        return Err(ProviderError::request(response.status));
    }
    let data = document.as_ref().and_then(|doc| doc.get("data"));
    let token = data
        .and_then(|data| data.get("token"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if token.is_empty() {
        return Err(ProviderError::new("InvalidTokenJson"));
    }
    let info = data.and_then(|data| data.get("info"));
    let info_text = |key: &str| {
        info.and_then(|info| info.get(key))
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    let (kind, masked) = (info_text("card_type"), info_text("masked_card_number"));
    let (kind, digits) = if kind.is_empty() || masked.is_empty() {
        (String::new(), "")
    } else {
        (kind.to_lowercase(), last4(masked))
    };
    Ok(NewCredentials::new(
        card_title(&kind, digits),
        serde_json::json!({ "token": token, "type": "card" }).to_string(),
    ))
}
