//! The WebAuthn options TDLib returns, parsed like Telegram Desktop's
//! `data/data_passkey_deserialize.cpp` (`DeserializeRegisterData`,
//! `DeserializeLoginData`), and the `clientDataJSON` it builds.
#![deny(clippy::indexing_slicing)]

use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use serde_json::Value;

/// The timeout when the options have none (tdesktop `toInt(60000)`).
pub const DEFAULT_TIMEOUT_MS: u32 = 60_000;

/// The origin Telegram's passkeys belong to (tdesktop `SerializeClientData`).
pub const ORIGIN: &str = "https://telegram.org";

/// COSE algorithm identifiers (tdesktop uses libfido2's `COSE_*`).
pub const COSE_ES256: i64 = -7;
pub const COSE_EDDSA: i64 = -8;
pub const COSE_RS256: i64 = -257;

/// `PublicKeyCredentialCreationOptions` (`getPasskeyParameters`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreationOptions {
    pub rp_id: String,
    pub rp_name: String,
    pub user_id: Vec<u8>,
    pub user_name: String,
    pub user_display_name: String,
    pub challenge: Vec<u8>,
    /// `pubKeyCredParams[].alg`, in the server's order.
    pub algorithms: Vec<i64>,
    pub timeout_ms: u32,
}

/// `userVerification` of the request options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UserVerification {
    Required,
    Discouraged,
    Preferred,
    /// Missing or any other value. libfido2 omits `uv` for it as for
    /// `preferred` (`FIDO_OPT_OMIT`); Windows asks for `DISCOURAGED`
    /// (tdesktop `webauthn_win.cpp` `LoginViaNative`).
    #[default]
    Unspecified,
}

/// `PublicKeyCredentialRequestOptions` (`getAuthenticationPasskeyParameters`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestOptions {
    pub rp_id: String,
    pub challenge: Vec<u8>,
    /// `allowCredentials[].id`.
    pub allow_credentials: Vec<Vec<u8>>,
    pub user_verification: UserVerification,
    pub timeout_ms: u32,
}

/// Base64 in either alphabet, padded or not. tdesktop decodes `user.id`
/// as standard base64 and the challenge as base64url; one forgiving
/// decoder reads both the same way for every input either accepts.
pub fn decode_base64(text: &str) -> Vec<u8> {
    const LENIENT: GeneralPurpose = GeneralPurpose::new(
        &alphabet::URL_SAFE,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    let normalized: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '+' => '-',
            '/' => '_',
            other => other,
        })
        .collect();
    LENIENT.decode(normalized).unwrap_or_default()
}

/// Base64url without padding (Qt `Base64UrlEncoding | OmitTrailingEquals`).
pub fn encode_base64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Qt's `QJsonValue::toInt(default)`: a whole number in `i32`, else the
/// default.
fn qt_int(value: Option<&Value>, default: i64) -> i64 {
    value
        .and_then(Value::as_f64)
        .filter(|v| v.fract() == 0.0 && *v >= f64::from(i32::MIN) && *v <= f64::from(i32::MAX))
        .map_or(default, |v| v as i64)
}

/// The timeout of the options; zero or negative means the default
/// (tdesktop `timeout > 0 ? timeout : kDefaultTimeout`).
fn timeout(public_key: &Value) -> u32 {
    u32::try_from(qt_int(
        public_key.get("timeout"),
        i64::from(DEFAULT_TIMEOUT_MS),
    ))
    .ok()
    .filter(|t| *t > 0)
    .unwrap_or(DEFAULT_TIMEOUT_MS)
}

/// The non-empty `publicKey` object of the JSON, or `None`.
fn public_key(json: &str) -> Option<Value> {
    let root: Value = serde_json::from_str(json).ok()?;
    let key = root.get("publicKey")?;
    key.as_object()
        .filter(|object| !object.is_empty())
        .map(|_| key.clone())
}

impl CreationOptions {
    /// tdesktop `DeserializeRegisterData`: `None` unless the JSON is an
    /// object with a non-empty `publicKey`; missing fields stay empty.
    pub fn parse(json: &str) -> Option<Self> {
        let key = public_key(json)?;
        let rp = key.get("rp").cloned().unwrap_or(Value::Null);
        let user = key.get("user").cloned().unwrap_or(Value::Null);
        let algorithms = key
            .get("pubKeyCredParams")
            .and_then(Value::as_array)
            .map(|params| {
                params
                    .iter()
                    .map(|param| qt_int(param.get("alg"), 0))
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            rp_id: text(&rp, "id"),
            rp_name: text(&rp, "name"),
            user_id: decode_base64(&text(&user, "id")),
            user_name: text(&user, "name"),
            user_display_name: text(&user, "displayName"),
            challenge: decode_base64(&text(&key, "challenge")),
            algorithms,
            timeout_ms: timeout(&key),
        })
    }

    /// The one algorithm asked of the key (tdesktop `PreferredAlgorithm`):
    /// ES256 when offered, else the first EdDSA or RS256, else ES256.
    pub fn preferred_algorithm(&self) -> i64 {
        if self.algorithms.contains(&COSE_ES256) {
            return COSE_ES256;
        }
        self.algorithms
            .iter()
            .copied()
            .find(|alg| *alg == COSE_EDDSA || *alg == COSE_RS256)
            .unwrap_or(COSE_ES256)
    }

    /// `clientDataJSON` for this registration.
    pub fn client_data_json(&self) -> String {
        client_data_json("webauthn.create", &self.challenge)
    }
}

impl RequestOptions {
    /// tdesktop `DeserializeLoginData`.
    pub fn parse(json: &str) -> Option<Self> {
        let key = public_key(json)?;
        let allow_credentials = key
            .get("allowCredentials")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|credential| decode_base64(&text(credential, "id")))
                    .collect()
            })
            .unwrap_or_default();
        let user_verification = match text(&key, "userVerification").as_str() {
            "required" => UserVerification::Required,
            "discouraged" => UserVerification::Discouraged,
            "preferred" => UserVerification::Preferred,
            _ => UserVerification::Unspecified,
        };
        Some(Self {
            rp_id: text(&key, "rpId"),
            challenge: decode_base64(&text(&key, "challenge")),
            allow_credentials,
            user_verification,
            timeout_ms: timeout(&key),
        })
    }

    /// `clientDataJSON` for this sign-in.
    pub fn client_data_json(&self) -> String {
        client_data_json("webauthn.get", &self.challenge)
    }
}

/// tdesktop `SerializeClientData`: a `QJsonObject` written compact, so
/// the keys come out sorted: `challenge`, `crossOrigin`, `origin`,
/// `type`. The challenge is base64url without padding, which never needs
/// escaping, and the other values are fixed ASCII.
pub fn client_data_json(kind: &str, challenge: &[u8]) -> String {
    format!(
        "{{\"challenge\":\"{}\",\"crossOrigin\":false,\"origin\":\"{ORIGIN}\",\"type\":\"{kind}\"}}",
        encode_base64url(challenge)
    )
}

#[cfg(test)]
#[path = "options_tests.rs"]
mod tests;
