//! `authenticatorGetInfo` (CTAP 2.1 section 6.4): what Quill reads of it.
#![deny(clippy::indexing_slicing)]

use super::cbor::{as_array, as_bool, as_i64, as_text, get_int_key, map};
use ciborium::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    /// `versions` (0x01): `FIDO_2_0`, `FIDO_2_1`, `U2F_V2`, ...
    pub versions: Vec<String>,
    /// `options.clientPin` (0x04): `Some(true)` once a PIN is set.
    pub client_pin: Option<bool>,
    /// `options.uv`: a built-in user verification method (fingerprint).
    pub uv: Option<bool>,
    /// `options.pinUvAuthToken`: the 2.1 token commands with permissions.
    pub pin_uv_auth_token: bool,
    /// `pinUvAuthProtocols` (0x06), in the key's order of preference.
    pub pin_protocols: Vec<i64>,
}

impl Info {
    pub fn parse(value: &Value) -> Option<Self> {
        let entries = map(value)?;
        let versions = get_int_key(entries, 0x01)
            .and_then(as_array)
            .map(|list| {
                list.iter()
                    .filter_map(as_text)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let options = get_int_key(entries, 0x04).and_then(map).unwrap_or_default();
        let option = |name: &str| super::cbor::get_text_key(options, name).and_then(as_bool);
        let pin_protocols = get_int_key(entries, 0x06)
            .and_then(as_array)
            .map(|list| list.iter().filter_map(as_i64).collect())
            .unwrap_or_default();
        Some(Self {
            versions,
            client_pin: option("clientPin"),
            uv: option("uv"),
            pin_uv_auth_token: option("pinUvAuthToken") == Some(true),
            pin_protocols,
        })
    }

    /// Whether the key speaks CTAP2 at all.
    pub fn is_fido2(&self) -> bool {
        self.versions.iter().any(|v| v.starts_with("FIDO_2_"))
    }
}
