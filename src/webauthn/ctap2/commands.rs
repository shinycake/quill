//! `authenticatorMakeCredential` and `authenticatorGetAssertion` with the
//! parameters Telegram Desktop's libfido2 calls send
//! (`webauthn/webauthn_common.cpp` `RunRegisterCeremony`,
//! `RunLoginCeremony`), and the attestation object it builds.
#![deny(clippy::indexing_slicing)]

use super::cbor::{
    as_bytes, bytes, get_int_key, get_text_key, int, int_entry, map, text, text_entry,
};
use super::pin::PinToken;
use super::{Authenticator, Control, CtapError, command};
use crate::webauthn::options::{CreationOptions, RequestOptions, UserVerification};
use ciborium::Value;

/// What makeCredential returned that Quill keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MadeCredential {
    pub credential_id: Vec<u8>,
    pub auth_data: Vec<u8>,
}

/// What getAssertion returned (the first assertion, as tdesktop takes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GotAssertion {
    pub credential_id: Vec<u8>,
    pub auth_data: Vec<u8>,
    pub signature: Vec<u8>,
    pub user_handle: Vec<u8>,
}

/// The credential id inside authenticator data: rpIdHash (32), flags (1),
/// signCount (4), then with the AT flag aaguid (16), a two-byte length
/// and the id.
pub fn credential_id_from_auth_data(auth_data: &[u8]) -> Option<Vec<u8>> {
    const AT: u8 = 0x40;
    let flags = *auth_data.get(32)?;
    if flags & AT == 0 {
        return None;
    }
    let len_at = 32 + 1 + 4 + 16;
    let len = auth_data.get(len_at..len_at + 2)?;
    let len = usize::from(u16::from_be_bytes([*len.first()?, *len.get(1)?]));
    let start = len_at + 2;
    auth_data
        .get(start..start.checked_add(len)?)
        .map(<[u8]>::to_vec)
}

/// tdesktop `NoneAttestationObject`: `{"fmt": "none", "attStmt": {},
/// "authData": <authData>}` in that key order, whatever attestation the
/// key made.
pub fn none_attestation_object(auth_data: &[u8]) -> Vec<u8> {
    const PREFIX: &[u8] = &[
        0xA3, // map(3)
        0x63, b'f', b'm', b't', 0x64, b'n', b'o', b'n', b'e', // "fmt": "none"
        0x67, b'a', b't', b't', b'S', b't', b'm', b't', 0xA0, // "attStmt": {}
        0x68, b'a', b'u', b't', b'h', b'D', b'a', b't', b'a', // "authData"
    ];
    let mut out = PREFIX.to_vec();
    let len = auth_data.len();
    // The byte string header, shortest form (tdesktop writes the same for
    // every length below 64 KiB; longer authData never comes from a key).
    if let Ok(small) = u8::try_from(len) {
        if small < 24 {
            out.push(0x40 | small);
        } else {
            out.push(0x58);
            out.push(small);
        }
    } else if let Ok(medium) = u16::try_from(len) {
        out.push(0x59);
        out.extend_from_slice(&medium.to_be_bytes());
    } else {
        out.push(0x5A);
        out.extend_from_slice(&u32::try_from(len).unwrap_or(u32::MAX).to_be_bytes());
    }
    out.extend_from_slice(auth_data);
    out
}

/// makeCredential (CTAP 2.1 section 6.1): one algorithm (tdesktop
/// `fido_cred_set_type`), a discoverable credential (`rk`), user
/// verification left to the key (`FIDO_OPT_OMIT`), and the PIN token's
/// `pinUvAuthParam` when there is one.
pub fn make_credential(
    key: &mut Authenticator,
    options: &CreationOptions,
    client_data_hash: &[u8],
    token: Option<&PinToken>,
    control: &mut Control<'_>,
) -> Result<MadeCredential, CtapError> {
    let mut params = vec![
        int_entry(1, bytes(client_data_hash)),
        int_entry(
            2,
            Value::Map(vec![
                text_entry("id", text(&options.rp_id)),
                text_entry("name", text(&options.rp_name)),
            ]),
        ),
        int_entry(
            3,
            Value::Map(vec![
                text_entry("id", bytes(&options.user_id)),
                text_entry("name", text(&options.user_name)),
                text_entry("displayName", text(&options.user_display_name)),
            ]),
        ),
        int_entry(
            4,
            Value::Array(vec![Value::Map(vec![
                text_entry("alg", int(options.preferred_algorithm())),
                text_entry("type", text("public-key")),
            ])]),
        ),
        int_entry(7, Value::Map(vec![text_entry("rk", Value::Bool(true))])),
    ];
    if let Some(token) = token {
        params.push(int_entry(8, bytes(&token.param(client_data_hash))));
        params.push(int_entry(9, int(token.protocol.number())));
    }
    let answer = key.call(command::MAKE_CREDENTIAL, Some(Value::Map(params)), control)?;
    let entries = map(&answer).ok_or(CtapError::Malformed("makeCredential"))?;
    let auth_data = get_int_key(entries, 2)
        .and_then(as_bytes)
        .ok_or(CtapError::Malformed("authData"))?
        .to_vec();
    let credential_id =
        credential_id_from_auth_data(&auth_data).ok_or(CtapError::Malformed("credentialId"))?;
    Ok(MadeCredential {
        credential_id,
        auth_data,
    })
}

/// getAssertion (CTAP 2.1 section 6.2): the allow list, `uv` as the
/// request asks (tdesktop `UserVerification`: true for `required`, false
/// for `discouraged`, omitted otherwise), and the token's
/// `pinUvAuthParam` when there is one (then `uv` is not sent).
pub fn get_assertion(
    key: &mut Authenticator,
    options: &RequestOptions,
    client_data_hash: &[u8],
    token: Option<&PinToken>,
    control: &mut Control<'_>,
) -> Result<GotAssertion, CtapError> {
    let mut params = vec![
        int_entry(1, text(&options.rp_id)),
        int_entry(2, bytes(client_data_hash)),
    ];
    if !options.allow_credentials.is_empty() {
        let list = options
            .allow_credentials
            .iter()
            .map(|id| {
                Value::Map(vec![
                    text_entry("id", bytes(id)),
                    text_entry("type", text("public-key")),
                ])
            })
            .collect();
        params.push(int_entry(3, Value::Array(list)));
    }
    let uv = match options.user_verification {
        _ if token.is_some() => None,
        UserVerification::Required => Some(true),
        UserVerification::Discouraged => Some(false),
        UserVerification::Preferred | UserVerification::Unspecified => None,
    };
    if let Some(uv) = uv {
        params.push(int_entry(
            5,
            Value::Map(vec![text_entry("uv", Value::Bool(uv))]),
        ));
    }
    if let Some(token) = token {
        params.push(int_entry(6, bytes(&token.param(client_data_hash))));
        params.push(int_entry(7, int(token.protocol.number())));
    }
    let answer = key.call(command::GET_ASSERTION, Some(Value::Map(params)), control)?;
    let entries = map(&answer).ok_or(CtapError::Malformed("getAssertion"))?;
    // With exactly one allowed credential the key may leave it out.
    let credential_id = get_int_key(entries, 1)
        .and_then(map)
        .and_then(|credential| get_text_key(credential, "id"))
        .and_then(as_bytes)
        .map(<[u8]>::to_vec)
        .or_else(|| match options.allow_credentials.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        })
        .ok_or(CtapError::Malformed("credential"))?;
    let auth_data = get_int_key(entries, 2)
        .and_then(as_bytes)
        .ok_or(CtapError::Malformed("authData"))?
        .to_vec();
    let signature = get_int_key(entries, 3)
        .and_then(as_bytes)
        .ok_or(CtapError::Malformed("signature"))?
        .to_vec();
    let user_handle = get_int_key(entries, 4)
        .and_then(map)
        .and_then(|user| get_text_key(user, "id"))
        .and_then(as_bytes)
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    Ok(GotAssertion {
        credential_id,
        auth_data,
        signature,
        user_handle,
    })
}
