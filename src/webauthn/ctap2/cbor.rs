//! Small helpers over `ciborium::Value` for CTAP2's CBOR maps. Requests
//! are built with their keys already in CTAP2 canonical order (integers
//! ascending; text keys shorter first, then bytewise), which ciborium keeps.
#![deny(clippy::indexing_slicing)]

use ciborium::Value;

/// An integer-keyed map entry.
pub fn int_entry(key: i64, value: Value) -> (Value, Value) {
    (Value::Integer(key.into()), value)
}

/// A text-keyed map entry.
pub fn text_entry(key: &str, value: Value) -> (Value, Value) {
    (Value::Text(key.to_string()), value)
}

pub fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

pub fn bytes(value: &[u8]) -> Value {
    Value::Bytes(value.to_vec())
}

pub fn int(value: i64) -> Value {
    Value::Integer(value.into())
}

/// The entries of a map, or `None`.
pub fn map(value: &Value) -> Option<&[(Value, Value)]> {
    match value {
        Value::Map(entries) => Some(entries),
        _ => None,
    }
}

/// The value under integer key `key`.
pub fn get_int_key(entries: &[(Value, Value)], key: i64) -> Option<&Value> {
    entries.iter().find_map(|(k, v)| match k {
        Value::Integer(k) if i128::from(*k) == i128::from(key) => Some(v),
        _ => None,
    })
}

/// The value under text key `key`.
pub fn get_text_key<'a>(entries: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    entries.iter().find_map(|(k, v)| match k {
        Value::Text(k) if k == key => Some(v),
        _ => None,
    })
}

pub fn as_bytes(value: &Value) -> Option<&[u8]> {
    match value {
        Value::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}

pub fn as_text(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text),
        _ => None,
    }
}

pub fn as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        _ => None,
    }
}

pub fn as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(int) => i64::try_from(i128::from(*int)).ok(),
        _ => None,
    }
}

pub fn as_array(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Array(items) => Some(items),
        _ => None,
    }
}

/// Encode a value (used for the attestation object and in tests).
pub fn encode(value: &Value) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    ciborium::into_writer(value, &mut out).ok()?;
    Some(out)
}
