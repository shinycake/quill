//! What goes through the tunnel after the handshake (tdesktop
//! `cable_core.cpp` `BuildMakeCredentialRequest`,
//! `BuildGetAssertionRequest`, `ParsePostHandshakeMessage`;
//! `cable_ceremony.cpp` `SendCtapRequest`, `HandleReply`).
//!
//! The phone speaks first: a CBOR map with its `getInfo` and features.
//! Quill then sends one CTAP2 command (in protocol revision 1 behind a
//! message-type byte) and waits for the answer, skipping updates.
#![deny(clippy::indexing_slicing)]

use crate::webauthn::ctap2::cbor::{
    as_array, as_bytes, as_text, bytes, encode, get_int_key, int, int_entry, map, text, text_entry,
};
use crate::webauthn::ctap2::command;
use crate::webauthn::options::{COSE_ES256, CreationOptions, RequestOptions};
use ciborium::Value;

/// CBOR nesting Telegram Desktop accepts (`kMaxCborDepth`).
const MAX_DEPTH: usize = 16;

/// The first byte of a message in protocol revision 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Shutdown = 0,
    Ctap = 1,
    Update = 2,
    Json = 3,
}

impl MessageType {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Shutdown),
            1 => Some(Self::Ctap),
            2 => Some(Self::Update),
            3 => Some(Self::Json),
            _ => None,
        }
    }
}

/// Decode exactly one CBOR item filling all of `data`.
pub fn decode_exact(data: &[u8]) -> Option<Value> {
    let mut rest = data;
    let value: Value = ciborium::de::from_reader_with_recursion_limit(&mut rest, MAX_DEPTH).ok()?;
    rest.is_empty().then_some(value)
}

/// `authenticatorMakeCredential` for the phone: every algorithm the
/// server offers (ES256 when it offers none), a discoverable credential
/// and user verification (tdesktop `RegisterViaCable`).
pub fn make_credential_request(
    client_data_hash: &[u8],
    options: &CreationOptions,
) -> Option<Vec<u8>> {
    let algorithms = if options.algorithms.is_empty() {
        vec![COSE_ES256]
    } else {
        options.algorithms.clone()
    };
    let params = algorithms
        .iter()
        .map(|alg| {
            Value::Map(vec![
                text_entry("alg", int(*alg)),
                text_entry("type", text("public-key")),
            ])
        })
        .collect();
    let request = Value::Map(vec![
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
        int_entry(4, Value::Array(params)),
        int_entry(
            7,
            Value::Map(vec![
                text_entry("rk", Value::Bool(true)),
                text_entry("uv", Value::Bool(true)),
            ]),
        ),
    ]);
    let mut out = vec![command::MAKE_CREDENTIAL];
    out.extend(encode(&request)?);
    Some(out)
}

/// `authenticatorGetAssertion` for the phone: the allow list and both
/// user presence and verification (tdesktop `LoginViaCable`).
pub fn get_assertion_request(client_data_hash: &[u8], options: &RequestOptions) -> Option<Vec<u8>> {
    let mut request = vec![
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
        request.push(int_entry(3, Value::Array(list)));
    }
    request.push(int_entry(
        5,
        Value::Map(vec![
            text_entry("up", Value::Bool(true)),
            text_entry("uv", Value::Bool(true)),
        ]),
    ));
    let mut out = vec![command::GET_ASSERTION];
    out.extend(encode(&Value::Map(request))?);
    Some(out)
}

/// The phone's first message after the handshake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostHandshake {
    pub get_info: Vec<u8>,
    /// 1 when the message is plain CBOR, 0 when it is padded (older
    /// phones); revision 1 puts a type byte in front of every message.
    pub protocol_revision: u8,
    pub supports_ctap: bool,
}

/// A CBOR map with padding after it: a two-byte little-endian count, or
/// the older single-byte one (tdesktop `DecodePaddedCborMap`).
fn decode_padded_map(data: &[u8]) -> Option<Value> {
    let is_map = |value: &Value| matches!(value, Value::Map(_));
    if let [.., lo, hi] = data {
        let padding = usize::from(u16::from_le_bytes([*lo, *hi]));
        if let Some(len) = data
            .len()
            .checked_sub(padding)
            .and_then(|n| n.checked_sub(2))
            && let Some(value) = data.get(..len).and_then(decode_exact)
            && is_map(&value)
        {
            return Some(value);
        }
    }
    let padding = usize::from(*data.last()?);
    let len = data.len().checked_sub(padding)?.checked_sub(1)?;
    data.get(..len).and_then(decode_exact).filter(is_map)
}

/// tdesktop `ParsePostHandshakeMessage`.
pub fn parse_post_handshake(plain: &[u8]) -> Option<PostHandshake> {
    let (parsed, protocol_revision) = match decode_exact(plain) {
        Some(value @ Value::Map(_)) => (value, 1),
        _ => (decode_padded_map(plain)?, 0),
    };
    let entries = map(&parsed)?;
    let mut supports_ctap = true;
    if let Some(features) = get_int_key(entries, 3) {
        let features = as_array(features)?;
        supports_ctap = features
            .iter()
            .any(|feature| as_text(feature) == Some("ctap"));
    }
    let get_info = match get_int_key(entries, 1) {
        Some(value) => as_bytes(value)?.to_vec(),
        None => Vec::new(),
    };
    Some(PostHandshake {
        get_info,
        protocol_revision,
        supports_ctap,
    })
}

/// One decrypted message from the phone while Quill waits for its
/// answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// An update; keep waiting.
    Update,
    /// The CTAP2 answer: the status byte, then CBOR.
    Ctap(Vec<u8>),
    /// Anything else ends the ceremony.
    Unexpected,
}

/// Sort a message by its type byte (revision 1) or take it as the answer.
pub fn classify_reply(plain: Vec<u8>, protocol_revision: u8) -> Reply {
    if protocol_revision == 0 {
        return Reply::Ctap(plain);
    }
    let Some((&kind, rest)) = plain.split_first() else {
        return Reply::Unexpected;
    };
    match MessageType::from_byte(kind) {
        Some(MessageType::Update) => Reply::Update,
        Some(MessageType::Ctap) => Reply::Ctap(rest.to_vec()),
        _ => Reply::Unexpected,
    }
}

/// The CBOR of a successful answer; `Err` with the status otherwise.
pub fn ctap_answer(payload: &[u8]) -> Result<Value, Option<u8>> {
    let (&status, body) = payload.split_first().ok_or(None)?;
    if status != 0 {
        return Err(Some(status));
    }
    decode_exact(body).ok_or(None)
}

/// The message to send: the CTAP request, behind the type byte in
/// revision 1.
pub fn frame_request(request: &[u8], protocol_revision: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(request.len().saturating_add(1));
    if protocol_revision >= 1 {
        out.push(MessageType::Ctap as u8);
    }
    out.extend_from_slice(request);
    out
}
